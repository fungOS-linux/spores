//! Behaviour of the normalized worklog: what a producer becomes, what order it
//! is read in, and what redelivery is allowed to do.

use std::path::PathBuf;

use worklog::{
    Append, Event, Kind, Store, from_github, from_marbles, signature_header, snapshot_lua,
    verify_signature,
};

/// A log path that no other test in this binary shares.
fn scratch(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("worklog-test-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("create temp dir");
    let path = directory.join(format!("{name}.jsonl"));
    let _ = std::fs::remove_file(&path);
    path
}

fn marbles(body: &str) -> Event {
    from_marbles(body.as_bytes()).expect("marbles envelope normalizes")
}

const CLOSED: &str = r#"{
  "schema": "marbles.event/1",
  "id": "fpl:7",
  "store_id": "fpl",
  "company": "fpl",
  "project": "spores",
  "issue_id": "spores-42",
  "event": "closed",
  "occurred_at": 1767225600
}"#;

#[test]
fn a_marbles_envelope_becomes_one_event() {
    let event = marbles(CLOSED);
    assert_eq!(event.schema, worklog::SCHEMA);
    assert_eq!(
        event.id, "marbles:fpl:7",
        "the envelope's (store, seq) is the identity"
    );
    assert_eq!(event.kind, Kind::Closed);
    assert_eq!(event.source, "marbles");
    assert_eq!(event.subject.as_str(), "marbles://fpl/spores-42");
    assert_eq!(
        event.at, 1_767_225_600_000,
        "seconds are normalized to milliseconds"
    );
    assert_eq!(event.links.len(), 1, "the subject is also the link to open");
    // The envelope carries no actor yet; a guess would attribute work to a
    // person who did not do it.
    assert_eq!(event.actor, "marbles");
}

#[test]
fn an_unmapped_lifecycle_event_is_an_error() {
    let body = CLOSED.replace("\"closed\"", "\"teleported\"");
    let error = from_marbles(body.as_bytes()).expect_err("unknown event must fail");
    assert!(error.contains("teleported"), "{error}");
}

#[test]
fn redelivery_does_not_append_twice() {
    let path = scratch("dedupe");
    let mut store = Store::open(&path).expect("open");

    assert!(matches!(
        store.append_at(marbles(CLOSED), 1_000).expect("append"),
        Append::Written(_)
    ));
    assert_eq!(
        store.append_at(marbles(CLOSED), 2_000).expect("reappend"),
        Append::Duplicate
    );

    let reopened = Store::open(&path).expect("reopen");
    assert_eq!(reopened.scan().expect("scan").len(), 1, "one row on disk");
}

#[test]
fn a_backwards_clock_still_sorts_by_arrival() {
    let path = scratch("clock");
    let mut store = Store::open(&path).expect("open");

    let first = marbles(CLOSED);
    store.append_at(first, 5_000).expect("first");

    // A second event arrives while the wall clock has stepped backwards.
    let second = from_marbles(
        CLOSED
            .replace("\"fpl:7\"", "\"fpl:8\"")
            .replace("spores-42", "spores-43")
            .as_bytes(),
    )
    .expect("normalize");
    store.append_at(second, 4_000).expect("second");

    let recent = store.recent(10).expect("recent");
    assert_eq!(recent.len(), 2);
    assert_eq!(
        recent[0].id, "marbles:fpl:8",
        "the later arrival is first even though the clock went backwards"
    );
    assert!(
        recent[0].hlc > recent[1].hlc,
        "ingest order is strictly increasing"
    );
}

#[test]
fn a_late_old_event_sorts_at_the_top_but_keeps_its_own_time() {
    let path = scratch("late");
    let mut store = Store::open(&path).expect("open");

    store
        .append_at(marbles(CLOSED), 10_000_000)
        .expect("now event");

    // Marbles retries a delivery from three days earlier; its sequence is newer,
    // so it is a different event that the log has never seen.
    let old = from_marbles(
        CLOSED
            .replace("\"fpl:7\"", "\"fpl:9\"")
            .replace("spores-42", "spores-44")
            .replace("1767225600", "1767000000")
            .as_bytes(),
    )
    .expect("normalize");
    store.append_at(old, 10_000_001).expect("late append");

    let recent = store.recent(10).expect("recent");
    assert_eq!(recent[0].id, "marbles:fpl:9", "newest arrival is first");
    assert_eq!(
        recent[0].at, 1_767_000_000_000,
        "the producer's own time survives, so the view can say when it happened"
    );
    assert!(recent[0].at < recent[1].at, "it really did happen earlier");
}

#[test]
fn an_interrupted_append_is_recovered_not_fatal() {
    let path = scratch("truncated");
    {
        let mut store = Store::open(&path).expect("open");
        store.append_at(marbles(CLOSED), 1_000).expect("append");
    }
    // Simulate a writer that died mid-line: no terminating newline means the
    // append was never acknowledged, so nobody is owed that event.
    let mut text = std::fs::read_to_string(&path).expect("read");
    text.push_str("{\"schema\":\"fungos.worklog/event@1\",\"id\":\"half");
    std::fs::write(&path, text).expect("write");

    let store = Store::open(&path).expect("a partial tail must not stop the store");
    assert_eq!(store.scan().expect("scan").len(), 1);
    let body = std::fs::read_to_string(&path).expect("reread");
    assert!(body.ends_with('\n'), "the partial tail is truncated");
}

