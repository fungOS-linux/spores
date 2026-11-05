//! Calendar over `soil:records`.
//!
//! The host owns the records book and iCal bodies. This Spore owns the calendar
//! projection: parse stored event fields, resolve TZID time zones, expand bounded
//! recurrence windows, detect same-day overlaps, and emit one Canvas snapshot.

use std::str::FromStr;

use chrono::TimeZone;
use chrono_tz::Tz as ChronoTz;
use rrule::{RRule, RRuleSet, Tz, Unvalidated};
use serde_json::{Value, json};

wit_bindgen::generate!({ path: "wit", world: "calendar-spore", generate_all });

use soil::records::records::{self, Entry};

const COLLECTION: &str = "event";
const SCAN_LIMIT: u32 = 512;
const MAX_EVENTS: usize = 512;
const RECUR_BEFORE_MS: u64 = 7 * 24 * 60 * 60 * 1000;
const RECUR_AFTER_MS: u64 = 365 * 24 * 60 * 60 * 1000;

struct Component;
export!(Component);

#[derive(Clone, Default)]
struct Event {
    id: String,
    summary: String,
    calendar_id: String,
    calendar_label: String,
    calendar_color: String,
    date_key: String,
    end_date: String,
    start_time: String,
    end_time: String,
    start_ms: u64,
    end_ms: u64,
    all_day: bool,
    is_recurring: bool,
    rrule: String,
    location: String,
    organizer: String,
    attendees: Vec<String>,
    notes: String,
    conflicts: Vec<String>,
}

impl Guest for Component {
    fn snapshot(query: String, now_ms: u64, local_zone: String) -> Result<Vec<u8>, String> {
        let needle = query.trim().to_lowercase();
        let local_zone = ChronoTz::from_str(&local_zone).map(Tz::from).unwrap_or(Tz::UTC);
        let mut events = Vec::new();
        let mut calendars = std::collections::BTreeMap::<String, (String, String, usize)>::new();
        let mut revision = 0u64;
        for entry in scan(COLLECTION, SCAN_LIMIT) {
            revision = revision.max(entry.revision);
            let Ok(value) = serde_json::from_slice::<Value>(&entry.body) else {
                continue;
            };
            for event in project(entry.key.id, value, now_ms, local_zone) {
                if !needle.is_empty() {
                    let haystack = format!(
                        "{} {} {} {}",
                        event.summary, event.location, event.notes, event.calendar_label
                    )
                    .to_lowercase();
                    if !haystack.contains(&needle) {
                        continue;
                    }
                }
                calendars
                    .entry(event.calendar_id.clone())
                    .and_modify(|slot| slot.2 += 1)
                    .or_insert((event.calendar_label.clone(), event.calendar_color.clone(), 1));
                events.push(event);
            }
        }
        events.sort_by(|left, right| {
            left.start_ms
                .cmp(&right.start_ms)
                .then_with(|| left.summary.to_lowercase().cmp(&right.summary.to_lowercase()))
        });
        mark_conflicts(&mut events);
        events.truncate(MAX_EVENTS);
        let selected = events.first().map(|event| event.id.as_str()).unwrap_or("");
        let calendars = calendars
            .into_iter()
            .map(|(id, (label, color, count))| {
                json!({"id": id, "label": label, "color": color, "count": count})
            })
            .collect::<Vec<_>>();
        let events = events
            .iter()
            .map(|event| {
                json!({
                    "id": event.id,
                    "summary": event.summary,
                    "calendar_id": event.calendar_id,
                    "calendar_label": event.calendar_label,
                    "calendar_color": event.calendar_color,
                    "date_key": event.date_key,
                    "end_date": event.end_date,
                    "start_time": event.start_time,
                    "end_time": event.end_time,
                    "start_ms": event.start_ms,
                    "end_ms": event.end_ms,
                    "all_day": event.all_day,
                    "is_recurring": event.is_recurring,
                    "rrule": event.rrule,
                    "location": event.location,
                    "organizer": event.organizer,
                    "attendees": event.attendees,
                    "notes": event.notes,
                    "conflicts": event.conflicts,
                })
            })
            .collect::<Vec<_>>();
        let output = json!({
            "revision": revision,
            "title": "Calendar",
            "account": {"label": "Records · calendar", "sync": "live"},
            "calendars": calendars,
            "events": events,
            "selected_id": selected,
            "stats": {
                "events": events.len(),
                "conflicts": events.iter().filter(|event| !event["conflicts"].as_array().is_some_and(Vec::is_empty)).count(),
            },
        });
        serde_json::to_vec(&output).map_err(|error| error.to_string())
    }
}

