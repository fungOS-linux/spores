//! Pulse console semantics.
//!
//! A Pulse cron is a record, not a running process. The scheduler owns the clock
//! and the dispatch; this crate owns the rules the console needs to show and
//! edit one: the five-field schedule, the next and previous fire, the human
//! description, and the health of the last runs. The second half is the same
//! projection for Confluence's webhook ingress, because "what runs on a timer"
//! and "what arrives unasked" are the two unattended edges an operator checks.
//!
//! Nothing in the rules reads a clock, a file or a socket. `now_ms` is an
//! argument and the records directory belongs to the caller; that is what lets
//! the same rules run in a Spore, in the CLI and in a test.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const MS_PER_MINUTE: i64 = 60_000;
const MS_PER_DAY: i64 = 86_400_000;
const MINUTES_PER_DAY: i64 = 1_440;
/// Ten years is enough for every schedule but a hand-written "29 February and a
/// Monday", which the console reports as never firing instead of guessing.
const SEARCH_DAYS: i64 = 3_660;
/// A scheduler that is a few minutes late has not failed; it is late.
const OVERDUE_GRACE_MS: i64 = 5 * MS_PER_MINUTE;
/// A run that starts this close to its due instant is that fire.
const RUN_TOLERANCE_MS: i64 = 2 * MS_PER_MINUTE;

pub const CRON_SCHEMA: &str = "fungos.pulse/cron@1";
pub const RUN_SCHEMA: &str = "fungos.pulse/run@1";
pub const TARGET_SCHEMA: &str = "fungos.pulse/target@1";
pub const HOOK_SCHEMA: &str = "fungos.confluence/hook@1";
pub const DELIVERY_SCHEMA: &str = "fungos.confluence/delivery@1";

/// Collections this console reads. They are `soil:records` names, and the JSONL
/// files are the stand-in until a host implements records.
pub const CRONS: &str = "pulse.cron";
pub const RUNS: &str = "pulse.run";
pub const TARGETS: &str = "pulse.target";
pub const HOOKS: &str = "confluence.hook";
pub const DELIVERIES: &str = "confluence.delivery";

pub const COLLECTIONS: [&str; 5] = [CRONS, RUNS, TARGETS, HOOKS, DELIVERIES];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Problem {
    pub field: String,
    pub text: String,
}

impl Problem {
    pub fn new(field: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            text: text.into(),
        }
    }
}

// ---- schedules -----------------------------------------------------------

/// A parsed five-field cron expression.
///
/// Fields are minute, hour, day of month, month and day of week, in that order.
/// `*`, `n`, `a-b`, `a/n`, `a-b/n` and comma lists are accepted; names (`MON`)
/// are not, because the console, the scheduler and the record must agree on one
/// grammar and three hundred spellings is not a grammar.
///
/// Day of month and day of week are **ANDed**, not ORed as Vixie cron does.
/// `0 9 1 * 1` is "the first, when it is a Monday" and not "every Monday and
/// the first". The stricter rule is the one a reader can predict from the
/// expression they typed, and the console prints the description beside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Schedule {
    text: String,
    fields: [Field; 5],
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Field {
    raw: String,
    values: Vec<u8>,
}

const FIELD_NAMES: [&str; 5] = ["minute", "hour", "day", "month", "weekday"];
const FIELD_BOUNDS: [(u8, u8); 5] = [(0, 59), (0, 23), (1, 31), (1, 12), (0, 7)];
const WEEKDAY_LABELS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTH_LABELS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

impl Schedule {
    pub fn parse(text: &str) -> Result<Self, Vec<Problem>> {
        let parts: Vec<&str> = text.split_whitespace().collect();
        if parts.len() != 5 {
            return Err(vec![Problem::new(
                "schedule",
                format!(
                    "a schedule is five fields (minute hour day month weekday); {text:?} has {}",
                    parts.len()
                ),
            )]);
        }

        let mut problems = Vec::new();
        let mut parsed: Vec<Option<Field>> = Vec::with_capacity(5);
        for (index, part) in parts.iter().enumerate() {
            let (min, max) = FIELD_BOUNDS[index];
            let mut field = parse_field(part, min, max, FIELD_NAMES[index], &mut problems);
            if index == 4 {
                if let Some(field) = field.as_mut() {
                    // Sunday is 0 and 7 is the same day; normalise once so the
                    // rest of the crate never has to remember that.
                    for value in &mut field.values {
                        if *value == 7 {
                            *value = 0;
                        }
                    }
                    field.values.sort_unstable();
                    field.values.dedup();
                }
            }
            parsed.push(field);
        }

        if problems.is_empty() {
            if let [
                Some(minute),
                Some(hour),
                Some(day),
                Some(month),
                Some(weekday),
            ] = parsed.as_slice()
            {
                return Ok(Self {
                    text: parts.join(" "),
                    fields: [
                        minute.clone(),
                        hour.clone(),
                        day.clone(),
                        month.clone(),
                        weekday.clone(),
                    ],
                });
            }
            problems.push(Problem::new(
                "schedule",
                "a schedule field was not readable",
            ));
        }
        Err(problems)
    }

