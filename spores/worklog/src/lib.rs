//! `fungos.worklog/event@1`: one event shape for every producer.
//!
//! A worklog is a firehose of things that happened across tools — a Marble
//! closed, a pull request merged, a thread mentioned someone. Each arrives in a
//! different envelope, so the only way to have one timeline is to normalize on
//! the way in and store the normalized shape. See `docs/worklog.md`.
//!
//! Two timestamps are carried on purpose:
//!
//! * `at` is when the producer says it happened. The view prints it.
//! * `hlc` is the order it entered this log, a hybrid logical clock assigned on
//!   ingest.
//!
//! Sorting by `at` interleaves events from machines whose clocks disagree, and
//! the order changes when a producer retries an old delivery. `hlc` is
//! monotonic for one store, so "newest first" is stable and pages without
//! duplicates or gaps. A Marble that happened three days ago but arrived now
//! sorts now, which is honest: that is when it entered the log. Printing `at`
//! keeps the reader from being lied to about when it really happened.

use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Sha256;

/// The normalized event schema every producer is reduced to.
pub const SCHEMA: &str = "fungos.worklog/event@1";
/// The envelope Marbles publishes on its private webhook.
pub const MARBLES_SCHEMA: &str = "marbles.event/1";
/// The `x-marbles-signature` / `X-Hub-Signature-256` value prefix.
pub const SIGNATURE_PREFIX: &str = "sha256=";

/// Milliseconds since the Unix epoch.
#[must_use]
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default()
}

// ---- ordering -----------------------------------------------------------

/// A hybrid logical clock: physical milliseconds plus a counter for events that
/// share one millisecond.
///
/// The derived `Ord` is the whole point of the type: `(physical, counter)`
/// orders correctly and every comparison the store does uses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Hlc {
    pub physical: i64,
    pub counter: u64,
}

impl Hlc {
    #[must_use]
    pub const fn origin() -> Self {
        Self {
            physical: 0,
            counter: 0,
        }
    }

    /// Advance past `now` and past this clock, whichever is later.
    ///
    /// A backwards system clock must not produce an event that sorts before one
    /// already written, so the physical component never decreases; the counter
    /// absorbs the collision.
    #[must_use]
    pub fn tick(self, now: i64) -> Self {
        if now > self.physical {
            Self {
                physical: now,
                counter: 0,
            }
        } else {
            Self {
                physical: self.physical,
                counter: self.counter.saturating_add(1),
            }
        }
    }
}

// ---- the event ----------------------------------------------------------

/// What happened, in a vocabulary shared across producers.
///
/// Unknown names are an error, not a passthrough: a producer that invents a
/// kind should make someone decide how it renders, once, here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Created,
    Changed,
    Claimed,
    Released,
    Closed,
    Escalated,
    Dependency,
    /// A lease heartbeat. Stored, but dropped from the default view.
    Lease,
    Merged,
    Mention,
}

impl Kind {
    /// The wire name, matching the serde representation.
    ///
    /// Spelled out rather than derived from `Debug` so the log, the snapshot and
    /// the view all agree on one string.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Changed => "changed",
            Self::Claimed => "claimed",
            Self::Released => "released",
            Self::Closed => "closed",
            Self::Escalated => "escalated",
            Self::Dependency => "dependency",
            Self::Lease => "lease",
            Self::Merged => "merged",
            Self::Mention => "mention",
        }
    }

    /// Kinds a reader does not want in a timeline they skim.
    ///
    /// Kept in the log rather than filtered at ingest: the raw record is
    /// evidence, and a view is allowed to be opinionated about it.
    #[must_use]
    pub fn is_noise(self) -> bool {
        matches!(self, Self::Lease)
    }
}

/// A typed reference to a thing, not a URL.
///
/// URLs rot — a tracker move or a session rename leaves a dead link — and the
/// scheme is also the only cheap way to know what kind of thing a row points
/// at. Names must be added deliberately, which is why an unknown one fails.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Locator(String);

