//! Contacts over `soil:records`.
//!
//! The host owns the records book and vCard bodies. This Spore owns only the
//! contact projection and the substring search that Canvas renders.

use serde_json::{Value, json};

wit_bindgen::generate!({ path: "wit", world: "contacts-spore", generate_all });

use soil::records::records::{self, Entry};

const COLLECTION: &str = "contact";
const PREFERENCE_COLLECTION: &str = "contact_preference";
const INTERACTION_COLLECTION: &str = "contact_interaction";
const SCAN_LIMIT: u32 = 512;
const SIDE_SCAN_LIMIT: u32 = 2048;
const DISPLAY_LIMIT: usize = 120;

struct Component;
export!(Component);

impl Guest for Component {
    fn snapshot(query: String) -> Result<Vec<u8>, String> {
        let needle = query.trim().to_lowercase();
        let mut contacts = Vec::new();
        let mut organizations = std::collections::BTreeSet::new();
        let preferences = preferences();
        let interactions = interactions();
        for entry in scan(COLLECTION, SCAN_LIMIT) {
            let Ok(value) = serde_json::from_slice::<Value>(&entry.body) else {
                continue;
            };
            let name = text(&value, "display_name")
                .or_else(|| {
                    let given = text(&value, "given_name").unwrap_or_default();
                    let family = text(&value, "family_name").unwrap_or_default();
                    let combined = format!("{given} {family}").trim().to_string();
                    (!combined.is_empty()).then_some(combined)
                })
                .unwrap_or_else(|| "(no name)".into());
            let org = text(&value, "organization").unwrap_or_default();
            if !org.is_empty() {
                organizations.insert(org.clone());
            }
            let emails = values(&value, "emails");
            let phones = values(&value, "phones");
            let addresses = values(&value, "addresses");
            let matched: Vec<Value> = interactions
                .iter()
                .filter(|interaction| interaction_matches(interaction, &entry.key.id, &emails, &phones))
                .cloned()
                .collect();
            let last_contacted = matched
                .iter()
                .filter_map(|interaction| interaction.get("last_at_ms").and_then(Value::as_u64))
                .max()
                .unwrap_or(0);
            let preference = preferences.get(&entry.key.id);
            let favorite = preference
                .and_then(|value| value.get("favorite"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let tags = preference
                .and_then(|value| value.get("tags"))
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let haystack = format!(
                "{name} {org} {} {} {}",
                emails.join(" "),
                phones.join(" "),
                text(&value, "note").unwrap_or_default()
            )
            .to_lowercase();
            if !needle.is_empty() && !haystack.contains(&needle) {
                continue;
            }
            contacts.push(json!({
                "id": entry.key.id,
                "name": name,
                "org": org,
                "title": text(&value, "title").unwrap_or_default(),
                "emails": emails,
                "phones": phones,
                "addresses": addresses,
                "note": text(&value, "note").unwrap_or_default(),
                "birthday": text(&value, "birthday").unwrap_or_default(),
                "created": text(&value, "created").unwrap_or_default(),
                "last_modified": text(&value, "last_modified").unwrap_or_default(),
                "updated_at_ms": entry.revision,
                "favorite": favorite,
                "tags": tags,
                "last_contacted": last_contacted,
                "interactions": matched,
            }));
        }
        contacts.sort_by(|left, right| {
            let left_favorite = left["favorite"].as_bool().unwrap_or(false);
            let right_favorite = right["favorite"].as_bool().unwrap_or(false);
            if left_favorite != right_favorite {
                return right_favorite.cmp(&left_favorite);
            }
            let left_mru = left["last_contacted"].as_u64().unwrap_or(0);
            let right_mru = right["last_contacted"].as_u64().unwrap_or(0);
            if left_mru != right_mru {
                return right_mru.cmp(&left_mru);
            }
            left["name"].as_str().unwrap_or("").to_lowercase().cmp(
                &right["name"].as_str().unwrap_or("").to_lowercase(),
            )
        });
        let total = contacts.len();
        contacts.truncate(DISPLAY_LIMIT);
        let selected = contacts.first().and_then(|contact| contact["id"].as_str()).unwrap_or("");
        let output = json!({
            "revision": entries_revision(),
            "title": "Contacts",
            "search": query,
            "account": {"label": "Records · contacts", "sync": "live"},
            "total": total,
            "organizations": organizations.len(),
            "selected_id": selected,
            "filters": [
                {"id": "all", "label": "All people", "count": total, "selected": true},
                {"id": "org", "label": "Organizations", "count": organizations.len()},
            ],
            "contacts": contacts,
        });
        serde_json::to_vec(&output).map_err(|error| error.to_string())
    }
}

fn scan(collection: &str, limit: u32) -> Vec<Entry> {
    let mut entries = Vec::new();
    let mut cursor = None;
    while entries.len() < limit as usize {
        let Ok(page) = records::scan(collection, cursor.as_deref(), limit) else {
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

fn preferences() -> std::collections::BTreeMap<String, Value> {
    scan(PREFERENCE_COLLECTION, SIDE_SCAN_LIMIT)
        .into_iter()
        .filter_map(|entry| {
            let value = serde_json::from_slice::<Value>(&entry.body).ok()?;
            Some((entry.key.id, value))
        })
        .collect()
}

fn interactions() -> Vec<Value> {
    scan(INTERACTION_COLLECTION, SIDE_SCAN_LIMIT)
        .into_iter()
        .filter_map(|entry| {
            let value = serde_json::from_slice::<Value>(&entry.body).ok()?;
            let source = text(&value, "source").unwrap_or_else(|| "unknown".into());
            let kind = text(&value, "kind").unwrap_or_else(|| "interaction".into());
            let last_at_ms = value
                .get("last_at_ms")
                .and_then(Value::as_u64)
                .unwrap_or(entry.revision);
            let count = value.get("count").and_then(Value::as_u64).unwrap_or(1);
            Some(json!({
                "contact_id": text(&value, "contact_id").unwrap_or_default(),
                "identity_kind": text(&value, "identity_kind").unwrap_or_default(),
                "identity": text(&value, "identity").unwrap_or_default(),
                "source": source,
                "kind": kind,
                "last_at_ms": last_at_ms,
                "count": count,
            }))
        })
        .collect()
}

fn interaction_matches(interaction: &Value, contact_id: &str, emails: &[String], phones: &[String]) -> bool {
    if interaction.get("contact_id").and_then(Value::as_str) == Some(contact_id) {
        return true;
    }
    let identity = interaction.get("identity").and_then(Value::as_str).unwrap_or("");
    match interaction.get("identity_kind").and_then(Value::as_str) {
        Some("email") => emails.iter().any(|email| email.eq_ignore_ascii_case(identity)),
        Some("phone") => {
            let wanted = digits(identity);
            wanted.len() >= 7 && phones.iter().any(|phone| {
                let have = digits(phone);
                have.len() >= 7 && (have.ends_with(&wanted) || wanted.ends_with(&have))
            })
        }
        _ => false,
    }
}

fn digits(value: &str) -> String {
    value.chars().filter(char::is_ascii_digit).collect()
}

fn entries_revision() -> u64 {
    // The host owns revisions; the UI only needs a stable, monotonic-enough stamp
    // for its scene cache. Use the max record revision in the contact page.
    let mut max = 0;
    for entry in scan(COLLECTION, SCAN_LIMIT) {
        max = max.max(entry.revision);
    }
    max
}

fn text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(|value| value.trim().trim_end_matches(';').trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn values(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("value").and_then(Value::as_str))
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}
