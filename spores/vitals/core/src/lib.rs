//! Cross-device health fusion for the vitals Spore.
//!
//! The device Spores own protocols and emit typed samples. This crate owns the
//! policy that turns those samples into one wearer-scoped summary:
//!
//! * the ring is primary for overnight HR/HRV/SpO2/temperature and sleep;
//! * the PineTime is primary for motion and the fallback for HR/sleep;
//! * sleep stages are selected from one source, never averaged;
//! * steps stay per device, never summed;
//! * stale or guest-worn samples are not the owner's data.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

pub const SOURCE: &str = "dev.fungos.vitals";
pub const SCHEMA: &str = "vitals.summary.v1";
pub const POLICY_VERSION: &str = "vitals-fusion-v1";

/// Display bounds for the activity rings. These are presentation scales, not
/// health targets; a wearer profile can replace them without changing the
/// `vitals.view.v1` shape.
const MOVE_RING_MAX_STEPS: u64 = 10_000;
const HEART_RING_MAX_BPM: u64 = 180;
const OXYGEN_RING_MAX_PERCENT: u64 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Ring,
    Watch,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    Fresh,
    Stale,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    pub fresh_ms: u64,
    pub stale_ms: u64,
    /// Ring first for HR/sleep. Set false for a watch-first experiment.
    pub ring_primary: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            fresh_ms: 60_000,
            stale_ms: 300_000,
            ring_primary: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Signal {
    pub signal_id: String,
    pub kind: String,
    pub source_device: String,
    pub source_spore: String,
    pub wearer: Option<String>,
    pub observed_at: u64,
    pub quality: f64,
    pub value: Value,
}

impl Signal {
    #[must_use]
    pub fn from_record(body: &Value) -> Option<Self> {
        Some(Self {
            signal_id: body.get("signal_id")?.as_str()?.to_string(),
            kind: body.get("kind")?.as_str()?.to_string(),
            source_device: body.get("source_device")?.as_str()?.to_string(),
            source_spore: body
                .get("source_spore")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string(),
            wearer: body
                .get("wearer")
                .and_then(Value::as_str)
                .map(str::to_string),
            observed_at: body.get("observed_at_device")?.as_u64()?,
            quality: body.get("quality").and_then(Value::as_f64).unwrap_or(0.0),
            value: body.get("value")?.clone(),
        })
    }

    #[must_use]
    pub fn freshness(&self, now: u64, policy: Policy) -> Freshness {
        if self.observed_at > now {
            return Freshness::Stale;
        }
        let age = now - self.observed_at;
        if age <= policy.fresh_ms {
            Freshness::Fresh
        } else if age <= policy.stale_ms {
            Freshness::Stale
        } else {
            Freshness::Expired
        }
    }

    #[must_use]
    pub fn is_fresh(&self, now: u64, policy: Policy) -> bool {
        self.freshness(now, policy) == Freshness::Fresh
    }

    #[must_use]
    pub fn device_kind(&self) -> DeviceKind {
        let spore = self.source_spore.to_ascii_lowercase();
        let device = self.source_device.to_ascii_lowercase();
        if spore.contains("pinetime") || device.contains("pinetime") {
            DeviceKind::Watch
        } else if spore.contains("colmi") || spore.contains("ring") || device.contains("ring") {
            DeviceKind::Ring
        } else {
            DeviceKind::Other
        }
    }

