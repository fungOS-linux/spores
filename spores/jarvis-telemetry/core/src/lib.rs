//! Deterministic presence/attention fusion for the JARVIS telemetry Spore.
//!
//! The Spore reads `signal.sample.v1` records written by device Spores and
//! publishes latest-value presence/attention frames. This crate is pure Rust so
//! the policy is testable without wasm or hardware.
//!
//! The rules are deliberately conservative:
//! * a signal belongs to one wearer; a guest's ring is not the owner's data;
//! * step counts stay per device and are never summed;
//! * stale/expired values are unknown, not zero;
//! * raw audio never becomes a bus payload.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

pub const SOURCE: &str = "dev.fungos.jarvis-telemetry";
pub const PRESENCE_KIND: &str = "presence.state.v1";
pub const ATTENTION_KIND: &str = "attention.state.v1";

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
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            fresh_ms: 60_000,
            stale_ms: 300_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Signal {
    pub signal_id: String,
    pub kind: String,
    pub source_device: String,
    pub wearer: Option<String>,
    pub observed_at: u64,
    pub quality: f64,
    pub value: Value,
}

impl Signal {
    /// Parse the body of a `signal.sample.v1` record. A body that does not carry
    /// the minimum identity/time fields is not a signal and is skipped, not
    /// guessed at.
    #[must_use]
    pub fn from_record(body: &Value) -> Option<Self> {
        Some(Self {
            signal_id: body.get("signal_id")?.as_str()?.to_string(),
            kind: body.get("kind")?.as_str()?.to_string(),
            source_device: body.get("source_device")?.as_str()?.to_string(),
            wearer: body
                .get("wearer")
                .and_then(Value::as_str)
                .map(str::to_string),
            observed_at: body.get("observed_at_device")?.as_u64()?,
            quality: body.get("quality").and_then(Value::as_f64).unwrap_or(0.0),
            value: body.get("value")?.clone(),
        })
    }

    /// A future timestamp is not fresh; it is a clock disagreement and is
    /// treated as stale until the source proves otherwise.
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

    /// Heart rate is unknown when stale or absent. There is intentionally no
    /// `unwrap_or(0.0)`.
    #[must_use]
    pub fn bpm(&self, now: u64, policy: Policy) -> Option<f64> {
        if !self.is_fresh(now, policy) {
            return None;
        }
        self.value.get("bpm").and_then(Value::as_f64)
    }

    #[must_use]
    pub fn steps(&self, now: u64, policy: Policy) -> Option<u64> {
        if !self.is_fresh(now, policy) {
            return None;
        }
        self.value.get("count").and_then(Value::as_u64)
    }

    #[must_use]
    pub fn motion_magnitude(&self, now: u64, policy: Policy) -> Option<f64> {
        if !self.is_fresh(now, policy) {
            return None;
        }
        let x = self.value.get("x")?.as_f64()?;
        let y = self.value.get("y")?.as_f64()?;
        let z = self.value.get("z")?.as_f64()?;
        Some((x * x + y * y + z * z).sqrt())
    }
}

/// Latest step count per device. Two devices are two rows, never one total:
/// the same walk can be seen by a watch and a phone, and summing double-counts.
#[must_use]
pub fn steps_by_device(
    signals: &[Signal],
    wearer: &str,
    now: u64,
    policy: Policy,
) -> BTreeMap<String, u64> {
    let mut by_device = BTreeMap::new();
    for signal in signals {
        if signal.wearer.as_deref() != Some(wearer) || signal.kind != "body.steps.v1" {
            continue;
        }
        if let Some(count) = signal.steps(now, policy) {
            by_device
                .entry(signal.source_device.clone())
                .and_modify(|existing: &mut u64| *existing = (*existing).max(count))
                .or_insert(count);
        }
    }
    by_device
}