fn project(id: String, value: Value, now_ms: u64, local_zone: Tz) -> Vec<Event> {
    let Some(start_raw) = text(&value, "start") else {
        return Vec::new();
    };
    let zone = event_zone(&value, &start_raw);
    let Some(start) = parse_datetime(&start_raw, zone) else {
        return Vec::new();
    };
    let end = parse_datetime(&text(&value, "end").unwrap_or_default(), zone).unwrap_or(start);
    let duration_ms = (end.timestamp_millis() - start.timestamp_millis()).max(0);
    let categories = value
        .get("categories")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).map(str::to_owned).collect::<Vec<_>>())
        .unwrap_or_default();
    let calendar_label = categories
        .first()
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Calendar".into());
    let calendar_id = slug(&calendar_label);
    let calendar_color = match calendar_id.as_str() {
        "personal" => "success",
        "family" | "birthdays" => "warning",
        _ => "primary",
    }
    .into();
    let attendees = value
        .get("attendees")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    text(item, "name")
                        .or_else(|| text(item, "value"))
                        .filter(|value| !value.is_empty())
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let organizer = value
        .get("organizer")
        .and_then(|organizer| text(organizer, "name").or_else(|| text(organizer, "value")))
        .unwrap_or_default();
    let rrule = text(&value, "rrule").unwrap_or_default();
    let shown_start = start.with_timezone(&local_zone);
    let shown_end = end.with_timezone(&local_zone);
    let base = Event {
        id,
        summary: text(&value, "summary").unwrap_or_else(|| "Untitled event".into()),
        calendar_id,
        calendar_label,
        calendar_color,
        date_key: shown_start.date_naive().format("%Y-%m-%d").to_string(),
        end_date: shown_end.date_naive().format("%Y-%m-%d").to_string(),
        start_time: if start.time() == chrono::NaiveTime::MIN { String::new() } else { shown_start.format("%H:%M").to_string() },
        end_time: if end.time() == chrono::NaiveTime::MIN { String::new() } else { shown_end.format("%H:%M").to_string() },
        start_ms: start.timestamp_millis().max(0) as u64,
        end_ms: end.timestamp_millis().max(0) as u64,
        all_day: value.get("all_day").and_then(Value::as_bool).unwrap_or(start.time() == chrono::NaiveTime::MIN),
        is_recurring: !rrule.is_empty(),
        rrule,
        location: text(&value, "location").unwrap_or_default(),
        organizer,
        attendees,
        notes: text(&value, "description").or_else(|| text(&value, "notes")).unwrap_or_default(),
        conflicts: Vec::new(),
    };
    if !base.is_recurring {
        return vec![base];
    }
    let Ok(rule) = RRule::<Unvalidated>::from_str(&base.rrule) else {
        return vec![base];
    };
    let Ok(rule) = rule.validate(start) else {
        return vec![base];
    };
    let after_ms = now_ms.saturating_sub(RECUR_BEFORE_MS);
    let before_ms = now_ms.saturating_add(RECUR_AFTER_MS);
    let after = zone.timestamp_millis_opt(after_ms as i64).single();
    let before = zone.timestamp_millis_opt(before_ms as i64).single();
    let (Some(after), Some(before)) = (after, before) else {
        return vec![base];
    };
    let set = RRuleSet::new(start).rrule(rule).after(after).before(before);
    let dates = set.all(MAX_EVENTS as u16).dates;
    if dates.is_empty() {
        return vec![base];
    }
    dates
        .into_iter()
        .map(|occurrence| {
            let occurrence_end = occurrence + chrono::Duration::milliseconds(duration_ms);
            let shown_start = occurrence.with_timezone(&local_zone);
            let shown_end = occurrence_end.with_timezone(&local_zone);
            let occurrence_ms = occurrence.timestamp_millis().max(0) as u64;
            Event {
                id: format!("{}@{}", base.id, occurrence_ms / 1000),
                date_key: shown_start.date_naive().format("%Y-%m-%d").to_string(),
                end_date: shown_end.date_naive().format("%Y-%m-%d").to_string(),
                start_time: shown_start.format("%H:%M").to_string(),
                end_time: if duration_ms > 0 { shown_end.format("%H:%M").to_string() } else { String::new() },
                start_ms: occurrence_ms,
                end_ms: occurrence_end.timestamp_millis().max(occurrence_ms as i64) as u64,
                ..base.clone()
            }
        })
        .collect()
}

fn event_zone(value: &Value, start: &str) -> Tz {
    if start.ends_with('Z') {
        return Tz::UTC;
    }
    text(value, "timezone")
        .and_then(|name| ChronoTz::from_str(&name).ok())
        .map(Tz::from)
        .unwrap_or(Tz::UTC)
}

fn parse_datetime(raw: &str, zone: Tz) -> Option<chrono::DateTime<Tz>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    if digits.len() < 8 {
        return None;
    }
    let year = digits.get(0..4)?.parse().ok()?;
    let month = digits.get(4..6)?.parse().ok()?;
    let day = digits.get(6..8)?.parse().ok()?;
    let (hour, minute, second) = if digits.len() >= 14 {
        (digits.get(8..10)?.parse().ok()?, digits.get(10..12)?.parse().ok()?, digits.get(12..14)?.parse().ok()?)
    } else {
        (0, 0, 0)
    };
    let zone = if raw.ends_with('Z') { Tz::UTC } else { zone };
    zone.with_ymd_and_hms(year, month, day, hour, minute, second).single()
}

fn mark_conflicts(events: &mut [Event]) {
    for left in 0..events.len() {
        let a = &events[left];
        if a.all_day || a.start_ms == 0 || a.end_ms <= a.start_ms {
            continue;
        }
        let mut ids = Vec::new();
        for right in 0..events.len() {
            if left == right {
                continue;
            }
            let b = &events[right];
            if b.all_day || b.start_ms == 0 || b.end_ms <= b.start_ms {
                continue;
            }
            if a.start_ms < b.end_ms && b.start_ms < a.end_ms {
                ids.push(b.id.clone());
            }
        }
        events[left].conflicts = ids;
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

fn text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(|value| value.trim().trim_end_matches(';').trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn slug(value: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() { "calendar".into() } else { out }
}