impl Locator {
    pub const SCHEMES: &'static [&'static str] = &["marbles://", "github:", "delta://", "vfs:"];

    pub fn parse(raw: &str) -> Result<Self, String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err("locator is empty".to_string());
        }
        if !Self::SCHEMES
            .iter()
            .any(|scheme| trimmed.starts_with(scheme))
        {
            return Err(format!(
                "locator {trimmed:?} has no known scheme (expected one of {})",
                Self::SCHEMES.join(", ")
            ));
        }
        Ok(Self(trimmed.to_string()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Locator {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A named edge from an event to something the reader can open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub rel: String,
    pub locator: Locator,
}

impl Link {
    #[must_use]
    pub fn new(rel: &str, locator: &Locator) -> Self {
        Self {
            rel: rel.to_string(),
            locator: locator.clone(),
        }
    }
}

/// One normalized worklog event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub schema: String,
    /// Producer-derived and stable: redelivery of the same event must not append
    /// twice, so this is what deduplication keys on.
    pub id: String,
    /// When the producer says it happened, in milliseconds.
    pub at: i64,
    /// Ingest order. Assigned by [`Store`], ignored if a producer sets it.
    pub hlc: Hlc,
    pub source: String,
    pub kind: Kind,
    pub actor: String,
    pub subject: Locator,
    pub summary: String,
    pub links: Vec<Link>,
    #[serde(default)]
    pub payload: Value,
}

impl Event {
    fn new(id: String, at: i64, source: &str, kind: Kind, actor: String, subject: Locator) -> Self {
        Self {
            schema: SCHEMA.to_string(),
            id,
            at,
            hlc: Hlc::origin(),
            source: source.to_string(),
            kind,
            actor,
            links: vec![Link::new("subject", &subject)],
            subject,
            summary: String::new(),
            payload: Value::Null,
        }
    }

    fn with_summary(mut self, summary: String, payload: Value) -> Self {
        self.summary = summary;
        self.payload = payload;
        self
    }

    /// Reject an event this module should never have written.
    fn check(&self) -> Result<(), String> {
        if self.schema != SCHEMA {
            return Err(format!(
                "event {} has schema {:?}, not {SCHEMA:?}",
                self.id, self.schema
            ));
        }
        if self.id.is_empty() {
            return Err("event has an empty id".to_string());
        }
        Ok(())
    }
}

// ---- producers ----------------------------------------------------------

/// Marbles' `marbles.event/1` envelope, as far as this ingester reads it.
///
/// `actor` and `detail` are absent from today's envelope and default to empty;
/// see `docs/worklog.md` for the gap and what Marbles needs to send. They are
/// optional here so that the day Marbles starts sending them this crate needs no
/// change.
#[derive(Debug, Deserialize)]
struct MarblesEnvelope {
    schema: String,
    id: String,
    company: String,
    project: String,
    issue_id: String,
    event: String,
    occurred_at: i64,
    #[serde(default)]
    actor: Option<String>,
    #[serde(default)]
    detail: Option<String>,
}

/// Map a Marbles lifecycle event name onto the shared vocabulary.
fn marbles_kind(event: &str) -> Result<Kind, String> {
    Ok(match event {
        "created" => Kind::Created,
        "updated" | "status" | "labels" => Kind::Changed,
        "claimed" => Kind::Claimed,
        "released" | "claim_expired" => Kind::Released,
        "closed" => Kind::Closed,
        "escalated" => Kind::Escalated,
        "depends_on" | "unblocked_from" | "unblocked_from_remote" => Kind::Dependency,
        "heartbeat" => Kind::Lease,
        other => return Err(format!("unmapped marbles event {other:?}")),
    })
}

pub fn from_marbles(body: &[u8]) -> Result<Event, String> {
    let envelope: MarblesEnvelope =
        serde_json::from_slice(body).map_err(|error| format!("marbles envelope JSON: {error}"))?;
    if envelope.schema != MARBLES_SCHEMA {
        return Err(format!(
            "envelope schema {:?} is not {MARBLES_SCHEMA:?}",
            envelope.schema
        ));
    }

    let kind = marbles_kind(&envelope.event)?;
    // Marbles prefixes every issue id with its project (`mycelium-6us5`), so
    // carrying the project in the locator as well printed it twice:
    // `marbles://local/mycelium/mycelium-6us5`. The id is unique within a store;
    // the project travels in the payload and in the row text.
    let subject = Locator::parse(&format!(
        "marbles://{}/{}",
        envelope.company, envelope.issue_id
    ))?;

    // The row already shows the kind in its head and the locator beneath it, so
    // repeating `closed mycelium/mycelium-6us5` in the summary printed the same
    // issue id twice per row. The detail is the part that carries information;
    // when there is none, the identity is all the row has.
    //
    // Marbles writes its detail with a leading `|` separator, which read as
    // `— | ack: …` once it landed in a sentence.
    let detail = envelope
        .detail
        .as_deref()
        .map(|detail| detail.trim().trim_start_matches('|').trim())
        .filter(|detail| !detail.is_empty());
    let summary = match detail {
        Some(detail) => detail.to_string(),
        None => format!("{} {}", envelope.event, envelope.issue_id),
    };
    let actor = match envelope.actor.as_deref() {
        Some(actor) if !actor.trim().is_empty() => actor.to_string(),
        // The envelope carries no actor today. Saying "marbles" is true; naming
        // a person would be a guess.
        _ => "marbles".to_string(),
    };

    Ok(Event::new(
        // `(store_id, seq)` is already the envelope's stable identity; reuse it
        // instead of minting a second one that could drift.
        format!("marbles:{}", envelope.id),
        // The envelope counts in seconds, the log in milliseconds.
        envelope.occurred_at.saturating_mul(1000),
        "marbles",
        kind,
        actor,
        subject,
    )
    .with_summary(
        summary,
        json!({
            "company": envelope.company,
            "project": envelope.project,
            "issue_id": envelope.issue_id,
            "event": envelope.event,
        }),
    ))
}

/// A GitHub webhook delivery, reduced to a worklog event.
///
/// Only pull requests are mapped for now: a merge is the thing a timeline is
/// for. Everything else (`push`, `issues`, `workflow_run`) is acknowledged by
/// the receiver and dropped, so adding one later is additive.
#[derive(Debug, Deserialize)]
struct GithubPullRequest {
    action: String,
    sender: GithubUser,
    pull_request: GithubPr,
    repository: GithubRepo,
}

#[derive(Debug, Deserialize)]
struct GithubUser {
    login: String,
}

#[derive(Debug, Deserialize)]
struct GithubRepo {
    full_name: String,
}

#[derive(Debug, Deserialize)]
struct GithubPr {
    number: u64,
    title: String,
    merged: Option<bool>,
    merged_at: Option<String>,
    updated_at: Option<String>,
}

pub fn from_github(body: &[u8], now: i64) -> Result<Event, String> {
    let delivery: GithubPullRequest =
        serde_json::from_slice(body).map_err(|error| format!("github payload JSON: {error}"))?;
    let pull = &delivery.pull_request;
    let subject = Locator::parse(&format!(
        "github:{}#{}",
        delivery.repository.full_name, pull.number
    ))?;

    let (kind, verb) = match (delivery.action.as_str(), pull.merged) {
        ("closed", Some(true)) => (Kind::Merged, "merged"),
        ("closed", _) => (Kind::Closed, "closed without merging"),
        ("opened" | "reopened", _) => (Kind::Changed, "opened"),
        (other, _) => return Err(format!("unmapped pull_request action {other:?}")),
    };

    let stamp = pull.merged_at.clone().or_else(|| pull.updated_at.clone());
    let (at, parsed) = match stamp.as_deref().map(rfc3339_ms) {
        Some(Ok(ms)) => (ms, true),
        // A timestamp we cannot read must not cost us the event, and must not be
        // silently replaced by "now" either: the raw value stays in the payload
        // and the caller reports it.
        _ => (now, false),
    };

    Ok(Event::new(
        format!(
            "github:{}#{}:{}",
            delivery.repository.full_name, pull.number, delivery.action
        ),
        at,
        "github",
        kind,
        delivery.sender.login.clone(),
        subject,
    )
    .with_summary(
        format!("{verb}: {}", pull.title),
        json!({
            "repository": delivery.repository.full_name,
            "number": pull.number,
            "action": delivery.action,
            "title": pull.title,
            "timestamp": stamp,
            "timestamp_parsed": parsed,
        }),
    ))
}

// ---- signatures ---------------------------------------------------------

/// Verify a `sha256=<hex>` signature header over the exact request body.
///
/// Both channels this receiver speaks — `x-marbles-signature` and GitHub's
/// `X-Hub-Signature-256` — are HMAC-SHA256 over the raw bytes with the same
/// value shape, so the check is shared rather than re-derived per producer. The
/// comparison is constant time (`verify_slice`) because a tag comparison that
/// returns early leaks the prefix.
#[must_use]
pub fn verify_signature(secret: &[u8], body: &[u8], header: &str) -> bool {
    let Some(encoded) = header.trim().strip_prefix(SIGNATURE_PREFIX) else {
        return false;
    };
    let Some(expected) = hex_decode(encoded) else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret) else {
        return false;
    };
    mac.update(body);
    mac.verify_slice(&expected).is_ok()
}

