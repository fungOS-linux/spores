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

/// Step deltas are compared in five-minute buckets. Devices that have no
/// sample in a bucket cannot contribute to it.
pub const STEP_BUCKET_MS: u64 = 5 * 60 * 1_000;
const DAY_MS: u64 = 24 * 60 * 60 * 1_000;

/// Windowed step estimate built from cumulative per-device counters.
///
/// A counter decrease is treated as a reset; each delta is attributed to the
/// five-minute bucket containing the later sample. The first sample seeds its
/// bucket with the count accumulated before observation. The day starts at UTC
/// midnight; local timezone is a future profile input. `confidence` is the
/// fraction of day buckets covered by any device, not a claim about step
/// accuracy.
#[derive(Debug, Clone, PartialEq)]
pub struct StepEstimate {
    pub value: Option<u64>,
    pub source_device: Option<String>,
    pub policy: &'static str,
    pub confidence: f64,
    pub buckets_covered: u32,
    pub buckets_total: u32,
    pub contributors: BTreeMap<String, u64>,
    pub window_start: u64,
    pub window_end: u64,
}

impl StepEstimate {
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "value": self.value,
            "source_device": self.source_device,
            "policy": self.policy,
            "confidence": self.confidence,
            "coverage": {
                "covered_buckets": self.buckets_covered,
                "total_buckets": self.buckets_total,
                "bucket_ms": STEP_BUCKET_MS,
            },
            "contributors": self.contributors,
            "window": {"start": self.window_start, "end": self.window_end},
        })
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
    pub step_estimate: StepEstimate,
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
            "estimate": self.step_estimate.to_json(),
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
        let step_estimate = self.step_estimate.to_json();
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
            "steps": {
                "rows": steps,
                "primary_device": self.primary_motion_device,
                "estimate": step_estimate,
            },
            "devices": self.devices,
            "policy_version": POLICY_VERSION,
            "source": SOURCE,
        })
    }
}