#[derive(Debug, Clone)]
pub struct Fused {
    pub presence: Value,
    pub attention: Value,
    pub evidence: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusError {
    RawAudio,
    AudioNeedsBlobRef,
    Serialize,
}

/// Publish only references for audio. Raw bytes belong in Records/blob storage,
/// never on the live bus.
pub fn bus_payload(kind: &str, body: &Value) -> Result<Vec<u8>, BusError> {
    if kind.starts_with("audio.") {
        if body.get("raw").is_some() || body.get("bytes").is_some() {
            return Err(BusError::RawAudio);
        }
        if body.get("blob_ref").and_then(Value::as_str).is_none() {
            return Err(BusError::AudioNeedsBlobRef);
        }
    }
    serde_json::to_vec(body).map_err(|_| BusError::Serialize)
}

/// Fuse visible signals into latest-value presence and attention. `allow_synthetic`
/// is the explicit development switch: with no records it cycles a deterministic
/// fake so the JARVIS side can run; the evidence always says `synthetic: true`.
#[must_use]
pub fn fuse(
    signals: &[Signal],
    wearer: &str,
    now: u64,
    policy: Policy,
    allow_synthetic: bool,
) -> Fused {
    let visible: Vec<&Signal> = signals
        .iter()
        .filter(|signal| signal.wearer.as_deref() == Some(wearer))
        .collect();
    let fresh = visible
        .iter()
        .filter(|signal| signal.freshness(now, policy) == Freshness::Fresh)
        .count();
    let stale = visible
        .iter()
        .any(|signal| signal.freshness(now, policy) == Freshness::Stale);
    let devices: BTreeSet<String> = visible
        .iter()
        .map(|signal| signal.source_device.clone())
        .collect();
    let steps = steps_by_device(signals, wearer, now, policy);

    let synthetic = visible.is_empty() && allow_synthetic;
    let (presence_state, freshness, presence_confidence) = if synthetic {
        synthetic_presence(now)
    } else if fresh > 0 {
        ("present", Freshness::Fresh, 0.9)
    } else if stale {
        ("unknown", Freshness::Stale, 0.4)
    } else if visible.is_empty() {
        ("unknown", Freshness::Expired, 0.0)
    } else {
        ("away", Freshness::Expired, 0.7)
    };

    let (attention_state, attention_confidence) = if synthetic {
        synthetic_attention(now)
    } else if presence_state == "present" {
        attention_from_signals(&visible, now, policy)
    } else {
        ("unknown", 0.0)
    };

    let evidence = json!({
        "wearer": wearer,
        "signals": visible.len(),
        "fresh": fresh,
        "stale": stale,
        "devices": devices,
        "steps_by_device": steps,
        "synthetic": synthetic,
    });

    let presence = json!({
        "schema": PRESENCE_KIND,
        "state": presence_state,
        "wearer": wearer,
        "observed_at": now,
        "source": SOURCE,
        "confidence": presence_confidence,
        "freshness": freshness_name(freshness),
        "evidence": evidence,
    });
    let attention = json!({
        "schema": ATTENTION_KIND,
        "state": attention_state,
        "wearer": wearer,
        "observed_at": now,
        "source": SOURCE,
        "confidence": attention_confidence,
        "freshness": freshness_name(freshness),
        "evidence": evidence,
    });

    Fused {
        presence,
        attention,
        evidence,
    }
}

fn attention_from_signals(visible: &[&Signal], now: u64, policy: Policy) -> (&'static str, f64) {
    let bpm = visible
        .iter()
        .filter_map(|signal| signal.bpm(now, policy))
        .next_back();
    let motion = visible
        .iter()
        .filter_map(|signal| signal.motion_magnitude(now, policy))
        .next_back();
    match (bpm, motion) {
        (Some(bpm), Some(motion)) if bpm < 55.0 && motion < 80.0 => ("asleep", 0.6),
        (Some(bpm), _) if bpm > 95.0 => ("focused", 0.5),
        (Some(_), Some(motion)) if motion > 150.0 => ("focused", 0.5),
        (Some(_), _) => ("available", 0.5),
        _ => ("available", 0.3),
    }
}

fn synthetic_presence(now: u64) -> (&'static str, Freshness, f64) {
    match (now / 60_000) % 6 {
        0..=3 => ("present", Freshness::Fresh, 0.4),
        4 => ("away", Freshness::Expired, 0.4),
        _ => ("unknown", Freshness::Expired, 0.2),
    }
}

fn synthetic_attention(now: u64) -> (&'static str, f64) {
    match (now / 60_000) % 6 {
        0 | 1 => ("focused", 0.4),
        2 | 3 => ("available", 0.4),
        4 => ("asleep", 0.3),
        _ => ("unknown", 0.2),
    }
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

/// Synthetic records for a hardware-free JARVIS loop. The guest ring is
/// deliberate: it is how the owner/guest boundary gets exercised.
#[must_use]
pub fn seed(now: u64) -> Vec<SeedRecord> {
    let mut records = Vec::new();
    for (device, wearer, bpm, steps, motion) in [
        ("sim-pinetime", "owner", 64, 4_231, (12, -4, 1_002)),
        ("sim-ring", "guest", 88, 9_000, (0, 0, 980)),
    ] {
        for (kind, value, privacy, retention) in [
            (
                "body.hr.v1",
                json!({"bpm": bpm, "contact": true}),
                "health",
                "raw",
            ),
            (
                "body.steps.v1",
                json!({"count": steps}),
                "health",
                "summary",
            ),
            (
                "body.motion.v1",
                json!({"x": motion.0, "y": motion.1, "z": motion.2}),
                "health",
                "raw",
            ),
        ] {
            let id = format!("seed:{device}:{kind}:{now}");
            records.push(SeedRecord {
                collection: "signal",
                id: id.clone(),
                schema: "signal.sample.v1",
                body: json!({
                    "signal_id": id,
                    "kind": kind,
                    "schema_version": 1,
                    "source_device": device,
                    "source_spore": SOURCE,
                    "wearer": wearer,
                    "observed_at_device": now.saturating_sub(1_000),
                    "received_at_host": now.saturating_sub(1_000),
                    "monotonic_age_ms": 1_000,
                    "clock_uncertainty_ms": null,
                    "sync_epoch": 0,
                    "quality": 1.0,
                    "provenance": "simulated",
                    "model_version": "jarvis-telemetry-seed@0.1.0",
                    "privacy_class": privacy,
                    "retention_class": retention,
                    "value": value,
                }),
            });
        }
        records.push(SeedRecord {
            collection: "device.state",
            id: device.to_string(),
            schema: "device.state.v1",
            body: json!({
                "device_id": device,
                "model": if device == "sim-pinetime" { "PineTime" } else { "COLMI Ring 2" },
                "battery_percent": if device == "sim-pinetime" { 82 } else { 74 },
                "wearer": wearer,
                "observed_at_device": now.saturating_sub(1_000),
                "received_at_host": now.saturating_sub(1_000),
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

    fn signals_from_seed(now: u64) -> Vec<Signal> {
        seed(now)
            .into_iter()
            .filter(|record| record.collection == "signal")
            .filter_map(|record| Signal::from_record(&record.body))
            .collect()
    }

    #[test]
    fn seeded_owner_fusion_does_not_see_the_guest() {
        let now = 10_000;
        let fused = fuse(
            &signals_from_seed(now),
            "owner",
            now,
            Policy::default(),
            false,
        );
        assert_eq!(fused.presence["state"], "present");
        let devices = fused.evidence["devices"].as_array().expect("devices");
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0], "sim-pinetime");
    }

    #[test]
    fn steps_stay_per_device_and_are_not_summed() {
        let now = 10_000;
        let signals = signals_from_seed(now)
            .into_iter()
            .map(|mut signal| {
                // Same wearer, two devices: a watch and a ring.
                signal.wearer = Some("owner".to_string());
                signal
            })
            .collect::<Vec<_>>();
        let steps = steps_by_device(&signals, "owner", now, Policy::default());
        assert_eq!(steps.get("sim-pinetime"), Some(&4_231));
        assert_eq!(steps.get("sim-ring"), Some(&9_000));
        assert_eq!(steps.len(), 2);
        assert!(!steps.values().any(|value| *value == 13_231));
    }

    #[test]
    fn stale_values_are_unknown_not_zero() {
        let signal = Signal {
            signal_id: "s".into(),
            kind: "body.hr.v1".into(),
            source_device: "watch".into(),
            wearer: Some("owner".into()),
            observed_at: 1_000,
            quality: 1.0,
            value: json!({"bpm": 72}),
        };
        let now = 1_000 + Policy::default().fresh_ms + 1;
        assert_eq!(signal.freshness(now, Policy::default()), Freshness::Stale);
        assert_eq!(signal.bpm(now, Policy::default()), None);
        assert_ne!(signal.bpm(now, Policy::default()), Some(0.0));
        let fused = fuse(&[signal], "owner", now, Policy::default(), false);
        assert_eq!(fused.presence["state"], "unknown");
    }

    #[test]
    fn raw_audio_is_refused_but_a_blob_reference_is_allowed() {
        assert_eq!(
            bus_payload("audio.segment.v1", &json!({"raw": [1, 2, 3]})),
            Err(BusError::RawAudio)
        );
        assert!(bus_payload("audio.segment.v1", &json!({"blob_ref": "blob:sha256:abc"})).is_ok());
        assert_eq!(
            bus_payload("audio.segment.v1", &json!({"duration_ms": 10})),
            Err(BusError::AudioNeedsBlobRef)
        );
    }

    #[test]
    fn synthetic_mode_is_explicit_in_the_evidence() {
        let now = 0;
        let fused = fuse(&[], "owner", now, Policy::default(), true);
        assert_eq!(fused.evidence["synthetic"], true);
        assert_eq!(fused.presence["state"], "present");
    }

    #[test]
    fn seed_is_deterministic() {
        assert_eq!(seed(42), seed(42));
    }
}