/// Sign a body the way both producers do. Exposed so a test or a replay tool can
/// produce a header the verifier accepts without reimplementing the scheme.
#[must_use]
pub fn signature_header(secret: &[u8], body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("hmac accepts any key length");
    mac.update(body);
    format!(
        "{SIGNATURE_PREFIX}{}",
        hex_encode(&mac.finalize().into_bytes())
    )
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn hex_decode(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let high = (pair[0] as char).to_digit(16)?;
        let low = (pair[1] as char).to_digit(16)?;
        out.push((high * 16 + low) as u8);
    }
    // `chunks_exact` drops a trailing odd byte; refuse the string instead of
    // silently decoding a tag that is not the one that was sent.
    (out.len() * 2 == bytes.len()).then_some(out)
}

// ---- the store ----------------------------------------------------------

/// Whether an append wrote a row or was a redelivery of one already present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Append {
    Written(Hlc),
    Duplicate,
}

/// An append-only JSONL file of normalized events.
///
/// This is the honest stand-in for the host store. Soil would keep each event as
/// a `soil:records` entry in collection `worklog.event`, with the same body; the
/// file exists because no Soil host implements records yet, and it stays
/// inspectable with `grep` in the meantime.
#[derive(Debug)]
pub struct Store {
    path: PathBuf,
    clock: Hlc,
    ids: BTreeSet<String>,
}

