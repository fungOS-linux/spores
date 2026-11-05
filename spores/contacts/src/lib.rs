//! Contacts over `soil:records`.
//!
//! The host owns the records book and vCard bodies. This Spore owns only the
//! contact projection and the substring search that Canvas renders.

use serde_json::{Value, json};

wit_bindgen::generate!({ path: "wit", world: "contacts-spore", generate_all });

use soil::records::records::{self, Entry};

const COLLECTION: &str = "contact";
const SCAN_LIMIT: u32 = 512;
const DISPLAY_LIMIT: usize = 120;

struct Component;
export!(Component);

impl Guest for Component {
    fn snapshot(query: String) -> Result<Vec<u8>, String> {
        let needle = query.trim().to_lowercase();
        let mut contacts = Vec::new();
        let mut organizations = std::collections::BTreeSet::new();
        for entry in scan() {
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
            }));
        }
        contacts.sort_by(|left, right| {
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

fn scan() -> Vec<Entry> {
    let mut entries = Vec::new();
    let mut cursor = None;
    while entries.len() < SCAN_LIMIT as usize {
        let Ok(page) = records::scan(COLLECTION, cursor.as_deref(), SCAN_LIMIT) else {
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

fn entries_revision() -> u64 {
    // The host owns revisions; the UI only needs a stable, monotonic-enough stamp
    // for its scene cache. Use the max record revision in the page.
    let mut max = 0;
    for entry in scan() {
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