    fn number(&self, key: &str) -> Option<f64> {
        self.value.get(key).and_then(Value::as_f64)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub value: Value,
    pub source_device: String,
    pub observed_at: u64,
    pub freshness: Freshness,
    pub quality: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Vitals {
    pub wearer: String,
    pub window_start: u64,
    pub window_end: u64,
    pub heart_rate: Option<Reading>,
    pub hrv: Option<Reading>,
    pub spo2: Option<Reading>,
    pub temperature_c: Option<Reading>,
    pub sleep: Option<Reading>,
    pub steps: BTreeMap<String, u64>,
    pub motion: Option<Reading>,
    pub primary_motion_device: Option<String>,
    pub devices: BTreeSet<String>,
    pub freshness: Freshness,
}

impl Vitals {
    #[must_use]
    pub fn to_json(&self) -> Value {
        let steps = json!({
            "by_device": self.steps,
            "primary_device": self.primary_motion_device,
        });
        json!({
            "schema": SCHEMA,
            "wearer": self.wearer,
            "window": {"start": self.window_start, "end": self.window_end},
            "heart_rate": self.heart_rate.as_ref().map(|reading| reading_json(reading, "bpm")),
            "hrv": self.hrv.as_ref().map(|reading| reading_json(reading, "rmssd_ms")),
            "spo2": self.spo2.as_ref().map(|reading| reading_json(reading, "percent")),
            "temperature_c": self.temperature_c.as_ref().map(|reading| reading_json(reading, "celsius")),
            "sleep": self.sleep.as_ref().map(|reading| reading_json(reading, "stage")),
            "steps": steps,
            "motion": self.motion.as_ref().map(|reading| reading_json(reading, "magnitude")),
            "devices": self.devices,
            "freshness": freshness_name(self.freshness),
            "policy_version": POLICY_VERSION,
            "source": SOURCE,
        })
    }
    #[must_use]
    pub fn to_view(&self) -> Value {
        let rings = activity_rings(self);
        let cards = vec![
            card("heart_rate", "Heart rate", self.heart_rate.as_ref(), "bpm"),
            card("hrv", "HRV", self.hrv.as_ref(), "ms"),
            card("spo2", "SpO2", self.spo2.as_ref(), "%"),
            card(
                "temperature",
                "Skin temperature",
                self.temperature_c.as_ref(),
                "C",
            ),
            card("sleep", "Sleep", self.sleep.as_ref(), ""),
            card("motion", "Motion", self.motion.as_ref(), "mg"),
        ];
        let steps: Vec<Value> = self
            .steps
            .iter()
            .map(|(device, count)| json!({"device": device, "count": count}))
            .collect();
        json!({
            "schema": "vitals.view.v1",
            "title": "Vitals",
            "wearer": self.wearer,
            "updated_at": self.window_end,
            "freshness": freshness_name(self.freshness),
            "empty": self.devices.is_empty(),
            "rings": rings,
            "cards": cards,
            // A GPUI renderer draws one row per device; there is deliberately
            // no `total` field to accidentally render a summed step count.
            "steps": {"rows": steps, "primary_device": self.primary_motion_device},
            "devices": self.devices,
            "policy_version": POLICY_VERSION,
            "source": SOURCE,
        })
    }
}

fn activity_rings(vitals: &Vitals) -> Vec<Value> {
    let move_value = vitals
        .primary_motion_device
        .as_ref()
        .and_then(|device| vitals.steps.get(device))
        .copied();
    let move_ring = json!({
        "id": "move",
        "label": "Move",
        "value": move_value,
        "max": MOVE_RING_MAX_STEPS,
        "unit": "steps",
        "source_device": vitals.primary_motion_device,
        "freshness": if move_value.is_some() {
            freshness_name(vitals.freshness)
        } else {
            "unknown"
        },
        "detail": "primary device only",
    });

    let heart_value = vitals
        .heart_rate
        .as_ref()
        .and_then(|reading| reading.value.as_u64());
    let heart_ring = json!({
        "id": "heart",
        "label": "Heart",
        "value": heart_value,
        "max": HEART_RING_MAX_BPM,
        "unit": "bpm",
        "source_device": vitals.heart_rate.as_ref().map(|reading| reading.source_device.clone()),
        "freshness": vitals.heart_rate.as_ref().map(|reading| freshness_name(reading.freshness)).unwrap_or("unknown"),
    });

    let oxygen_value = vitals
        .spo2
        .as_ref()
        .and_then(|reading| reading.value.as_u64());
    let oxygen_ring = json!({
        "id": "oxygen",
        "label": "Oxygen",
        "value": oxygen_value,
        "max": OXYGEN_RING_MAX_PERCENT,
        "unit": "%",
        "source_device": vitals.spo2.as_ref().map(|reading| reading.source_device.clone()),
        "freshness": vitals.spo2.as_ref().map(|reading| freshness_name(reading.freshness)).unwrap_or("unknown"),
    });

    vec![move_ring, heart_ring, oxygen_ring]
}

fn card(id: &str, label: &str, reading: Option<&Reading>, unit: &str) -> Value {
    let Some(reading) = reading else {
        return json!({
            "id": id,
            "label": label,
            "value": null,
            "unit": unit,
            "source_device": null,
            "freshness": "unknown",
        });
    };
    json!({
        "id": id,
        "label": label,
        "value": reading.value,
        "unit": unit,
        "source_device": reading.source_device,
        "freshness": freshness_name(reading.freshness),
        "quality": reading.quality,
    })
}

fn reading_json(reading: &Reading, key: &str) -> Value {
    let mut object = serde_json::Map::new();
    object.insert(key.to_string(), reading.value.clone());
    object.insert(
        "source_device".to_string(),
        Value::String(reading.source_device.clone()),
    );
    object.insert("observed_at".to_string(), Value::from(reading.observed_at));
    object.insert(
        "freshness".to_string(),
        Value::String(freshness_name(reading.freshness).to_string()),
    );
    object.insert("quality".to_string(), Value::from(reading.quality));
    Value::Object(object)
}

#[must_use]
pub fn fuse(signals: &[Signal], wearer: &str, now: u64, policy: Policy) -> Vitals {
    let visible: Vec<&Signal> = signals
        .iter()
        .filter(|signal| signal.wearer.as_deref() == Some(wearer))
        .collect();
    let devices: BTreeSet<String> = visible
        .iter()
        .map(|signal| signal.source_device.clone())
        .collect();
    let freshness = if visible.iter().any(|signal| signal.is_fresh(now, policy)) {
        Freshness::Fresh
    } else if visible
        .iter()
        .any(|signal| signal.freshness(now, policy) == Freshness::Stale)
    {
        Freshness::Stale
    } else {
        Freshness::Expired
    };

    let ring_hr = pick(&visible, "body.hr.v1", DeviceKind::Ring, now, policy);
    let watch_hr = pick(&visible, "body.hr.v1", DeviceKind::Watch, now, policy);
    let heart_rate = if policy.ring_primary {
        ring_hr.or(watch_hr)
    } else {
        watch_hr.or(ring_hr)
    }
    .map(|signal| reading(signal, "bpm"));

    let hrv = pick(&visible, "body.hrv.v1", DeviceKind::Ring, now, policy)
        .map(|signal| reading(signal, "rmssd_ms"));
    let spo2 = pick(&visible, "body.spo2.v1", DeviceKind::Ring, now, policy)
        .map(|signal| reading(signal, "percent"));
    let temperature_c = pick(&visible, "body.temp.v1", DeviceKind::Ring, now, policy)
        .map(|signal| reading(signal, "celsius"));

    let ring_sleep = pick(&visible, "body.sleep.v1", DeviceKind::Ring, now, policy);
    let watch_sleep = pick(&visible, "body.sleep.v1", DeviceKind::Watch, now, policy);
    let sleep = if policy.ring_primary {
        ring_sleep.or(watch_sleep)
    } else {
        watch_sleep.or(ring_sleep)
    }
    .map(|signal| reading(signal, "stage"));

    let watch_motion = pick(&visible, "body.motion.v1", DeviceKind::Watch, now, policy);
    let ring_motion = pick(&visible, "body.motion.v1", DeviceKind::Ring, now, policy);
    let motion = watch_motion.or(ring_motion);
    let primary_motion_device = motion.map(|signal| signal.source_device.clone());
    let motion = motion.map(motion_reading);

    let steps = steps_by_device(&visible, now, policy);
    let window_start = visible
        .iter()
        .map(|signal| signal.observed_at)
        .min()
        .unwrap_or(now);

    Vitals {
        wearer: wearer.to_string(),
        window_start,
        window_end: now,
        heart_rate,
        hrv,
        spo2,
        temperature_c,
        sleep,
        steps,
        motion,
        primary_motion_device,
        devices,
        freshness,
    }
}

fn pick<'a>(
    signals: &[&'a Signal],
    kind: &str,
    device_kind: DeviceKind,
    now: u64,
    policy: Policy,
) -> Option<&'a Signal> {
    signals
        .iter()
        .copied()
        .filter(|signal| {
            signal.kind == kind
                && signal.device_kind() == device_kind
                && signal.is_fresh(now, policy)
        })
        .max_by_key(|signal| signal.observed_at)
}

fn reading(signal: &Signal, key: &str) -> Reading {
    Reading {
        value: signal.value.get(key).cloned().unwrap_or(Value::Null),
        source_device: signal.source_device.clone(),
        observed_at: signal.observed_at,
        freshness: Freshness::Fresh,
        quality: signal.quality,
    }
}

fn motion_reading(signal: &Signal) -> Reading {
    let x = signal.number("x").unwrap_or(0.0);
    let y = signal.number("y").unwrap_or(0.0);
    let z = signal.number("z").unwrap_or(0.0);
    Reading {
        value: json!((x * x + y * y + z * z).sqrt()),
        source_device: signal.source_device.clone(),
        observed_at: signal.observed_at,
        freshness: Freshness::Fresh,
        quality: signal.quality,
    }
}

/// Steps are only ever reported per device. There is intentionally no total.
#[must_use]
pub fn steps_by_device(signals: &[&Signal], now: u64, policy: Policy) -> BTreeMap<String, u64> {
    let mut latest: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for signal in signals {
        if signal.kind != "body.steps.v1" || !signal.is_fresh(now, policy) {
            continue;
        }
        let Some(count) = signal.value.get("count").and_then(Value::as_u64) else {
            continue;
        };
        let entry = latest
            .entry(signal.source_device.clone())
            .or_insert((signal.observed_at, count));
        if signal.observed_at >= entry.0 {
            *entry = (signal.observed_at, count);
        }
    }
    latest
        .into_iter()
        .map(|(device, (_, count))| (device, count))
        .collect()
}

fn freshness_name(freshness: Freshness) -> &'static str {
    match freshness {
        Freshness::Fresh => "fresh",
        Freshness::Stale => "stale",
        Freshness::Expired => "expired",
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SeedRecord {
    pub collection: &'static str,
    pub id: String,
    pub schema: &'static str,
    pub body: Value,
}

/// Synthetic vitals for the dev host and any GPUI viewer. The owner has a
/// watch and a ring; the guest has a second ring so the wearer boundary is
/// exercised by real data rather than a test-only branch.
#[must_use]
pub fn seed(now: u64) -> Vec<SeedRecord> {
    let observed = now.saturating_sub(1_000);
    let mut records = Vec::new();
    let mut push =
        |device: &str, spore: &str, wearer: &str, kind: &str, value: Value, privacy: &str| {
            let id = format!("seed:{wearer}:{device}:{kind}:{now}");
            records.push(SeedRecord {
                collection: "signal",
                id: id.clone(),
                schema: "signal.sample.v1",
                body: json!({
                    "signal_id": id,
                    "kind": kind,
                    "schema_version": 1,
                    "source_device": device,
                    "source_spore": spore,
                    "wearer": wearer,
                    "observed_at_device": observed,
                    "received_at_host": observed,
                    "monotonic_age_ms": 1_000,
                    "clock_uncertainty_ms": null,
                    "sync_epoch": 0,
                    "quality": 1.0,
                    "provenance": "simulated",
                    "model_version": "vitals-seed@0.1.0",
                    "privacy_class": privacy,
                    "retention_class": "raw",
                    "value": value,
                }),
            });
        };

    // Owner watch: motion/fallback HR/steps.
    push(
        "sim-pinetime",
        "dev.fungos.pinetime",
        "owner",
        "body.hr.v1",
        json!({"bpm": 64, "contact": true}),
        "health",
    );
    push(
        "sim-pinetime",
        "dev.fungos.pinetime",
        "owner",
        "body.steps.v1",
        json!({"count": 4_231}),
        "health",
    );
    push(
        "sim-pinetime",
        "dev.fungos.pinetime",
        "owner",
        "body.motion.v1",
        json!({"x": 12, "y": -4, "z": 1_002}),
        "health",
    );

    // Owner ring: primary overnight HR and the ring-only vitals.
    push(
        "sim-ring",
        "dev.fungos.colmi-ring",
        "owner",
        "body.hr.v1",
        json!({"bpm": 58, "contact": true}),
        "health",
    );
    push(
        "sim-ring",
        "dev.fungos.colmi-ring",
        "owner",
        "body.hrv.v1",
        json!({"rmssd_ms": 48, "sdnn_ms": 52}),
        "health",
    );
    push(
        "sim-ring",
        "dev.fungos.colmi-ring",
        "owner",
        "body.spo2.v1",
        json!({"percent": 97}),
        "health",
    );
    push(
        "sim-ring",
        "dev.fungos.colmi-ring",
        "owner",
        "body.temp.v1",
        json!({"celsius": 36.6}),
        "health",
    );
    push(
        "sim-ring",
        "dev.fungos.colmi-ring",
        "owner",
        "body.sleep.v1",
        json!({"stage": "deep"}),
        "health",
    );
    push(
        "sim-ring",
        "dev.fungos.colmi-ring",
        "owner",
        "body.steps.v1",
        json!({"count": 9_000}),
        "health",
    );

    // Guest ring: never visible in the owner's summary.
    push(
        "sim-ring-guest",
        "dev.fungos.colmi-ring",
        "guest",
        "body.hr.v1",
        json!({"bpm": 88, "contact": true}),
        "health",
    );
    push(
        "sim-ring-guest",
        "dev.fungos.colmi-ring",
        "guest",
        "body.steps.v1",
        json!({"count": 7_000}),
        "health",
    );

    for (device, wearer, battery) in [
        ("sim-pinetime", "owner", 82),
        ("sim-ring", "owner", 74),
        ("sim-ring-guest", "guest", 61),
    ] {
        records.push(SeedRecord {
            collection: "device.state",
            id: device.to_string(),
            schema: "device.state.v1",
            body: json!({
                "device_id": device,
                "battery_percent": battery,
                "wearer": wearer,
                "observed_at_device": observed,
                "received_at_host": observed,
                "provenance": "simulated",
                "source_spore": SOURCE,
            }),
        });
    }
    records
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded(now: u64) -> Vec<Signal> {
        seed(now)
            .into_iter()
            .filter(|record| record.collection == "signal")
            .filter_map(|record| Signal::from_record(&record.body))
            .collect()
    }

    #[test]
    fn ring_is_primary_for_overnight_hr() {
        let now = 10_000;
        let vitals = fuse(&seeded(now), "owner", now, Policy::default());
        assert_eq!(vitals.heart_rate.as_ref().unwrap().value, json!(58));
        assert_eq!(vitals.hrv.as_ref().unwrap().value, json!(48));
        assert_eq!(vitals.spo2.as_ref().unwrap().value, json!(97));
    }

    #[test]
    fn watch_is_the_motion_primary() {
        let now = 10_000;
        let vitals = fuse(&seeded(now), "owner", now, Policy::default());
        assert_eq!(
            vitals.primary_motion_device.as_deref(),
            Some("sim-pinetime")
        );
    }

    #[test]
    fn sleep_stage_is_selected_not_averaged() {
        let now = 10_000;
        let mut signals = seeded(now);
        signals.push(Signal {
            signal_id: "watch-sleep".into(),
            kind: "body.sleep.v1".into(),
            source_device: "sim-pinetime".into(),
            source_spore: "dev.fungos.pinetime".into(),
            wearer: Some("owner".into()),
            observed_at: now - 500,
            quality: 1.0,
            value: json!({"stage": "light"}),
        });
        let vitals = fuse(&signals, "owner", now, Policy::default());
        assert_eq!(vitals.sleep.unwrap().value, json!("deep"));
    }

    #[test]
    fn steps_are_per_device_never_summed() {
        let now = 10_000;
        let vitals = fuse(&seeded(now), "owner", now, Policy::default());
        assert_eq!(vitals.steps.get("sim-pinetime"), Some(&4_231));
        assert_eq!(vitals.steps.get("sim-ring"), Some(&9_000));
        assert_eq!(vitals.steps.len(), 2);
        assert!(!vitals.steps.values().any(|count| *count == 13_231));
    }

    #[test]
    fn guest_signals_do_not_leak_into_the_owner() {
        let now = 10_000;
        let vitals = fuse(&seeded(now), "owner", now, Policy::default());
        assert!(!vitals.devices.contains("sim-ring-guest"));
        assert_eq!(vitals.heart_rate.unwrap().value, json!(58));
    }

    #[test]
    fn stale_is_unknown_not_zero() {
        let signals = vec![Signal {
            signal_id: "stale-hr".into(),
            kind: "body.hr.v1".into(),
            source_device: "sim-ring".into(),
            source_spore: "dev.fungos.colmi-ring".into(),
            wearer: Some("owner".into()),
            observed_at: 1_000,
            quality: 1.0,
            value: json!({"bpm": 72}),
        }];
        let now = 1_000 + Policy::default().fresh_ms + 1;
        let vitals = fuse(&signals, "owner", now, Policy::default());
        assert!(vitals.heart_rate.is_none());
        assert_eq!(vitals.freshness, Freshness::Stale);
    }

    #[test]
    fn summary_has_the_expected_schema_and_policy() {
        let now = 10_000;
        let json = fuse(&seeded(now), "owner", now, Policy::default()).to_json();
        assert_eq!(json["schema"], SCHEMA);
        assert_eq!(json["policy_version"], POLICY_VERSION);
        assert_eq!(json["wearer"], "owner");
        assert_eq!(json["steps"]["by_device"]["sim-ring"], 9_000);
    }

    #[test]
    fn seed_is_deterministic() {
        assert_eq!(seed(42), seed(42));
    }

    #[test]
    fn view_has_cards_and_no_step_total() {
        let now = 10_000;
        let view = fuse(&seeded(now), "owner", now, Policy::default()).to_view();
        assert_eq!(view["schema"], "vitals.view.v1");
        let cards = view["cards"].as_array().expect("cards");
        assert!(cards.iter().any(|card| {
            card["id"] == "heart_rate" && card["value"] == 58 && card["source_device"] == "sim-ring"
        }));
        assert_eq!(view["steps"]["rows"].as_array().unwrap().len(), 2);
        assert!(view["steps"].get("total").is_none());
        assert_eq!(view["rings"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn view_rings_use_primary_steps_not_a_total() {
        let now = 10_000;
        let view = fuse(&seeded(now), "owner", now, Policy::default()).to_view();
        let rings = view["rings"].as_array().expect("rings");
        let move_ring = rings.iter().find(|ring| ring["id"] == "move").unwrap();
        assert_eq!(move_ring["value"], 4_231);
        assert_eq!(move_ring["source_device"], "sim-pinetime");
        assert!(move_ring.get("total").is_none());

        let heart_ring = rings.iter().find(|ring| ring["id"] == "heart").unwrap();
        assert_eq!(heart_ring["value"], 58);
        assert_eq!(heart_ring["max"], 180);

        let oxygen_ring = rings.iter().find(|ring| ring["id"] == "oxygen").unwrap();
        assert_eq!(oxygen_ring["value"], 97);
        assert_eq!(oxygen_ring["max"], 100);
    }

    #[test]
    fn view_marks_missing_reading_null() {
        let now = 10_000;
        let view = fuse(&[], "owner", now, Policy::default()).to_view();
        let cards = view["cards"].as_array().expect("cards");
        assert!(cards.iter().all(|card| card["value"].is_null()));
        let rings = view["rings"].as_array().expect("rings");
        assert!(rings.iter().all(|ring| ring["value"].is_null()));
        assert_eq!(view["empty"], true);
    }
}