impl Store {
    /// Open `path`, recovering the clock and the seen ids.
    ///
    /// An append is only acknowledged after its newline is durable, so a final
    /// line with no newline is by definition an append that was never acked: the
    /// writer died mid-write. It is truncated here rather than parsed, because
    /// the producer will redeliver, and refusing to start would need a human to
    /// clear a file over a crash that already cost nothing.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref().to_path_buf();
        let mut clock = Hlc::origin();
        let mut ids = BTreeSet::new();

        if !path.exists() {
            return Ok(Self { path, clock, ids });
        }

        let file =
            File::open(&path).map_err(|error| format!("open {}: {error}", path.display()))?;
        let mut reader = BufReader::new(file);
        let mut last_complete = 0_u64;
        let mut offset = 0_u64;
        let mut line = String::new();
        loop {
            line.clear();
            let read = reader
                .read_line(&mut line)
                .map_err(|error| format!("read {}: {error}", path.display()))?;
            if read == 0 {
                break;
            }
            let complete = line.ends_with('\n');
            let body = line.trim_end_matches(['\n', '\r']);
            if !complete {
                // Interrupted append; the loop ends here and the tail is cut.
                break;
            }
            if !body.trim().is_empty() {
                let event = Self::parse(body).map_err(|error| {
                    format!("{} at line ending {offset}: {error}", path.display())
                })?;
                clock = clock.max(event.hlc);
                ids.insert(event.id);
            }
            offset += read as u64;
            last_complete = offset;
        }
        drop(reader);

        // Compare against the real file length: an unfinished tail never
        // advances `offset`, so the cursor alone cannot tell that it is there.
        let total = fs::metadata(&path)
            .map_err(|error| format!("stat {}: {error}", path.display()))?
            .len();
        if last_complete != total {
            let file = OpenOptions::new()
                .write(true)
                .open(&path)
                .map_err(|error| format!("reopen {} to truncate: {error}", path.display()))?;
            file.set_len(last_complete)
                .map_err(|error| format!("truncate {}: {error}", path.display()))?;
        }