    pub fn validate(text: &str) -> Vec<Problem> {
        Self::parse(text).err().unwrap_or_default()
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    fn values(&self, index: usize) -> &[u8] {
        &self.fields[index].values
    }

    fn raw(&self, index: usize) -> &str {
        &self.fields[index].raw
    }

    /// The field's value when it is a single plain number. Stepping a list or a
    /// step expression is ambiguous, so the console refuses rather than picking
    /// one reading.
    pub fn single(&self, index: usize) -> Option<u8> {
        self.fields[index].raw.parse::<u8>().ok()
    }

    fn day_matches(&self, month: u8, day: u8, weekday: u8) -> bool {
        self.values(3).contains(&month)
            && self.values(2).contains(&day)
            && self.values(4).contains(&weekday)
    }

    /// The first fire strictly after `now_ms`.
    pub fn next_after(&self, now_ms: i64) -> Option<i64> {
        let now_minute = now_ms.div_euclid(MS_PER_MINUTE);
        let today = now_minute.div_euclid(MINUTES_PER_DAY);
        let start = now_minute.rem_euclid(MINUTES_PER_DAY);

        for offset in 0..=SEARCH_DAYS {
            let days = today + offset;
            let (_, month, day) = civil_from_days(days);
            let weekday = weekday_from_days(days);
            if !self.day_matches(month, day, weekday) {
                continue;
            }
            let from = if offset == 0 { start + 1 } else { 0 };
            for hour in self.values(1) {
                for minute in self.values(0) {
                    let at = i64::from(*hour) * 60 + i64::from(*minute);
                    if at < from {
                        continue;
                    }
                    return Some(days * MS_PER_DAY + at * MS_PER_MINUTE);
                }
            }
        }
        None
    }

    /// The last fire strictly before `now_ms`.
    pub fn prev_before(&self, now_ms: i64) -> Option<i64> {
        let now_minute = now_ms.div_euclid(MS_PER_MINUTE);
        let today = now_minute.div_euclid(MINUTES_PER_DAY);
        let start = now_minute.rem_euclid(MINUTES_PER_DAY);

        for offset in 0..=SEARCH_DAYS {
            let days = today - offset;
            let (_, month, day) = civil_from_days(days);
            let weekday = weekday_from_days(days);
            if !self.day_matches(month, day, weekday) {
                continue;
            }
            let upto = if offset == 0 {
                start
            } else {
                MINUTES_PER_DAY - 1
            };
            for hour in self.values(1).iter().rev() {
                for minute in self.values(0).iter().rev() {
                    let at = i64::from(*hour) * 60 + i64::from(*minute);
                    if at > upto {
                        continue;
                    }
                    let candidate = days * MS_PER_DAY + at * MS_PER_MINUTE;
                    if candidate < now_ms {
                        return Some(candidate);
                    }
                }
            }
        }
        None
    }

    /// A sentence a person can check the expression against.
    pub fn describe(&self) -> String {
        let days_all = self.raw(2) == "*" && self.raw(4) == "*";
        let months_all = self.raw(3) == "*";

        if self.raw(0) == "*" && self.raw(1) == "*" && days_all && months_all {
            return "every minute".to_owned();
        }
        if self.raw(1) == "*" && days_all && months_all {
            if let Some(step) = step_of(self.raw(0)) {
                return if step == 1 {
                    "every minute".to_owned()
                } else {
                    format!("every {step} minutes")
                };
            }
            if self.raw(0) == "0" {
                return "every hour, on the hour".to_owned();
            }
        }
        if let (Some(minute), Some(hour)) = (self.single(0), self.single(1)) {
            return format!(
                "{} at {hour:02}:{minute:02}{}",
                self.day_phrase(),
                self.month_phrase()
            );
        }
        format!(
            "minute {} of hour {}{}{}",
            self.raw(0),
            self.raw(1),
            self.day_phrase(),
            self.month_phrase()
        )
    }

    fn day_phrase(&self) -> String {
        let dom_all = self.raw(2) == "*";
        let dow_all = self.raw(4) == "*";
        if dom_all && dow_all {
            return "every day".to_owned();
        }
        if dom_all {
            let days = self.values(4);
            if days == [1, 2, 3, 4, 5] {
                return "weekdays".to_owned();
            }
            let names: Vec<&str> = days
                .iter()
                .map(|day| WEEKDAY_LABELS[usize::from(*day) % 7])
                .collect();
            return format!("on {}", names.join(", "));
        }
        let days: Vec<String> = self.values(2).iter().map(u8::to_string).collect();
        if dow_all {
            return format!("on day {} of the month", days.join(", "));
        }
        let names: Vec<&str> = self
            .values(4)
            .iter()
            .map(|day| WEEKDAY_LABELS[usize::from(*day) % 7])
            .collect();
        format!(
            "on day {} of the month and {}",
            days.join(", "),
            names.join(", ")
        )
    }

    fn month_phrase(&self) -> String {
        if self.raw(3) == "*" {
            return String::new();
        }
        let names: Vec<&str> = self
            .values(3)
            .iter()
            .map(|month| MONTH_LABELS[usize::from(*month).saturating_sub(1) % 12])
            .collect();
        format!(" in {}", names.join(", "))
    }

    /// Replace one field with a plain number. Used by the steppers.
    pub fn with_value(&self, index: usize, value: u8) -> Result<Self, Vec<Problem>> {
        let mut parts: Vec<String> = self.fields.iter().map(|field| field.raw.clone()).collect();
        parts[index] = value.to_string();
        Self::parse(&parts.join(" "))
    }

    /// Toggle one weekday. A schedule always keeps at least one day, because a
    /// record nothing can ever fire is worse than a refused press.
    pub fn with_weekday(&self, day: u8) -> Result<Self, Vec<Problem>> {
        if day > 6 {
            return Err(vec![Problem::new(
                "weekday",
                format!("{day} is not a weekday (0 is Sunday, 6 is Saturday)"),
            )]);
        }
        let mut days: BTreeSet<u8> = self.values(4).iter().copied().collect();
        if !days.remove(&day) {
            days.insert(day);
        }
        if days.is_empty() {
            return Err(vec![Problem::new(
                "weekday",
                "a schedule needs at least one weekday",
            )]);
        }
        let raw = if days.len() == 7 {
            "*".to_owned()
        } else {
            days.iter().map(u8::to_string).collect::<Vec<_>>().join(",")
        };
        let mut parts: Vec<String> = self.fields.iter().map(|field| field.raw.clone()).collect();
        parts[4] = raw;
        Self::parse(&parts.join(" "))
    }

    /// The weekday buttons the editor draws: Monday first, because that is how a
    /// week is read, with the cron number carried in each button.
    pub fn weekday_buttons(&self) -> Vec<Value> {
        let days = self.values(4);
        [1u8, 2, 3, 4, 5, 6, 0]
            .iter()
            .map(|day| {
                json!({
                    "day": day,
                    "label": WEEKDAY_LABELS[usize::from(*day) % 7][..1].to_owned(),
                    "on": days.contains(day),
                })
            })
            .collect()
    }
}

fn step_of(raw: &str) -> Option<u16> {
    let (_, step) = raw.split_once('/')?;
    step.parse::<u16>().ok().filter(|step| *step > 0)
}

fn parse_field(
    raw: &str,
    min: u8,
    max: u8,
    name: &str,
    problems: &mut Vec<Problem>,
) -> Option<Field> {
    if raw.is_empty() {
        problems.push(Problem::new(name, "an empty field"));
        return None;
    }
    let mut values = BTreeSet::new();
    for term in raw.split(',') {
        let term = term.trim();
        if term.is_empty() {
            problems.push(Problem::new(
                name,
                format!("{raw:?} has an empty list item"),
            ));
            return None;
        }
        let (base, step) = match term.split_once('/') {
            Some((base, step)) => match step.parse::<u16>() {
                Ok(step) if step > 0 => (base, Some(u32::from(step))),
                _ => {
                    problems.push(Problem::new(
                        name,
                        format!("{term:?}: the step after '/' must be a positive number"),
                    ));
                    return None;
                }
            },
            None => (term, None),
        };
        let (lo, hi) = match base {
            "*" => (min, max),
            _ => match base.split_once('-') {
                Some((lo, hi)) => match (number(lo), number(hi)) {
                    (Some(lo), Some(hi)) => (lo, hi),
                    _ => {
                        problems.push(Problem::new(
                            name,
                            format!("{term:?}: a range needs two numbers"),
                        ));
                        return None;
                    }
                },
                None => match number(base) {
                    Some(value) => (value, if step.is_some() { max } else { value }),
                    None => {
                        problems.push(Problem::new(
                            name,
                            format!("{term:?}: {base:?} is not a number"),
                        ));
                        return None;
                    }
                },
            },
        };
        if lo < min || hi > max || lo > hi {
            problems.push(Problem::new(
                name,
                format!("{term:?} is outside {min}..={max}"),
            ));
            return None;
        }
        let step = step.unwrap_or(1);
        let mut value = u32::from(lo);
        let limit = u32::from(hi);
        while value <= limit {
            values.insert(value as u8);
            value += step;
        }
    }
    if values.is_empty() {
        problems.push(Problem::new(name, format!("{raw:?} selects nothing")));
        return None;
    }
    Some(Field {
        raw: raw.to_owned(),
        values: values.into_iter().collect(),
    })
}

fn number(text: &str) -> Option<u8> {
    text.parse::<u8>().ok()
}

// ---- civil time ----------------------------------------------------------

// Gregorian date arithmetic, days relative to 1970-01-01.
//
// @implements url:https://howardhinnant.github.io/date_algorithms.html -- days_from_civil and civil_from_days
#[cfg(test)]
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, u8, u8) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    (year + i64::from(month <= 2), month as u8, day as u8)
}

/// 0 = Sunday, matching the cron field.
fn weekday_from_days(days: i64) -> u8 {
    (days + 4).rem_euclid(7) as u8
}

pub fn iso(ms: i64) -> String {
    let minutes = ms.div_euclid(MS_PER_MINUTE);
    let days = minutes.div_euclid(MINUTES_PER_DAY);
    let minute_of_day = minutes.rem_euclid(MINUTES_PER_DAY);
    let hour = minute_of_day / 60;
    let minute = minute_of_day % 60;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:00Z")
}

