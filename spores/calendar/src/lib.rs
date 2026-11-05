//! Calendar over `soil:records`.
//!
//! The host owns the records book and iCal bodies. This Spore owns the calendar
//! projection: parse the stored event fields, normalize dates and times, detect
//! same-day overlaps, and emit one bounded Canvas snapshot.

use serde_json::{Value, json};

wit_bindgen::generate!({ path: "wit", world: "calendar-spore", generate_all });

use soil::records::records::{self, Entry};

const COLLECTION: &str = "event";
const SCAN_LIMIT: u32 = 512;
const MAX_EVENTS: usize = 512;

struct Component;
export!(Component);

#[derive(Default)]
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
    fn snapshot(query: String) -> Result<Vec<u8>, String> {
        let needle = query.trim().to_lowercase();
        let mut events = Vec::new();
        let mut calendars = std::collections::BTreeMap::<String, (String, String, usize)>::new();
        let mut revision = 0u64;
        for entry in scan(COLLECTION, SCAN_LIMIT) {
            revision = revision.max(entry.revision);
            let Ok(value) = serde_json::from_slice::<Value>(&entry.body) else {
                continue;
            };
            let Some(event) = project(entry.key.id, value) else {
                continue;
            };
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

fn project(id: String, value: Value) -> Option<Event> {
    let start = parse_moment(text(&value, "start")?)?;
    let end = parse_moment(text(&value, "end").unwrap_or_default()).unwrap_or_default();
    let all_day = value.get("all_day").and_then(Value::as_bool).unwrap_or(start.time.is_empty());
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
        "family" => "warning",
        "birthdays" => "warning",
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
    let end_date = if end.date.is_empty() { start.date.clone() } else { end.date.clone() };
    Some(Event {
        id,
        summary: text(&value, "summary").unwrap_or_else(|| "Untitled event".into()),
        calendar_id,
        calendar_label,
        calendar_color,
        date_key: start.date.clone(),
        end_date,
        start_time: start.time.clone(),
        end_time: end.time.clone(),
        start_ms: start.ms,
        end_ms: if end.ms > start.ms { end.ms } else { start.ms },
        all_day,
        is_recurring: !text(&value, "rrule").unwrap_or_default().is_empty(),
        rrule: text(&value, "rrule").unwrap_or_default(),
        location: text(&value, "location").unwrap_or_default(),
        organizer,
        attendees,
        notes: text(&value, "description").or_else(|| text(&value, "notes")).unwrap_or_default(),
        conflicts: Vec::new(),
    })
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

#[derive(Default)]
struct Moment {
    date: String,
    time: String,
    ms: u64,
}

fn parse_moment(raw: String) -> Option<Moment> {
    let raw = raw.trim().to_string();
    if raw.is_empty() {
        return None;
    }
    let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    if digits.len() < 8 {
        return None;
    }
    let year = digits.get(0..4)?.parse::<i64>().ok()?;
    let month = digits.get(4..6)?.parse::<i64>().ok()?;
    let day = digits.get(6..8)?.parse::<i64>().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let date = format!("{year:04}-{month:02}-{day:02}");
    let (hour, minute, second) = if digits.len() >= 14 {
        (
            digits.get(8..10)?.parse::<i64>().ok()?,
            digits.get(10..12)?.parse::<i64>().ok()?,
            digits.get(12..14)?.parse::<i64>().ok()?,
        )
    } else if raw.contains('T') && raw.len() >= 16 {
        (
            raw.get(11..13)?.parse::<i64>().ok()?,
            raw.get(14..16)?.parse::<i64>().ok()?,
            0,
        )
    } else {
        (0, 0, 0)
    };
    if !(0..=23).contains(&hour) || !(0..=59).contains(&minute) || !(0..=59).contains(&second) {
        return None;
    }
    let time = if digits.len() >= 12 || raw.contains('T') {
        format!("{hour:02}:{minute:02}")
    } else {
        String::new()
    };
    let days = days_from_civil(year, month, day);
    let ms = (days * 86_400 + hour * 3_600 + minute * 60 + second) as u64 * 1000;
    Some(Moment { date, time, ms })
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - if month <= 2 { 1 } else { 0 };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
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