#[test]
fn a_corrupt_line_in_the_middle_is_reported() {
    let path = scratch("corrupt");
    {
        let mut store = Store::open(&path).expect("open");
        store.append_at(marbles(CLOSED), 1_000).expect("append");
    }
    let mut text = std::fs::read_to_string(&path).expect("read");
    text.push_str("not json\n");
    std::fs::write(&path, text).expect("write");

    let error = Store::open(&path).expect_err("a complete but unparseable line is corruption");
    assert!(error.contains("line"), "{error}");
}

#[test]
fn signatures_cover_the_body() {
    let secret = b"shared-secret";
    let body = CLOSED.as_bytes();
    let header = signature_header(secret, body);

    assert!(verify_signature(secret, body, &header));
    assert!(!verify_signature(b"another-secret", body, &header));
    assert!(!verify_signature(secret, b"{}", &header));
    assert!(
        !verify_signature(secret, body, "deadbeef"),
        "the prefix is required"
    );
    assert!(
        !verify_signature(secret, body, "sha256=zz"),
        "non-hex is refused"
    );
}

#[test]
fn a_github_merge_becomes_a_merged_event() {
    let body = r#"{
      "action": "closed",
      "sender": {"login": "ajmwagar"},
      "repository": {"full_name": "fungOS-linux/spores"},
      "pull_request": {
        "number": 41,
        "title": "Render the worklog",
        "merged": true,
        "merged_at": "2026-01-02T03:04:05Z"
      }
    }"#;
    let event = from_github(body.as_bytes(), 999).expect("normalize");
    assert_eq!(event.kind, Kind::Merged);
    assert_eq!(event.actor, "ajmwagar");
    assert_eq!(event.subject.as_str(), "github:fungOS-linux/spores#41");
    assert_eq!(event.summary, "merged: Render the worklog");
    assert_eq!(event.id, "github:fungOS-linux/spores#41:closed");
    assert_eq!(event.at, 1_767_323_045_000);
}

#[test]
fn a_late_github_delivery_keeps_its_own_timestamp() {
    let body = r#"{
      "action": "opened",
      "sender": {"login": "someone"},
      "repository": {"full_name": "o/r"},
      "pull_request": {"number": 2, "title": "t", "updated_at": "2025-12-31T23:59:59Z"}
    }"#;
    let event = from_github(body.as_bytes(), 500_000).expect("normalize");
    assert_eq!(
        event.at, 1_767_225_599_000,
        "the payload time wins over arrival time"
    );
}

#[test]
fn a_snapshot_is_newest_first_and_hides_leases() {
    let path = scratch("snapshot");
    let mut store = Store::open(&path).expect("open");

    store.append_at(marbles(CLOSED), 1_000).expect("closed");
    let heartbeat = from_marbles(
        CLOSED
            .replace("\"closed\"", "\"heartbeat\"")
            .replace("\"fpl:7\"", "\"fpl:10\"")
            .as_bytes(),
    )
    .expect("normalize");
    store.append_at(heartbeat, 2_000).expect("heartbeat");

    let events = store.scan().expect("scan");
    let quiet = snapshot_lua(&events, 10, 1_767_300_000_000, false);
    assert!(
        !quiet.contains("lease"),
        "a heartbeat is not a timeline row"
    );
    assert!(quiet.contains("closed"), "the real event is present");
    assert!(
        quiet.contains("total = 1"),
        "the count reflects what is shown"
    );

    let loud = snapshot_lua(&events, 10, 1_767_300_000_000, true);
    assert!(loud.contains("lease"), "--all keeps it for debugging");

    // Newest arrival first: the lease row precedes the closed row.
    let lease_at = loud.find("kind = \"lease\"").expect("lease row");
    let closed_at = loud.find("kind = \"closed\"").expect("closed row");
    assert!(lease_at < closed_at, "snapshot order is newest first");
}

#[test]
fn a_quote_in_a_title_cannot_break_the_generated_lua() {
    let body = r#"{
      "action": "closed",
      "sender": {"login": "a"},
      "repository": {"full_name": "o/r"},
      "pull_request": {"number": 1, "title": "a \"quoted\" title", "merged": true, "merged_at": "2026-01-02T03:04:05Z"}
    }"#;
    let event = from_github(body.as_bytes(), 1).expect("normalize");
    let lua = snapshot_lua(&[event], 10, 1_767_300_000_000, false);
    assert!(lua.contains(r#"merged: a \"quoted\" title"#), "{lua}");
}

#[test]
fn a_locator_without_a_known_scheme_is_refused() {
    let error =
        worklog::Locator::parse("https://example.com/x").expect_err("urls are not locators");
    assert!(error.contains("scheme"), "{error}");
}