        Ok(Self { path, clock, ids })
    }

    fn parse(line: &str) -> Result<Event, String> {
        let event: Event =
            serde_json::from_str(line).map_err(|error| format!("event JSON: {error}"))?;
        event.check()?;
        if Locator::parse(event.subject.as_str())? != event.subject {
            return Err(format!("event {} has an unparsed subject", event.id));
        }
        Ok(event)
    }

    /// Append an event, assigning its clock. Redelivery is a no-op.
    pub fn append(&mut self, event: Event) -> Result<Append, String> {
        self.append_at(event, now_ms())
    }

    /// [`Self::append`] with an injected clock, so ordering is testable.
    pub fn append_at(&mut self, mut event: Event, now: i64) -> Result<Append, String> {
        event.check()?;
        if self.ids.contains(&event.id) {
            return Ok(Append::Duplicate);
        }
        let clock = self.clock.tick(now);
        event.hlc = clock;
        let mut line = serde_json::to_string(&event)
            .map_err(|error| format!("serialize {}: {error}", event.id))?;
        line.push('\n');

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|error| format!("open {}: {error}", self.path.display()))?;
        file.write_all(line.as_bytes())
            .map_err(|error| format!("write {}: {error}", self.path.display()))?;
        // The id is only claimed once the bytes are on disk; a write that fails
        // here leaves the log as it was and the producer free to retry.
        file.sync_data()
            .map_err(|error| format!("sync {}: {error}", self.path.display()))?;

        self.clock = clock;
        self.ids.insert(event.id);
        Ok(Append::Written(clock))
    }

    /// Newest first, by ingest order.
    pub fn recent(&self, limit: usize) -> Result<Vec<Event>, String> {
        let mut events = self.scan()?;
        events.sort_by_key(|event| std::cmp::Reverse(event.hlc));
        events.truncate(limit);
        Ok(events)
    }

    /// Every event, oldest first.
    pub fn scan(&self) -> Result<Vec<Event>, String> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let text = fs::read_to_string(&self.path)
            .map_err(|error| format!("read {}: {error}", self.path.display()))?;
        let mut events = Vec::new();
        for (index, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let event = Self::parse(line)
                .map_err(|error| format!("{} line {}: {error}", self.path.display(), index + 1))?;
            events.push(event);
        }
        events.sort_by_key(|event| event.hlc);
        Ok(events)
    }
}

// ---- time ---------------------------------------------------------------

/// Format milliseconds as `YYYY-MM-DDTHH:MM:SSZ`.
///
/// UTC rather than local time: a local offset needs a time zone database, and a
/// wrong offset is worse than an explicit one. The view prints relative times,
/// which are time-zone free.
#[must_use]
pub fn iso8601_utc(ms: i64) -> String {
    let seconds = ms.div_euclid(1000);
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        (rest % 3600) / 60,
        rest % 60
    )
}

/// Parse an RFC 3339 timestamp with a `Z` suffix into milliseconds.
///
/// Deliberately narrow: GitHub publishes exactly this shape, and a general
/// parser is a dependency this receiver does not otherwise need.
pub fn rfc3339_ms(text: &str) -> Result<i64, String> {
    let trimmed = text.trim();
    let bytes = trimmed.as_bytes();
    if bytes.len() < 20 || bytes[19] != b'Z' {
        return Err(format!(
            "timestamp {trimmed:?} is not RFC 3339 with a Z suffix"
        ));
    }
    let number = |from: usize, to: usize| -> Result<i64, String> {
        trimmed
            .get(from..to)
            .and_then(|part| part.parse::<i64>().ok())
            .ok_or_else(|| format!("timestamp {trimmed:?} has a non-numeric field"))
    };
    let (year, month, day) = (number(0, 4)?, number(5, 7)?, number(8, 10)?);
    let (hour, minute, second) = (number(11, 13)?, number(14, 16)?, number(17, 19)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(format!("timestamp {trimmed:?} is out of range"));
    }
    let days = days_from_civil(year, month as u32, day as u32);
    Ok(((days * 86_400) + hour * 3600 + minute * 60 + second) * 1000)
}

// Howard Hinnant, "chrono-Compatible Low-Level Date Algorithms" (2013):
// days_from_civil/civil_from_days, the standard proleptic-Gregorian pair. Using
// them directly keeps a fixed-format timestamp parser from pulling in a calendar
// crate and a time zone database.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * i64::from(if month > 2 { month - 3 } else { month + 9 }) + 2) / 5
        + i64::from(day)
        - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

// ---- the projection -----------------------------------------------------

/// Compose an age into the shortest string that still distinguishes it.
///
/// A timeline is skimmed, not read: `3h` carries the same decision as
/// `2026-01-02T03:04:05Z` and occupies a fifth of the row.
#[must_use]
pub fn relative_time(now: i64, at: i64) -> String {
    let age = now.saturating_sub(at);
    if age < 60_000 {
        return "now".to_string();
    }
    let minutes = age / 60_000;
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h");
    }
    let days = hours / 24;
    if days < 7 {
        return format!("{days}d");
    }
    format!("{}w", days / 7)
}