fn span(magnitude_ms: i64) -> String {
    let minutes = (magnitude_ms.max(0) / MS_PER_MINUTE).max(0);
    if minutes < 60 {
        format!("{minutes}m")
    } else if minutes < 1_440 {
        format!("{}h", minutes / 60)
    } else if minutes < 10_080 {
        format!("{}d", minutes / 1_440)
    } else {
        format!("{}w", minutes / 10_080)
    }
}

pub fn future_text(now_ms: i64, at_ms: i64) -> String {
    if at_ms <= now_ms {
        "now".to_owned()
    } else {
        format!("in {}", span(at_ms - now_ms))
    }
}

fn past_text(now_ms: i64, at_ms: i64) -> String {
    if now_ms - at_ms < MS_PER_MINUTE {
        "now".to_owned()
    } else {
        format!("{} ago", span(now_ms - at_ms))
    }
}

// ---- records -------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Key {
    pub collection: String,
    pub id: String,
}

/// One `soil:records` entry. The host assigns `revision` and `origin`; this
/// stand-in does too, so the file is shaped like the real thing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub key: Key,
    pub schema: String,
    pub revision: u64,
    #[serde(default)]
    pub updated_at_ms: i64,
    #[serde(default = "local_origin")]
    pub origin: String,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub body: Value,
}

fn local_origin() -> String {
    "local".to_owned()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cron {
    /// Filled from the record key on read; the key is authoritative.
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub schedule: String,
    pub target: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub created_at_ms: i64,
    #[serde(default)]
    pub updated_at_ms: i64,
}

fn default_enabled() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    #[serde(default)]
    pub id: String,
    pub cron_id: String,
    pub started_at_ms: i64,
    #[serde(default)]
    pub finished_at_ms: Option<i64>,
    pub outcome: Outcome,
    #[serde(default)]
    pub detail: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Ok,
    Failed,
    Missed,
    Skipped,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Failed => "failed",
            Self::Missed => "missed",
            Self::Skipped => "skipped",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hook {
    /// Filled from the record key on read (`slug` is the key).
    #[serde(default)]
    pub slug: String,
    pub name: String,
    pub provider: String,
    #[serde(default)]
    pub tenant: String,
    #[serde(default)]
    pub kind: String,
    /// The credential *name*, never the secret.
    #[serde(default)]
    pub credential: String,
    #[serde(default)]
    pub created_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delivery {
    #[serde(default)]
    pub id: String,
    pub slug: String,
    #[serde(default)]
    pub delivery_id: String,
    pub status: DeliveryStatus,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub bytes: u64,
    pub received_at_ms: i64,
    #[serde(default)]
    pub node: String,
    #[serde(default)]
    pub event_type: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeliveryStatus {
    Accepted,
    Rejected,
}

impl DeliveryStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    pub schedule: &'static str,
}

pub const PRESETS: [Preset; 6] = [
    Preset {
        id: "5m",
        label: "5 min",
        schedule: "*/5 * * * *",
    },
    Preset {
        id: "15m",
        label: "15 min",
        schedule: "*/15 * * * *",
    },
    Preset {
        id: "hourly",
        label: "Hourly",
        schedule: "0 * * * *",
    },
    Preset {
        id: "daily",
        label: "Daily 03:00",
        schedule: "0 3 * * *",
    },
    Preset {
        id: "weekdays",
        label: "Weekdays",
        schedule: "0 9 * * 1-5",
    },
    Preset {
        id: "weekly",
        label: "Weekly",
        schedule: "0 9 * * 1",
    },
];

pub fn preset(id: &str) -> Option<Preset> {
    PRESETS.iter().copied().find(|preset| preset.id == id)
}

// ---- store ---------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct Store {
    pub crons: BTreeMap<String, Cron>,
    pub targets: BTreeMap<String, Target>,
    pub runs: BTreeMap<String, Run>,
    pub hooks: BTreeMap<String, Hook>,
    pub deliveries: BTreeMap<String, Delivery>,
    revisions: BTreeMap<(String, String), u64>,
}

impl Store {
    /// Fold `soil:records` entries into a store. The last entry for a key wins
    /// unless it carries a lower revision; a tombstone removes the record, the
    /// same way `records.apply` treats one.
    pub fn fold(entries: impl IntoIterator<Item = Entry>) -> Result<Self, String> {
        let mut store = Self::default();
        for entry in entries {
            let key = (entry.key.collection.clone(), entry.key.id.clone());
            if let Some(seen) = store.revisions.get(&key).copied() {
                if entry.revision < seen {
                    continue;
                }
            }
            store.revisions.insert(key, entry.revision);
            let id = entry.key.id.clone();
            if entry.deleted {
                store.remove(&entry.key.collection, &id);
                continue;
            }
            match entry.key.collection.as_str() {
                CRONS => {
                    let mut cron: Cron = read_body(&entry, CRON_SCHEMA)?;
                    cron.id = id.clone();
                    store.crons.insert(id, cron);
                }
                TARGETS => {
                    let mut target: Target = read_body(&entry, TARGET_SCHEMA)?;
                    target.id = id.clone();
                    if target.label.trim().is_empty() {
                        target.label = id.clone();
                    }
                    store.targets.insert(id, target);
                }
                RUNS => {
                    let mut run: Run = read_body(&entry, RUN_SCHEMA)?;
                    run.id = id.clone();
                    store.runs.insert(id, run);
                }
                HOOKS => {
                    let mut hook: Hook = read_body(&entry, HOOK_SCHEMA)?;
                    hook.slug = id.clone();
                    store.hooks.insert(id, hook);
                }
                DELIVERIES => {
                    let mut delivery: Delivery = read_body(&entry, DELIVERY_SCHEMA)?;
                    delivery.id = id.clone();
                    store.deliveries.insert(id, delivery);
                }
                other => {
                    return Err(format!(
                        "{}: collection {other:?} is not one this console reads",
                        entry.key.id
                    ));
                }
            }
        }
        Ok(store)
    }

    pub fn load(dir: &Path) -> Result<Self, String> {
        let mut entries = Vec::new();
        for collection in COLLECTIONS {
            let path = collection_file(dir, collection);
            let text = match fs::read_to_string(&path) {
                Ok(text) => text,
                Err(error) if error.kind() == ErrorKind::NotFound => continue,
                Err(error) => return Err(format!("read {}: {error}", path.display())),
            };
            for (index, line) in text.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let entry: Entry = serde_json::from_str(line)
                    .map_err(|error| format!("{}:{}: {error}", path.display(), index + 1))?;
                entries.push(entry);
            }
        }
        Self::fold(entries)
    }

    /// Append entries to their collection files and make them durable before
    /// answering, so a caller can re-read the directory and see what it wrote.
    pub fn append(dir: &Path, entries: &[Entry]) -> Result<(), String> {
        fs::create_dir_all(dir).map_err(|error| format!("create {}: {error}", dir.display()))?;
        for entry in entries {
            let path = collection_file(dir, &entry.key.collection);
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(|error| format!("open {}: {error}", path.display()))?;
            let line = serde_json::to_string(entry)
                .map_err(|error| format!("encode {}: {error}", entry.key.id))?;
            writeln!(file, "{line}")
                .map_err(|error| format!("write {}: {error}", path.display()))?;
            file.sync_all()
                .map_err(|error| format!("sync {}: {error}", path.display()))?;
        }
        Ok(())
    }

    fn remove(&mut self, collection: &str, id: &str) {
        match collection {
            CRONS => drop(self.crons.remove(id)),
            TARGETS => drop(self.targets.remove(id)),
            RUNS => drop(self.runs.remove(id)),
            HOOKS => drop(self.hooks.remove(id)),
            DELIVERIES => drop(self.deliveries.remove(id)),
            _ => {}
        }
    }
}

pub fn collection_file(dir: &Path, collection: &str) -> PathBuf {
    dir.join(format!("{collection}.jsonl"))
}

