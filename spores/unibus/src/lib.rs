//! soil:unibus Spore: a read-only AI inbox.
//!
//! The host supplies `soil:unibus` frames and `soil:records` storage. This
//! module owns the mapping from bus metadata to records, the triage state, and
//! the JSON snapshot that `ui/inbox.lua` renders. It has no sockets, clock,
//! filesystem, or credentials.

wit_bindgen::generate!({ path: "wit", world: "unibus-spore", generate_all });

use serde_json::{Value, json};
use soil::records::records::{self, Entry, Key};
use soil::unibus::bus;

const SCAN_LIMIT: u32 = 200;
const POLL_LIMIT: u32 = 64;
const KINDS: &[&str] = &[
    "email.message.v1",
    "email.delta.v1",
    "chat.message.v1",
    "chat.delta.v1",
];

struct Component;
export!(Component);

impl Guest for Component {
    fn run(args: Vec<String>) -> Result<(), String> {
        if args.first().map(String::as_str) == Some("init") {
            return seed(&args[1..]);
        }
        poll_once()
    }

    fn snapshot() -> Result<Vec<u8>, String> {
        Ok(snapshot_json()?.into_bytes())
    }
}

fn seed(args: &[String]) -> Result<(), String> {
    // account_id provider address inbox_id kind label
    if args.len() < 6 {
        return Err(
            "usage: init <account_id> <provider> <address> <inbox_id> <kind> <label>".into(),
        );
    }
    upsert(
        "account",
        &args[0],
        "account.v1",
        &json!({
            "id": args[0], "provider": args[1], "address": args[2]
        }),
    )?;
    upsert(
        "inbox",
        &format!("{}/{}", args[0], args[3]),
        "inbox.v1",
        &json!({
            "id": args[3], "account_id": args[0], "kind": args[4], "label": args[5]
        }),
    )?;
    Ok(())
}

fn poll_once() -> Result<(), String> {
    let cursor = records::get(&Key {
        collection: "sync".into(),
        id: "cursor".into(),
    })
    .ok()
    .and_then(|entry| String::from_utf8(entry.body).ok());
    let page = bus::poll(
        &KINDS
            .iter()
            .map(|kind| kind.to_string())
            .collect::<Vec<_>>(),
        cursor.as_deref(),
        POLL_LIMIT,
    )
    .map_err(|error| format!("poll: {error:?}"))?;
    for frame in page.frames {
        ingest(&frame)?;
    }
    if let Some(next) = page.next {
        upsert(
            "sync",
            "cursor",
            "sync.cursor.v1",
            &json!({ "cursor": next }),
        )?;
    }
    Ok(())
}

fn ingest(frame: &bus::Frame) -> Result<(), String> {
    let value: Value =
        serde_json::from_slice(&frame.body).map_err(|error| format!("frame body: {error}"))?;
    match frame.kind.as_str() {
        "email.message.v1" => ingest_email(&value),
        "email.delta.v1" => {
            for message in value["upserts"].as_array().cloned().unwrap_or_default() {
                ingest_email(&message)?;
            }
            Ok(())
        }
        "chat.message.v1" => ingest_chat(&value),
        "chat.delta.v1" => {
            for message in value["upserts"].as_array().cloned().unwrap_or_default() {
                ingest_chat(&message)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn ingest_email(message: &Value) -> Result<(), String> {
    let account = string(message, "account_id").unwrap_or("mail");
    let mailbox = string(message, "mailbox").unwrap_or("INBOX");
    let uidvalidity = message["uidvalidity"].as_u64().unwrap_or(0);
    let uid = message["uid"].as_u64().unwrap_or(0);
    if uid == 0 {
        return Ok(());
    }
    let id = format!("{account}/{mailbox}/{uidvalidity}/{uid}");
    upsert("email.message", &id, "email.message.v1", message)?;
    let summary = message["metadata"]["subject"]
        .as_str()
        .or_else(|| message["metadata"]["from_domain"].as_str())
        .unwrap_or("(no subject)")
        .to_string();
    upsert(
        "conversation",
        &format!("{}/{id}", inbox_id()),
        "conversation.v1",
        &json!({
            "id": id, "inbox_id": inbox_id(), "status": "new",
            "assignee": null, "priority": 0, "summary": summary
        }),
    )
}

fn ingest_chat(message: &Value) -> Result<(), String> {
    let account = string(message, "account_id").unwrap_or("chat");
    let message_id = string(message, "message_id").unwrap_or("");
    if message_id.is_empty() {
        return Ok(());
    }
    let id = format!("{account}/{message_id}");
    upsert("chat.message", &id, "chat.message.v1", message)?;
    let summary = string(message, "sender").unwrap_or("chat");
    upsert(
        "conversation",
        &format!("{}/{id}", inbox_id()),
        "conversation.v1",
        &json!({
            "id": id, "inbox_id": inbox_id(), "status": "new",
            "assignee": null, "priority": 0, "summary": summary
        }),
    )
}

fn upsert(collection: &str, id: &str, schema: &str, body: &Value) -> Result<(), String> {
    let entry = Entry {
        key: Key {
            collection: collection.into(),
            id: id.into(),
        },
        schema: schema.into(),
        revision: 0,
        updated_at_ms: 0,
        origin: String::new(),
        deleted: false,
        body: serde_json::to_vec(body).map_err(|error| error.to_string())?,
    };
    records::upsert(&entry).map_err(|error| format!("upsert {collection}: {error:?}"))?;
    Ok(())
}

fn inbox_id() -> String {
    scan("inbox")
        .first()
        .and_then(|entry| serde_json::from_slice::<Value>(&entry.body).ok())
        .and_then(|value| value["id"].as_str().map(str::to_owned))
        .unwrap_or_else(|| "support".into())
}

fn scan(collection: &str) -> Vec<Entry> {
    let mut entries = Vec::new();
    let mut cursor = None;
    while entries.len() < SCAN_LIMIT as usize {
        let Ok(page) = records::scan(collection, cursor.as_deref(), SCAN_LIMIT) else {
            break;
        };
        entries.extend(page.entries);
        cursor = page.next;
        if cursor.is_none() {
            break;
        }
    }
    entries
}

fn snapshot_json() -> Result<String, String> {
    let mut sections = Vec::new();
    for (collection, label) in [
        ("email.message", "Mail"),
        ("chat.message", "Chat"),
        ("conversation", "Support triage"),
    ] {
        let records = scan(collection)
            .iter()
            .filter_map(|entry| serde_json::from_slice::<Value>(&entry.body).ok())
            .map(|value| {
                json!({
                    "id": value["id"],
                    "title": value["metadata"]["subject"].as_str()
                        .or_else(|| value["sender"].as_str())
                        .or_else(|| value["summary"].as_str())
                        .unwrap_or("(no subject)"),
                    "summary": value["summary"].as_str()
                        .or_else(|| value["metadata"]["from_domain"].as_str())
                        .unwrap_or(""),
                    "status": value["status"],
                    "assignee": value["assignee"],
                    "unread": value["status"] == "new",
                    "urgent": value["priority"].as_u64().unwrap_or(0) >= 2,
                })
            })
            .collect::<Vec<_>>();
        if !records.is_empty() {
            sections.push(json!({"collection": collection, "label": label, "records": records}));
        }
    }
    serde_json::to_string(&json!({
        "revision": 0,
        "title": "Inbox",
        "brief": format!("{} record section(s)", sections.len()),
        "sections": sections,
    }))
    .map_err(|error| error.to_string())
}

fn string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value[key].as_str()
}