fn activity_rings(vitals: &Vitals) -> Vec<Value> {
    let move_value = vitals.step_estimate.value;
    let move_source = vitals.step_estimate.source_device.as_deref();
    let move_ring = json!({
        "id": "move",
        "label": "Move",
        "value": move_value,
        "max": MOVE_RING_MAX_STEPS,
        "unit": "steps",
        "source_device": move_source,
        "freshness": if move_value.is_some() {
            freshness_name(vitals.freshness)
        } else {
            "unknown"
        },
        "detail": vitals.step_estimate.policy,
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
    let step_estimate = estimate_steps(&visible, now, &steps, primary_motion_device.as_deref());
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
        step_estimate,
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

fn estimate_steps(
    signals: &[&Signal],
    now: u64,
    latest: &BTreeMap<String, u64>,
    primary: Option<&str>,
) -> StepEstimate {
    let day_start = now - now % DAY_MS;
    let buckets_total = ((now - day_start) / STEP_BUCKET_MS + 1) as u32;

    let mut samples: BTreeMap<String, Vec<(u64, u64)>> = BTreeMap::new();
    for signal in signals {
        if signal.kind != "body.steps.v1" || signal.observed_at > now {
            continue;
        }
        let Some(count) = signal.value.get("count").and_then(Value::as_u64) else {
            continue;
        };
        samples
            .entry(signal.source_device.clone())
            .or_default()
            .push((signal.observed_at, count));
    }

    let mut buckets: BTreeMap<u64, BTreeMap<String, u64>> = BTreeMap::new();
    for (device, mut points) in samples {
        points.sort_by_key(|(at, _)| *at);
        points.dedup_by_key(|(at, _)| *at);
        let Some(first) = points.first().copied() else {
            continue;
        };

        // A cumulative counter's first sample already contains steps since the
        // day boundary. Seed that bucket so the estimate does not start at zero
        // just because the first sample arrived late.
        if first.0 >= day_start && first.1 > 0 {
            let bucket = first.0 - first.0 % STEP_BUCKET_MS;
            if bucket >= day_start {
                buckets
                    .entry(bucket)
                    .or_default()
                    .entry(device.clone())
                    .and_modify(|total| *total += first.1)
                    .or_insert(first.1);
            }
        }

        let mut previous = first;
        for current in points.into_iter().skip(1) {
            if current.0 <= previous.0 || current.0 > now {
                continue;
            }
            let delta = if current.1 >= previous.1 {
                current.1 - previous.1
            } else {
                // A counter reset is new steps since the reset, not a negative
                // delta. The bucket is still attributable to the later sample.
                current.1
            };
            if delta > 0 && current.0 >= day_start {
                let bucket = current.0 - current.0 % STEP_BUCKET_MS;
                if bucket >= day_start {
                    buckets
                        .entry(bucket)
                        .or_default()
                        .entry(device.clone())
                        .and_modify(|total| *total += delta)
                        .or_insert(delta);
                }
            }
            previous = current;
        }
    }

    let mut value = 0u64;
    let mut contributors = BTreeMap::new();
    let mut covered = 0u32;
    for deltas in buckets.values() {
        let Some((device, delta)) = best_bucket(deltas, primary) else {
            continue;
        };
        if delta == 0 {
            continue;
        }
        covered += 1;
        value = value.saturating_add(delta);
        *contributors.entry(device.to_string()).or_insert(0) += delta;
    }

    if covered > 0 {
        return StepEstimate {
            value: Some(value),
            source_device: Some("fused".to_string()),
            policy: "per-bucket-max",
            confidence: f64::from(covered) / f64::from(buckets_total.max(1)),
            buckets_covered: covered,
            buckets_total,
            contributors,
            window_start: day_start,
            window_end: now,
        };
    }

    if let Some((device, value)) = best_step_total(latest, primary) {
        let mut contributors = BTreeMap::new();
        contributors.insert(device.to_string(), value);
        return StepEstimate {
            value: Some(value),
            source_device: Some(device.to_string()),
            policy: "max-fresh-device",
            confidence: 1.0,
            buckets_covered: 0,
            buckets_total,
            contributors,
            window_start: day_start,
            window_end: now,
        };
    }

    StepEstimate {
        value: None,
        source_device: None,
        policy: "max-fresh-device",
        confidence: 0.0,
        buckets_covered: 0,
        buckets_total,
        contributors: BTreeMap::new(),
        window_start: day_start,
        window_end: now,
    }
}

fn best_bucket<'a>(
    deltas: &'a BTreeMap<String, u64>,
    primary: Option<&str>,
) -> Option<(&'a str, u64)> {
    let mut best: Option<(&str, u64)> = None;
    for (device, delta) in deltas {
        let better = match best {
            None => true,
            Some((best_device, best_delta)) => {
                *delta > best_delta
                    || (*delta == best_delta
                        && primary == Some(device.as_str())
                        && primary != Some(best_device))
            }
        };
        if better {
            best = Some((device.as_str(), *delta));
        }
    }
    best
}

fn best_step_total<'a>(
    latest: &'a BTreeMap<String, u64>,
    primary: Option<&str>,
) -> Option<(&'a str, u64)> {
    let mut best: Option<(&str, u64)> = None;
    for (device, count) in latest {
        let better = match best {
            None => true,
            Some((best_device, best_count)) => {
                *count > best_count
                    || (*count == best_count
                        && primary == Some(device.as_str())
                        && primary != Some(best_device))
            }
        };
        if better {
            best = Some((device.as_str(), *count));
        }
    }
    best
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
        assert_eq!(json["steps"]["estimate"]["value"], 9_000);
        assert_eq!(json["steps"]["estimate"]["source_device"], "fused");
        assert_eq!(json["steps"]["estimate"]["policy"], "per-bucket-max");
    }

    #[test]
    fn seed_is_deterministic() {
        assert_eq!(seed(42), seed(42));
    }

    #[test]
    fn per_bucket_max_uses_historical_steps_and_not_a_total() {
        let now = 3_600_000;
        let mut signals = Vec::new();
        let mut push = |device: &str, at: u64, count: u64| {
            signals.push(Signal {
                signal_id: format!("{device}:{at}"),
                kind: "body.steps.v1".into(),
                source_device: device.into(),
                source_spore: "test".into(),
                wearer: Some("owner".into()),
                observed_at: at,
                quality: 1.0,
                value: json!({"count": count}),
            });
        };
        for (at, watch, ring) in [
            (1_500_000, 200, 120),
            (1_800_000, 400, 300),
            (2_100_000, 600, 500),
            (2_400_000, 800, 700),
        ] {
            push("sim-pinetime", at, watch);
            push("sim-ring", at, ring);
        }

        let vitals = fuse(&signals, "owner", now, Policy::default());
        assert!(vitals.steps.is_empty(), "samples are intentionally stale");
        assert_eq!(vitals.step_estimate.value, Some(800));
        assert_eq!(vitals.step_estimate.policy, "per-bucket-max");
        assert_eq!(vitals.step_estimate.source_device.as_deref(), Some("fused"));
        assert_eq!(vitals.step_estimate.buckets_covered, 4);
        assert_eq!(
            vitals.step_estimate.contributors.get("sim-pinetime"),
            Some(&800)
        );
        assert_ne!(vitals.step_estimate.value, Some(1_500));
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
    fn view_rings_use_highest_fresh_steps_not_a_total() {
        let now = 10_000;
        let view = fuse(&seeded(now), "owner", now, Policy::default()).to_view();
        let rings = view["rings"].as_array().expect("rings");
        let move_ring = rings.iter().find(|ring| ring["id"] == "move").unwrap();
        assert_eq!(move_ring["value"], 9_000);
        assert_eq!(move_ring["source_device"], "fused");
        assert!(move_ring.get("total").is_none());

        assert_eq!(view["steps"]["estimate"]["value"], 9_000);
        assert_eq!(view["steps"]["estimate"]["source_device"], "fused");
        assert_eq!(view["steps"]["estimate"]["policy"], "per-bucket-max");
        assert_eq!(view["steps"]["estimate"]["contributors"]["sim-ring"], 9_000);
        assert_ne!(view["steps"]["estimate"]["value"], 4_231 + 9_000);

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
        assert!(view["steps"]["estimate"]["value"].is_null());
        assert_eq!(view["empty"], true);
    }
}