fn read_body<T: serde::de::DeserializeOwned>(entry: &Entry, schema: &str) -> Result<T, String> {
    if entry.schema != schema {
        return Err(format!(
            "{} {}: schema {:?} where {schema:?} was expected",
            entry.key.collection, entry.key.id, entry.schema
        ));
    }
    serde_json::from_value(entry.body.clone())
        .map_err(|error| format!("{} {}: {error}", entry.key.collection, entry.key.id))
}

// ---- snapshot ------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tab {
    #[default]
    Crons,
    Ingress,
}

impl Tab {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Crons => "crons",
            Self::Ingress => "ingress",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "crons" | "pulse" => Some(Self::Crons),
            "ingress" | "confluence" | "webhooks" => Some(Self::Ingress),
            _ => None,
        }
    }
}

impl Store {
    /// The whole contract the view paints. One document, so a host that can
    /// render one frame needs no second call.
    pub fn snapshot(
        &self,
        now_ms: i64,
        tab: Tab,
        selected: Option<&str>,
        selected_hook: Option<&str>,
    ) -> Value {
        let selected_cron = selected
            .filter(|id| self.crons.contains_key(*id))
            .map(str::to_owned)
            .or_else(|| self.crons.keys().next().cloned());
        let selected_hook = selected_hook
            .filter(|slug| self.hooks.contains_key(*slug))
            .map(str::to_owned)
            .or_else(|| self.hooks.keys().next().cloned());

        let crons: Vec<Value> = self
            .crons
            .values()
            .map(|cron| self.cron_row(cron, now_ms))
            .collect();
        let hooks: Vec<Value> = self
            .hooks
            .values()
            .map(|hook| self.hook_row(hook, now_ms))
            .collect();

        let mut cron_counts: BTreeMap<&str, usize> = BTreeMap::new();
        for row in &crons {
            if let Some(health) = row["health"].as_str() {
                *cron_counts.entry(health).or_default() += 1;
            }
        }
        let mut hook_counts: BTreeMap<&str, usize> = BTreeMap::new();
        for row in &hooks {
            if let Some(health) = row["health"].as_str() {
                *hook_counts.entry(health).or_default() += 1;
            }
        }

        let selected_schedule = selected_cron
            .as_ref()
            .and_then(|id| self.crons.get(id))
            .map(|cron| cron.schedule.clone());
        let selected_target = selected_cron
            .as_ref()
            .and_then(|id| self.crons.get(id))
            .map(|cron| cron.target.clone());

        let presets: Vec<Value> = PRESETS
            .iter()
            .map(|preset| {
                json!({
                    "id": preset.id,
                    "label": preset.label,
                    "schedule": preset.schedule,
                    "active": selected_schedule.as_deref() == Some(preset.schedule),
                })
            })
            .collect();

        let mut targets: Vec<Value> = self
            .targets
            .values()
            .map(|target| {
                json!({
                    "id": target.id,
                    "label": target.label,
                    "detail": target.detail,
                    "selected": selected_target.as_deref() == Some(target.id.as_str()),
                    "registered": true,
                })
            })
            .collect();
        if let Some(target) = selected_target.as_deref() {
            if !self.targets.contains_key(target) {
                targets.push(json!({
                    "id": target,
                    "label": target,
                    "detail": "not registered with the scheduler",
                    "selected": true,
                    "registered": false,
                }));
            }
        }
        targets.sort_by(|a, b| {
            let left = (
                a["label"].as_str().unwrap_or(""),
                a["id"].as_str().unwrap_or(""),
            );
            let right = (
                b["label"].as_str().unwrap_or(""),
                b["id"].as_str().unwrap_or(""),
            );
            left.cmp(&right)
        });

        json!({
            "now_ms": now_ms,
            "now": iso(now_ms),
            "tab": tab.as_str(),
            "title": "PULSE",
            "crons": crons,
            "selected": selected_cron,
            "presets": presets,
            "targets": targets,
            "hooks": hooks,
            "selected_hook": selected_hook,
            "summary": {
                "crons": self.crons.len(),
                "overdue": cron_counts.get("overdue").copied().unwrap_or(0),
                "failing": cron_counts.get("failing").copied().unwrap_or(0),
                "paused": cron_counts.get("paused").copied().unwrap_or(0),
                "idle": cron_counts.get("idle").copied().unwrap_or(0),
                "invalid": cron_counts.get("invalid").copied().unwrap_or(0),
                "hooks": self.hooks.len(),
                "hooks_failing": hook_counts.get("failing").copied().unwrap_or(0),
                "hooks_degraded": hook_counts.get("degraded").copied().unwrap_or(0),
                "hooks_silent": hook_counts.get("silent").copied().unwrap_or(0),
            },
        })
    }

    fn cron_row(&self, cron: &Cron, now_ms: i64) -> Value {
        let parsed = Schedule::parse(&cron.schedule);
        let problems: Vec<Value> = match &parsed {
            Err(problems) => problems
                .iter()
                .map(|problem| json!({"field": problem.field, "text": problem.text}))
                .collect(),
            Ok(_) => Vec::new(),
        };

        let mut runs: Vec<&Run> = self
            .runs
            .values()
            .filter(|run| run.cron_id == cron.id)
            .collect();
        runs.sort_by(|a, b| {
            b.started_at_ms
                .cmp(&a.started_at_ms)
                .then_with(|| b.id.cmp(&a.id))
        });

        let (health, health_text) = match &parsed {
            Err(_) => ("invalid", "invalid schedule"),
            Ok(schedule) => {
                if !cron.enabled {
                    ("paused", "paused")
                } else if schedule.next_after(now_ms).is_none() {
                    ("unschedulable", "never fires")
                } else {
                    let prev = schedule.prev_before(now_ms);
                    let overdue = prev.is_some_and(|prev| {
                        cron.created_at_ms <= prev
                            && now_ms - prev > OVERDUE_GRACE_MS
                            && runs
                                .first()
                                .is_none_or(|run| run.started_at_ms + RUN_TOLERANCE_MS < prev)
                    });
                    if overdue {
                        ("overdue", "overdue")
                    } else if consecutive_failures(&runs) >= 2 {
                        ("failing", "failing")
                    } else if runs.is_empty() {
                        ("idle", "no runs yet")
                    } else {
                        ("ok", "ok")
                    }
                }
            }
        };

        let next_at = parsed
            .as_ref()
            .ok()
            .and_then(|schedule| schedule.next_after(now_ms));
        let next_text = if !cron.enabled {
            "paused".to_owned()
        } else if next_at.is_none() {
            "never".to_owned()
        } else {
            next_at
                .map(|at| future_text(now_ms, at))
                .unwrap_or_default()
        };
        let last = runs.first().map(|run| {
            json!({
                "at_ms": run.started_at_ms,
                "text": past_text(now_ms, run.started_at_ms),
                "outcome": run.outcome.as_str(),
                "detail": run.detail,
            })
        });
        let recent: Vec<Value> = runs
            .iter()
            .take(5)
            .map(|run| {
                json!({
                    "at_ms": run.started_at_ms,
                    "text": past_text(now_ms, run.started_at_ms),
                    "outcome": run.outcome.as_str(),
                    "detail": run.detail,
                })
            })
            .collect();
        let editable = match &parsed {
            Ok(schedule) => json!({
                "minute": schedule.single(0).is_some(),
                "hour": schedule.single(1).is_some(),
            }),
            Err(_) => json!({"minute": false, "hour": false}),
        };
        let weekdays = parsed
            .as_ref()
            .map(Schedule::weekday_buttons)
            .unwrap_or_default();
        let description = parsed.as_ref().map(Schedule::describe).unwrap_or_default();
        let prev_iso = parsed
            .as_ref()
            .ok()
            .and_then(|schedule| schedule.prev_before(now_ms))
            .map(iso);

        json!({
            "id": cron.id,
            "name": display_name(cron),
            "schedule": cron.schedule,
            "target": cron.target,
            "enabled": cron.enabled,
            "health": health,
            "health_text": health_text,
            "next_at": next_at,
            "next_iso": next_at.map(iso),
            "next_text": next_text,
            "prev_iso": prev_iso,
            "description": description,
            "last": last,
            "runs": recent,
            "problems": problems,
            "editable": editable,
            "weekdays": weekdays,
        })
    }