fn lua_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            // A raw control byte in a Lua literal is legal but invisible in a
            // diff; escaping it keeps the generated file reviewable.
            character if character.is_control() => {
                out.push_str(&format!("\\u{{{:x}}}", character as u32));
            }
            character => out.push(character),
        }
    }
    out.push('"');
    out
}

/// Render events as the Lua snapshot `ui/worklog.lua` reads.
///
/// The view is a pure projection, so the ordering decision is made here and
/// carried in the data: `events` arrives newest first and the view paints it in
/// order without re-sorting. Noise kinds are dropped unless `include_noise`.
#[must_use]
pub fn snapshot_lua(events: &[Event], limit: usize, now: i64, include_noise: bool) -> String {
    let mut visible: Vec<&Event> = events
        .iter()
        .filter(|event| include_noise || !event.kind.is_noise())
        .collect();
    visible.sort_by_key(|event| std::cmp::Reverse(event.hlc));
    visible.truncate(limit);

    let mut sources: Vec<String> = visible.iter().map(|event| event.source.clone()).collect();
    sources.sort();
    sources.dedup();

    let today = now - now.rem_euclid(86_400_000);
    let today_count = visible.iter().filter(|event| event.at >= today).count();

    let mut out = String::new();
    out.push_str("-- generated by `worklog render-lua`; do not edit\n");
    out.push_str("return {\n");
    out.push_str("  title = \"WORKLOG\",\n");
    out.push_str(&format!("  limit = {limit},\n"));
    out.push_str(&format!(
        "  counts = {{ total = {}, today = {} }},\n",
        visible.len(),
        today_count
    ));
    out.push_str("  sources = {\n");
    for source in &sources {
        let count = visible
            .iter()
            .filter(|event| &event.source == source)
            .count();
        out.push_str(&format!(
            "    {{ id = {}, label = {}, count = {} }},\n",
            lua_string(source),
            lua_string(&title_case(source)),
            count
        ));
    }
    out.push_str("  },\n  events = {\n");
    for (index, event) in visible.iter().enumerate() {
        out.push_str("    {\n");
        out.push_str(&format!("      id = {},\n", lua_string(&event.id)));
        out.push_str(&format!(
            "      kind = {},\n",
            lua_string(event.kind.as_str())
        ));
        out.push_str(&format!("      source = {},\n", lua_string(&event.source)));
        out.push_str(&format!("      actor = {},\n", lua_string(&event.actor)));
        out.push_str(&format!(
            "      summary = {},\n",
            lua_string(&event.summary)
        ));
        out.push_str(&format!(
            "      subject = {},\n",
            lua_string(event.subject.as_str())
        ));
        out.push_str(&format!("      at = {},\n", event.at));
        out.push_str(&format!(
            "      at_text = {},\n",
            lua_string(&iso8601_utc(event.at))
        ));
        out.push_str(&format!(
            "      age = {},\n",
            lua_string(&relative_time(now, event.at))
        ));
        out.push_str(&format!("      selected = {},\n", index == 0));
        out.push_str("      links = {\n");
        for link in &event.links {
            out.push_str(&format!(
                "        {{ rel = {}, locator = {} }},\n",
                lua_string(&link.rel),
                lua_string(link.locator.as_str())
            ));
        }
        out.push_str("      },\n    },\n");
    }
    out.push_str("  },\n}\n");
    out
}

fn title_case(source: &str) -> String {
    let mut characters = source.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_never_goes_backwards() {
        let first = Hlc::origin().tick(1_000);
        assert_eq!(
            first,
            Hlc {
                physical: 1_000,
                counter: 0
            }
        );
        // A clock that steps backwards must not produce a smaller stamp.
        let second = first.tick(900);
        assert_eq!(
            second,
            Hlc {
                physical: 1_000,
                counter: 1
            }
        );
        assert!(second > first);
    }

    #[test]
    fn timestamps_round_trip() {
        let ms = rfc3339_ms("2026-01-02T03:04:05Z").expect("parses");
        assert_eq!(iso8601_utc(ms), "2026-01-02T03:04:05Z");
        assert!(rfc3339_ms("2026-01-02 03:04:05").is_err());
    }

    #[test]
    fn epoch_and_leap_day_are_right() {
        assert_eq!(iso8601_utc(0), "1970-01-01T00:00:00Z");
        let leap = rfc3339_ms("2024-02-29T12:00:00Z").expect("parses");
        assert_eq!(iso8601_utc(leap), "2024-02-29T12:00:00Z");
    }
}