    fn hook_row(&self, hook: &Hook, now_ms: i64) -> Value {
        let mut deliveries: Vec<&Delivery> = self
            .deliveries
            .values()
            .filter(|delivery| delivery.slug == hook.slug)
            .collect();
        deliveries.sort_by(|a, b| {
            b.received_at_ms
                .cmp(&a.received_at_ms)
                .then_with(|| b.id.cmp(&a.id))
        });

        let window_start = now_ms - MS_PER_DAY;
        let accepted_24h = deliveries
            .iter()
            .filter(|delivery| {
                delivery.received_at_ms > window_start
                    && delivery.status == DeliveryStatus::Accepted
            })
            .count();
        let rejected_24h = deliveries
            .iter()
            .filter(|delivery| {
                delivery.received_at_ms > window_start
                    && delivery.status == DeliveryStatus::Rejected
            })
            .count();
        let accepted_total = deliveries
            .iter()
            .filter(|delivery| delivery.status == DeliveryStatus::Accepted)
            .count();
        let rejected_total = deliveries
            .iter()
            .filter(|delivery| delivery.status == DeliveryStatus::Rejected)
            .count();
        let last = deliveries.first().copied();

        let health = match last {
            None => "unknown",
            Some(last) => {
                if last.status == DeliveryStatus::Rejected || rejected_24h >= 3 {
                    "failing"
                } else if rejected_24h >= 1 {
                    "degraded"
                } else if now_ms - last.received_at_ms > 7 * MS_PER_DAY {
                    "silent"
                } else {
                    "healthy"
                }
            }
        };
        let health_text = match health {
            "unknown" => "no deliveries",
            "failing" => "failing",
            "degraded" => "degraded",
            "silent" => "silent for a week",
            _ => "healthy",
        };

        let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
        for delivery in &deliveries {
            if delivery.received_at_ms > window_start && delivery.status == DeliveryStatus::Rejected
            {
                let reason = if delivery.reason.trim().is_empty() {
                    "rejected without a reason".to_owned()
                } else {
                    delivery.reason.clone()
                };
                *reasons.entry(reason).or_default() += 1;
            }
        }
        let mut reasons: Vec<(String, usize)> = reasons.into_iter().collect();
        reasons.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let reasons: Vec<Value> = reasons
            .into_iter()
            .take(3)
            .map(|(text, count)| json!({"text": text, "count": count}))
            .collect();

        let recent: Vec<Value> = deliveries
            .iter()
            .take(6)
            .map(|delivery| {
                json!({
                    "at_ms": delivery.received_at_ms,
                    "text": past_text(now_ms, delivery.received_at_ms),
                    "status": delivery.status.as_str(),
                    "reason": delivery.reason,
                    "bytes": delivery.bytes,
                    "event_type": delivery.event_type,
                    "node": delivery.node,
                })
            })
            .collect();

        json!({
            "slug": hook.slug,
            "name": if hook.name.trim().is_empty() { hook.slug.as_str() } else { hook.name.as_str() },
            "provider": hook.provider,
            "tenant": hook.tenant,
            "kind": hook.kind,
            "credential": hook.credential,
            "created_at_ms": hook.created_at_ms,
            "created_text": if hook.created_at_ms > 0 { iso(hook.created_at_ms) } else { String::new() },
            "health": health,
            "health_text": health_text,
            "accepted_24h": accepted_24h,
            "rejected_24h": rejected_24h,
            "accepted_total": accepted_total,
            "rejected_total": rejected_total,
            "last": last.map(|delivery| json!({
                "at_ms": delivery.received_at_ms,
                "text": past_text(now_ms, delivery.received_at_ms),
                "status": delivery.status.as_str(),
                "reason": delivery.reason,
            })),
            "reasons": reasons,
            "deliveries": recent,
        })
    }
}

fn display_name(cron: &Cron) -> String {
    if cron.name.trim().is_empty() {
        cron.id.clone()
    } else {
        cron.name.clone()
    }
}

fn consecutive_failures(runs: &[&Run]) -> usize {
    runs.iter()
        .take_while(|run| run.outcome == Outcome::Failed)
        .count()
}

// ---- intents -------------------------------------------------------------

/// What a press means. The view emits these; the host applies them. They are
/// serializable so the same JSON works from a button, an MCP call and the CLI.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Intent {
    Tab {
        tab: Tab,
    },
    Select {
        id: String,
    },
    SelectHook {
        slug: String,
    },
    Save {
        id: String,
        #[serde(default)]
        name: Option<String>,
        schedule: String,
        #[serde(default)]
        target: Option<String>,
        #[serde(default)]
        enabled: Option<bool>,
    },
    Schedule {
        id: String,
        schedule: String,
    },
    Preset {
        id: String,
        preset: String,
    },
    Step {
        id: String,
        field: StepField,
        delta: i32,
    },
    Weekday {
        id: String,
        day: u8,
    },
    Target {
        id: String,
        target: String,
    },
    Toggle {
        id: String,
    },
    Delete {
        id: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StepField {
    Minute,
    Hour,
}

#[derive(Clone, Debug, Default)]
pub struct Applied {
    /// Entries the caller must append to the records store. Empty when the
    /// intent only moved the view, or when it was refused.
    pub entries: Vec<Entry>,
    pub message: String,
    pub problems: Vec<Problem>,
}

impl Applied {
    pub fn ok(&self) -> bool {
        self.problems.is_empty()
    }
}

impl Store {
    pub fn apply(&mut self, intent: &Intent, now_ms: i64) -> Applied {
        match self.run(intent, now_ms) {
            Ok((entry, message)) => Applied {
                entries: entry.into_iter().collect(),
                message,
                problems: Vec::new(),
            },
            Err(problems) => Applied {
                entries: Vec::new(),
                message: String::new(),
                problems,
            },
        }
    }

    fn run(
        &mut self,
        intent: &Intent,
        now_ms: i64,
    ) -> Result<(Option<Entry>, String), Vec<Problem>> {
        match intent {
            Intent::Tab { .. } => Ok((None, String::new())),
            Intent::Select { id } => {
                self.cron(id)?;
                Ok((None, format!("selected {id}")))
            }
            Intent::SelectHook { slug } => {
                if !self.hooks.contains_key(slug) {
                    return Err(bad("slug", format!("no webhook named {slug:?}")));
                }
                Ok((None, format!("selected {slug}")))
            }
            Intent::Save {
                id,
                name,
                schedule,
                target,
                enabled,
            } => {
                let parsed = Schedule::parse(schedule)?;
                let existing = self.crons.get(id).cloned();
                let target = target
                    .clone()
                    .or_else(|| existing.as_ref().map(|cron| cron.target.clone()))
                    .unwrap_or_default();
                if target.trim().is_empty() {
                    return Err(bad("target", "a cron needs a dispatch target"));
                }
                if !self.targets.contains_key(&target) {
                    return Err(bad(
                        "target",
                        format!("{target:?} is not a registered Pulse target"),
                    ));
                }
                let name = name
                    .clone()
                    .or_else(|| existing.as_ref().map(|cron| cron.name.clone()))
                    .unwrap_or_default();
                if name.trim().is_empty() {
                    return Err(bad("name", "a cron needs a name"));
                }
                let cron = Cron {
                    id: id.clone(),
                    name,
                    schedule: parsed.text().to_owned(),
                    target,
                    enabled: enabled
                        .unwrap_or_else(|| existing.as_ref().is_none_or(|cron| cron.enabled)),
                    created_at_ms: existing.as_ref().map_or(now_ms, |cron| cron.created_at_ms),
                    updated_at_ms: now_ms,
                };
                let verb = if existing.is_some() {
                    "saved"
                } else {
                    "created"
                };
                let entry = self.persist(CRONS, CRON_SCHEMA, id, &cron, now_ms)?;
                self.crons.insert(id.clone(), cron);
                Ok((Some(entry), format!("{verb} {id}")))
            }
            Intent::Schedule { id, schedule } => self.set_schedule(id, schedule, now_ms),
            Intent::Preset { id, preset: name } => {
                let preset = preset(name)
                    .ok_or_else(|| bad("preset", format!("no preset named {name:?}")))?;
                self.set_schedule(id, preset.schedule, now_ms)
            }
            Intent::Step { id, field, delta } => {
                let cron = self.cron(id)?;
                let parsed = Schedule::parse(&cron.schedule)?;
                let (index, limit, label) = match field {
                    StepField::Minute => (0, 60, "minute"),
                    StepField::Hour => (1, 24, "hour"),
                };
                let value = parsed.single(index).ok_or_else(|| {
                    bad(
                        "schedule",
                        format!("the {label} field is not a single number; pick a preset first"),
                    )
                })?;
                let next = (i32::from(value) + delta).rem_euclid(limit) as u8;
                let updated = parsed.with_value(index, next)?;
                self.set_schedule(id, updated.text(), now_ms)
            }
            Intent::Weekday { id, day } => {
                let cron = self.cron(id)?;
                let parsed = Schedule::parse(&cron.schedule)?;
                let updated = parsed.with_weekday(*day)?;
                self.set_schedule(id, updated.text(), now_ms)
            }
            Intent::Target { id, target } => {
                if !self.targets.contains_key(target) {
                    return Err(bad(
                        "target",
                        format!("{target:?} is not a registered Pulse target"),
                    ));
                }
                let mut cron = self.cron(id)?;
                cron.target = target.clone();
                cron.updated_at_ms = now_ms;
                let entry = self.persist(CRONS, CRON_SCHEMA, id, &cron, now_ms)?;
                self.crons.insert(id.clone(), cron);
                Ok((Some(entry), format!("target for {id} is now {target}")))
            }
            Intent::Toggle { id } => {
                let mut cron = self.cron(id)?;
                cron.enabled = !cron.enabled;
                cron.updated_at_ms = now_ms;
                let state = if cron.enabled { "enabled" } else { "paused" };
                let entry = self.persist(CRONS, CRON_SCHEMA, id, &cron, now_ms)?;
                self.crons.insert(id.clone(), cron);
                Ok((Some(entry), format!("{state} {id}")))
            }
            Intent::Delete { id } => {
                self.cron(id)?;
                let entry = self.push_entry(CRONS, CRON_SCHEMA, id, json!({}), now_ms, true);
                self.crons.remove(id);
                Ok((Some(entry), format!("deleted {id}")))
            }
        }
    }

    fn cron(&self, id: &str) -> Result<Cron, Vec<Problem>> {
        self.crons
            .get(id)
            .cloned()
            .ok_or_else(|| bad("id", format!("no cron named {id:?}")))
    }

    fn set_schedule(
        &mut self,
        id: &str,
        text: &str,
        now_ms: i64,
    ) -> Result<(Option<Entry>, String), Vec<Problem>> {
        let parsed = Schedule::parse(text)?;
        let mut cron = self.cron(id)?;
        cron.schedule = parsed.text().to_owned();
        cron.updated_at_ms = now_ms;
        let entry = self.persist(CRONS, CRON_SCHEMA, id, &cron, now_ms)?;
        self.crons.insert(id.to_owned(), cron);
        Ok((Some(entry), format!("saved schedule for {id}")))
    }

    fn persist<T: Serialize>(
        &mut self,
        collection: &str,
        schema: &str,
        id: &str,
        record: &T,
        now_ms: i64,
    ) -> Result<Entry, Vec<Problem>> {
        let body = serde_json::to_value(record)
            .map_err(|error| bad("record", format!("encode {id}: {error}")))?;
        Ok(self.push_entry(collection, schema, id, body, now_ms, false))
    }

    fn push_entry(
        &mut self,
        collection: &str,
        schema: &str,
        id: &str,
        body: Value,
        now_ms: i64,
        deleted: bool,
    ) -> Entry {
        let key = (collection.to_owned(), id.to_owned());
        let revision = self.revisions.get(&key).copied().unwrap_or(0) + 1;
        self.revisions.insert(key, revision);
        Entry {
            key: Key {
                collection: collection.to_owned(),
                id: id.to_owned(),
            },
            schema: schema.to_owned(),
            revision,
            updated_at_ms: now_ms,
            origin: "local".to_owned(),
            deleted,
            body,
        }
    }
}

fn bad(field: &str, text: impl Into<String>) -> Vec<Problem> {
    vec![Problem::new(field, text)]
}

// ---- lua -----------------------------------------------------------------

impl Store {
    /// Emit a snapshot as the Lua table `ui/console.lua` reads.
    ///
    /// The view and the fixture are the same artifact: `pulse snapshot` writes this
    /// file, and `tend check` renders it. JSON stays the wire format; Lua is the
    /// form Canvas loads.
    pub fn snapshot_lua(
        &self,
        now_ms: i64,
        tab: Tab,
        selected: Option<&str>,
        selected_hook: Option<&str>,
    ) -> String {
        let mut out =
            String::from("-- generated by `pulse snapshot`; do not edit by hand\nreturn ");
        let snapshot = self.snapshot(now_ms, tab, selected, selected_hook);
        out.push_str(&lua_from_json(&snapshot));
        out.push('\n');
        out
    }
}

pub fn lua_from_json(value: &Value) -> String {
    let mut out = String::new();
    write_lua(value, &mut out, 0);
    out
}

fn write_lua(value: &Value, out: &mut String, depth: usize) {
    match value {
        Value::Null => out.push_str("nil"),
        Value::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        Value::Number(number) => {
            let _ = write!(out, "{number}");
        }
        Value::String(text) => write_lua_string(text, out),
        Value::Array(items) => {
            if items.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push_str("{\n");
            for item in items {
                indent(out, depth + 1);
                write_lua(item, out, depth + 1);
                out.push_str(",\n");
            }
            indent(out, depth);
            out.push('}');
        }
        Value::Object(map) => {
            if map.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push_str("{\n");
            for (key, item) in map {
                indent(out, depth + 1);
                if lua_identifier(key) {
                    out.push_str(key);
                } else {
                    write_lua_string(key, out);
                }
                out.push_str(" = ");
                write_lua(item, out, depth + 1);
                out.push_str(",\n");
            }
            indent(out, depth);
            out.push('}');
        }
    }
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn lua_identifier(key: &str) -> bool {
    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return false;
    }
    !matches!(
        key,
        "and"
            | "break"
            | "do"
            | "else"
            | "elseif"
            | "end"
            | "false"
            | "for"
            | "function"
            | "goto"
            | "if"
            | "in"
            | "local"
            | "nil"
            | "not"
            | "or"
            | "repeat"
            | "return"
            | "then"
            | "true"
            | "until"
            | "while"
    )
}

fn write_lua_string(text: &str, out: &mut String) {
    out.push('"');
    for character in text.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\{:03}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(year: i64, month: u32, day: u32, hour: u32, minute: u32) -> i64 {
        days_from_civil(year, month, day) * MS_PER_DAY
            + i64::from(hour * 60 + minute) * MS_PER_MINUTE
    }

    fn schedule(text: &str) -> Schedule {
        Schedule::parse(text).expect("schedule parses")
    }

    fn cron(id: &str, text: &str, created_at_ms: i64) -> Cron {
        Cron {
            id: id.to_owned(),
            name: id.to_owned(),
            schedule: text.to_owned(),
            target: "dev.fungos.worklog#pull".to_owned(),
            enabled: true,
            created_at_ms,
            updated_at_ms: created_at_ms,
        }
    }

    fn run(id: &str, cron_id: &str, started_at_ms: i64, outcome: Outcome) -> Run {
        Run {
            id: id.to_owned(),
            cron_id: cron_id.to_owned(),
            started_at_ms,
            finished_at_ms: Some(started_at_ms + 1_000),
            outcome,
            detail: String::new(),
        }
    }

    fn entry(
        collection: &str,
        schema: &str,
        id: &str,
        revision: u64,
        deleted: bool,
        body: Value,
    ) -> Entry {
        Entry {
            key: Key {
                collection: collection.to_owned(),
                id: id.to_owned(),
            },
            schema: schema.to_owned(),
            revision,
            updated_at_ms: 0,
            origin: "local".to_owned(),
            deleted,
            body,
        }
    }

    #[test]
    fn a_schedule_is_five_fields_and_nothing_else() {
        assert_eq!(schedule("*/15 * * * *").text(), "*/15 * * * *");
        for bad in [
            "* * * *",
            "* * * * * *",
            "60 * * * *",
            "* 24 * * *",
            "* * 0 * *",
            "* * * 13 *",
            "* * * * 8",
            "*/0 * * * *",
            "MON * * * *",
            "5-1 * * * *",
        ] {
            assert!(Schedule::parse(bad).is_err(), "{bad:?} should not parse");
        }
        // 7 and 0 are the same day; the crate keeps only one spelling.
        assert_eq!(schedule("0 0 * * 7").values(4), [0]);
    }

    #[test]
    fn next_fire_is_strictly_after_now() {
        let every_15 = schedule("*/15 * * * *");
        assert_eq!(
            every_15.next_after(at(2026, 1, 2, 3, 2)),
            Some(at(2026, 1, 2, 3, 15))
        );
        // At the fire itself the next one is the answer: a scheduler asks before
        // it runs, and "now" is not "later".
        assert_eq!(
            every_15.next_after(at(2026, 1, 2, 3, 15)),
            Some(at(2026, 1, 2, 3, 30))
        );
        assert_eq!(
            every_15.prev_before(at(2026, 1, 2, 3, 2)),
            Some(at(2026, 1, 2, 3, 0))
        );
        assert_eq!(
            every_15.prev_before(at(2026, 1, 2, 3, 0)),
            Some(at(2026, 1, 2, 2, 45))
        );
    }

    #[test]
    fn next_fire_crosses_days_and_weekdays() {
        let daily = schedule("0 3 * * *");
        assert_eq!(
            daily.next_after(at(2026, 1, 2, 4, 0)),
            Some(at(2026, 1, 3, 3, 0))
        );

        // 2026-01-02 is a Friday, so the next weekday fire is Monday the 5th.
        let weekdays = schedule("0 9 * * 1-5");
        assert_eq!(
            weekdays.next_after(at(2026, 1, 2, 10, 0)),
            Some(at(2026, 1, 5, 9, 0))
        );

        let leap = schedule("0 0 29 2 *");
        assert_eq!(
            leap.next_after(at(2026, 1, 1, 0, 0)),
            Some(at(2028, 2, 29, 0, 0))
        );

        // February 30 is a valid *expression* and an impossible date. Saying so
        // is better than returning a fire in a year that does not have it.
        assert_eq!(
            schedule("0 0 30 2 *").next_after(at(2026, 1, 1, 0, 0)),
            None
        );
    }

    #[test]
    fn day_of_month_and_weekday_are_anded() {
        // 2026-01-01 is a Thursday. A schedule for "the 1st and Monday" must
        // not fire until a first-of-month that is also a Monday.
        let both = schedule("0 0 1 * 1");
        assert_eq!(
            both.next_after(at(2026, 1, 2, 0, 0)),
            Some(at(2026, 6, 1, 0, 0))
        );
    }

    #[test]
    fn descriptions_match_the_main_shapes() {
        assert_eq!(schedule("*/15 * * * *").describe(), "every 15 minutes");
        assert_eq!(schedule("0 * * * *").describe(), "every hour, on the hour");
        assert_eq!(schedule("0 3 * * *").describe(), "every day at 03:00");
        assert_eq!(schedule("0 9 * * 1-5").describe(), "weekdays at 09:00");
        assert_eq!(schedule("0 0 * * 7").describe(), "on Sun at 00:00");
        assert_eq!(
            schedule("0 0 29 2 *").describe(),
            "on day 29 of the month at 00:00 in Feb"
        );
    }

    #[test]
    fn stepping_requires_one_readable_value() {
        let daily = schedule("0 3 * * *");
        assert_eq!(daily.single(0), Some(0));
        assert_eq!(daily.single(1), Some(3));
        assert_eq!(schedule("*/15 * * * *").single(0), None);
        assert_eq!(
            daily.with_value(1, 4).expect("step hour").text(),
            "0 4 * * *"
        );
    }

    #[test]
    fn weekday_toggle_keeps_at_least_one_day() {
        let every_day = schedule("0 0 * * *");
        let no_monday = every_day.with_weekday(1).expect("drop Monday");
        assert_eq!(no_monday.values(4), [0, 2, 3, 4, 5, 6]);
        let back = no_monday.with_weekday(1).expect("restore Monday");
        assert_eq!(back.text(), "0 0 * * *");

        let only_monday = schedule("0 0 * * 1");
        assert!(only_monday.with_weekday(1).is_err());
    }

    #[test]
    fn entries_fold_last_wins_and_tombstones_remove() {
        let store = Store::fold(vec![
            entry(
                CRONS,
                CRON_SCHEMA,
                "hub",
                1,
                false,
                json!(cron("hub", "*/15 * * * *", 0)),
            ),
            entry(
                CRONS,
                CRON_SCHEMA,
                "hub",
                2,
                false,
                json!(cron("hub", "*/5 * * * *", 0)),
            ),
            entry(
                CRONS,
                CRON_SCHEMA,
                "gone",
                1,
                false,
                json!(cron("gone", "0 * * * *", 0)),
            ),
            entry(CRONS, CRON_SCHEMA, "gone", 2, true, json!({})),
        ])
        .expect("fold");
        assert_eq!(store.crons["hub"].schedule, "*/5 * * * *");
        assert!(!store.crons.contains_key("gone"));
    }

    #[test]
    fn a_lower_revision_does_not_overwrite_a_higher_one() {
        let store = Store::fold(vec![
            entry(
                CRONS,
                CRON_SCHEMA,
                "hub",
                3,
                false,
                json!(cron("hub", "*/15 * * * *", 0)),
            ),
            entry(
                CRONS,
                CRON_SCHEMA,
                "hub",
                2,
                false,
                json!(cron("hub", "*/5 * * * *", 0)),
            ),
        ])
        .expect("fold");
        assert_eq!(store.crons["hub"].schedule, "*/15 * * * *");
    }

    fn store_with_hub() -> Store {
        Store::fold(vec![
            entry(
                TARGETS,
                TARGET_SCHEMA,
                "dev.fungos.worklog#pull",
                1,
                false,
                json!({"label": "worklog pull"}),
            ),
            entry(
                CRONS,
                CRON_SCHEMA,
                "hub",
                1,
                false,
                json!(cron("hub", "*/15 * * * *", 0)),
            ),
        ])
        .expect("fold")
    }

    #[test]
    fn preset_writes_a_higher_revision_and_a_new_schedule() {
        let mut store = store_with_hub();
        let now = at(2026, 1, 2, 4, 0);
        let applied = store.apply(
            &Intent::Preset {
                id: "hub".to_owned(),
                preset: "hourly".to_owned(),
            },
            now,
        );
        assert!(applied.ok(), "{:?}", applied.problems);
        assert_eq!(store.crons["hub"].schedule, "0 * * * *");
        assert_eq!(applied.entries.len(), 1);
        assert_eq!(applied.entries[0].revision, 2);
        assert_eq!(applied.entries[0].updated_at_ms, now);
    }

    #[test]
    fn invalid_schedule_is_refused_and_nothing_is_written() {
        let mut store = store_with_hub();
        let applied = store.apply(
            &Intent::Schedule {
                id: "hub".to_owned(),
                schedule: "every 5 minutes".to_owned(),
            },
            0,
        );
        assert!(!applied.ok());
        assert!(applied.entries.is_empty());
        assert_eq!(store.crons["hub"].schedule, "*/15 * * * *");
    }

    #[test]
    fn target_must_be_registered() {
        let mut store = store_with_hub();
        let refused = store.apply(
            &Intent::Target {
                id: "hub".to_owned(),
                target: "nobody#nope".to_owned(),
            },
            0,
        );
        assert!(!refused.ok());

        let accepted = store.apply(
            &Intent::Target {
                id: "hub".to_owned(),
                target: "dev.fungos.worklog#pull".to_owned(),
            },
            0,
        );
        assert!(accepted.ok());
    }

    #[test]
    fn delete_tombstones_and_removes() {
        let mut store = store_with_hub();
        let applied = store.apply(
            &Intent::Delete {
                id: "hub".to_owned(),
            },
            123,
        );
        assert!(applied.ok());
        assert!(store.crons.is_empty());
        assert!(applied.entries[0].deleted);
        assert_eq!(applied.entries[0].key.collection, CRONS);
    }

    #[test]
    fn cron_health_names_what_is_wrong() {
        let now = at(2026, 1, 2, 4, 7);
        let mut store = Store::default();
        store.crons.insert(
            "late".to_owned(),
            cron("late", "*/15 * * * *", at(2026, 1, 2, 3, 0)),
        );
        store.runs.insert(
            "late-1".to_owned(),
            run("late-1", "late", at(2026, 1, 2, 3, 0), Outcome::Ok),
        );
        let row = store.cron_row(&store.crons["late"], now);
        assert_eq!(row["health"], "overdue");

        store.runs.clear();
        for index in 0..2 {
            store.runs.insert(
                format!("fail-{index}"),
                run(
                    &format!("fail-{index}"),
                    "late",
                    at(2026, 1, 2, 4, 0),
                    Outcome::Failed,
                ),
            );
        }
        let row = store.cron_row(&store.crons["late"], now);
        assert_eq!(row["health"], "failing");

        store.runs.clear();
        store.crons.get_mut("late").expect("cron").enabled = false;
        let row = store.cron_row(&store.crons["late"], now);
        assert_eq!(row["health"], "paused");

        store.crons.get_mut("late").expect("cron").schedule = "nope".to_owned();
        let row = store.cron_row(&store.crons["late"], now);
        assert_eq!(row["health"], "invalid");
        assert!(!row["problems"].as_array().expect("problems").is_empty());
    }

    #[test]
    fn webhook_health_is_failing_degraded_silent_or_healthy() {
        let now = at(2026, 1, 2, 4, 0);
        let mut store = Store::default();
        for slug in ["ok", "bad", "deg", "quiet", "none"] {
            store.hooks.insert(
                slug.to_owned(),
                Hook {
                    slug: slug.to_owned(),
                    name: slug.to_owned(),
                    provider: "github".to_owned(),
                    tenant: "default".to_owned(),
                    kind: "confluence.webhook.received.v1".to_owned(),
                    credential: slug.to_owned(),
                    created_at_ms: now - 30 * MS_PER_DAY,
                },
            );
        }
        let delivery =
            |id: &str, slug: &str, at_ms: i64, status: DeliveryStatus, reason: &str| Delivery {
                id: id.to_owned(),
                slug: slug.to_owned(),
                delivery_id: id.to_owned(),
                status,
                reason: reason.to_owned(),
                bytes: 10,
                received_at_ms: at_ms,
                node: String::new(),
                event_type: String::new(),
            };
        store.deliveries.insert(
            "ok-1".to_owned(),
            delivery(
                "ok-1",
                "ok",
                now - MS_PER_MINUTE,
                DeliveryStatus::Accepted,
                "",
            ),
        );
        store.deliveries.insert(
            "bad-1".to_owned(),
            delivery(
                "bad-1",
                "bad",
                now - MS_PER_MINUTE,
                DeliveryStatus::Rejected,
                "401",
            ),
        );
        store.deliveries.insert(
            "deg-1".to_owned(),
            delivery(
                "deg-1",
                "deg",
                now - MS_PER_MINUTE,
                DeliveryStatus::Accepted,
                "",
            ),
        );
        store.deliveries.insert(
            "deg-2".to_owned(),
            delivery(
                "deg-2",
                "deg",
                now - 2 * MS_PER_MINUTE,
                DeliveryStatus::Rejected,
                "401",
            ),
        );
        store.deliveries.insert(
            "quiet-1".to_owned(),
            delivery(
                "quiet-1",
                "quiet",
                now - 8 * MS_PER_DAY,
                DeliveryStatus::Accepted,
                "",
            ),
        );

        let row = |slug: &str| store.hook_row(&store.hooks[slug], now);
        assert_eq!(row("ok")["health"], "healthy");
        assert_eq!(row("bad")["health"], "failing");
        assert_eq!(row("deg")["health"], "degraded");
        assert_eq!(row("deg")["rejected_24h"], 1);
        assert_eq!(row("deg")["reasons"][0]["text"], "401");
        assert_eq!(row("quiet")["health"], "silent");
        assert_eq!(row("none")["health"], "unknown");
    }

    #[test]
    fn snapshot_names_the_selection_and_counts_the_rows() {
        let mut store = store_with_hub();
        store.crons.insert(
            "backup".to_owned(),
            cron("backup", "0 3 * * *", at(2026, 1, 1, 0, 0)),
        );
        let now = at(2026, 1, 2, 4, 0);
        let snapshot = store.snapshot(now, Tab::Crons, Some("backup"), None);
        assert_eq!(snapshot["selected"], "backup");
        assert_eq!(snapshot["now"], "2026-01-02T04:00:00Z");
        assert_eq!(snapshot["summary"]["crons"], 2);
        assert_eq!(snapshot["crons"].as_array().expect("crons").len(), 2);
        let backup = snapshot["crons"]
            .as_array()
            .expect("crons")
            .iter()
            .find(|row| row["id"] == "backup")
            .expect("backup row");
        assert_eq!(backup["next_iso"], "2026-01-03T03:00:00Z");
        assert_eq!(backup["next_text"], "in 23h");
    }

    #[test]
    fn lua_emitter_quotes_strings_and_writes_real_tables() {
        let value = json!({
            "now_ms": 1,
            "tab": "crons",
            "note": "a \"quoted\" line\nand a second",
            "rows": [{ "on": true }, { "on": false }],
            "none": null,
        });
        let lua = lua_from_json(&value);
        assert!(lua.contains("now_ms = 1"));
        assert!(lua.contains("tab = \"crons\""));
        assert!(lua.contains("\\\"quoted\\\""));
        assert!(lua.contains("\\nand"));
        assert!(lua.contains("on = true"));
        assert!(lua.contains("none = nil"));
    }
}
