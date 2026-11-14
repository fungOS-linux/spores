//! What is true of any smartwatch, and nothing that is true of only one.
//!
//! A Spore that talks to a watch owns the radio session, the wearer identity,
//! and the durable record. The profile crate owns the wire: service UUIDs,
//! payload layouts, and which firmware has which service. This crate is the
//! seam between them, so a second watch is a new profile rather than a fork.
//!
//! Nothing here imports wasm: streams, envelopes and the identity rule are
//! testable on the host.

use serde_json::{json, Value};

/// A stream of observations a watch can emit. The Spore lets the wearer pick a
/// subset, so a control-heavy session need not turn on health data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stream {
    HeartRate,
    Steps,
    Motion,
    Media,
    Calls,
    Battery,
}

impl Stream {
    pub const ALL: [Self; 6] = [
        Self::HeartRate,
        Self::Steps,
        Self::Motion,
        Self::Media,
        Self::Calls,
        Self::Battery,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HeartRate => "hr",
            Self::Steps => "steps",
            Self::Motion => "motion",
            Self::Media => "media",
            Self::Calls => "calls",
            Self::Battery => "battery",
        }
    }

    /// The record kind this stream writes, if it writes one. Battery is device
    /// state, not a body signal, and has no kind.
    #[must_use]
    pub const fn kind(self) -> Option<&'static str> {
        match self {
            Self::HeartRate => Some("body.hr.v1"),
            Self::Steps => Some("body.steps.v1"),
            Self::Motion => Some("body.motion.v1"),
            Self::Media => Some("media.event.v1"),
            Self::Calls => Some("call.response.v1"),
            Self::Battery => None,
        }
    }

    /// Parse `hr,steps` or `all`. An unknown name is an error that names it; a
    /// typo must not silently subscribe to everything.
    pub fn parse(value: &str) -> Result<Vec<Self>, String> {
        let mut streams = Vec::new();
        for token in value.split(',').map(str::trim).filter(|t| !t.is_empty()) {
            if token.eq_ignore_ascii_case("all") {
                return Ok(Self::ALL.to_vec());
            }
            let stream = Self::ALL
                .into_iter()
                .find(|stream| stream.as_str() == token.to_ascii_lowercase())
                .ok_or_else(|| {
                    let known: Vec<_> = Self::ALL.iter().map(|s| s.as_str()).collect();
                    format!("unknown stream {token:?}; known: {}", known.join(", "))
                })?;
            if !streams.contains(&stream) {
                streams.push(stream);
            }
        }
        if streams.is_empty() {
            return Err("no streams selected".to_string());
        }
        Ok(streams)
    }
}

/// One observation, ready for the signal envelope.
#[derive(Debug, Clone, PartialEq)]
pub struct Signal {
    pub stream: Stream,
    pub value: Value,
    /// `health`, `presence`, …
    pub privacy_class: &'static str,
    /// `raw`, `summary`, …
    pub retention_class: &'static str,
    pub quality: f64,
}

impl Signal {
    #[must_use]
    pub fn kind(&self) -> &'static str {
        self.stream.kind().unwrap_or("unknown.v1")
    }
}

/// The signal envelope every watch Spore writes, whatever the hardware.
#[must_use]
pub fn signal_body(
    device_id: &str,
    wearer: &str,
    source_spore: &str,
    model_version: &str,
    signal: &Signal,
    observed_at_ms: u64,
    sequence: u64,
) -> Value {
    let signal_id = format!("{device_id}:{}:{observed_at_ms}:{sequence}", signal.kind());
    json!({
        "signal_id": signal_id,
        "kind": signal.kind(),
        "schema_version": 1,
        "source_device": device_id,
        "source_spore": source_spore,
        "wearer": wearer,
        "observed_at_device": observed_at_ms,
        "received_at_host": observed_at_ms,
        "monotonic_age_ms": 0,
        "clock_uncertainty_ms": null,
        "sync_epoch": 0,
        "quality": signal.quality,
        "provenance": "ble",
        "model_version": model_version,
        "privacy_class": signal.privacy_class,
        "retention_class": signal.retention_class,
        "value": signal.value,
    })
}

/// The device state record body. Battery lives here, not on a body stream.
#[must_use]
pub fn device_state_body(
    device_id: &str,
    wearer: &str,
    source_spore: &str,
    model: &str,
    firmware: &str,
    battery_percent: u8,
    observed_at_ms: u64,
) -> Value {
    json!({
        "device_id": device_id,
        "model": model,
        "firmware": firmware,
        "battery_percent": battery_percent,
        "wearer": wearer,
        "observed_at_device": observed_at_ms,
        "received_at_host": observed_at_ms,
        "provenance": "ble",
        "source_spore": source_spore,
    })
}

/// Identity comes from the device, never from the rotating BLE address.
///
/// The serial is the first choice. A firmware that exposes none may be given a
/// provisioned id by the caller; inventing one from a platform handle would
/// split the same watch into a new device on every host.
#[must_use]
pub fn device_id(serial: &str, provisioned: Option<&str>) -> Option<String> {
    let serial = serial.trim();
    if !serial.is_empty() {
        return Some(serial.to_string());
    }
    provisioned
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

/// A command the Spore can send. The profile decides whether the firmware on
/// the wrist is new enough for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    Watch,
    Notify,
    Media,
    Find,
    Time,
    Nav,
    Weather,
}

impl Intent {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Watch => "watch",
            Self::Notify => "notify",
            Self::Media => "media",
            Self::Find => "find",
            Self::Time => "time",
            Self::Nav => "nav",
            Self::Weather => "weather",
        }
    }
}

/// What a watch profile has to answer for the Spore to stay hardware-agnostic.
pub trait WatchProfile {
    /// Profile name, for the snapshot.
    fn name(&self) -> &'static str;

    /// Streams this profile can emit. `--streams` narrows this; it never
    /// widens it.
    fn streams(&self) -> &'static [Stream];

    /// Decode one inbound notification. `None` means the profile does not own
    /// that characteristic, which is not an error: a watch may expose more than
    /// the Spore subscribes to.
    fn decode(&self, char_uuid: &str, data: &[u8]) -> Result<Option<Signal>, String>;

    /// Is this firmware new enough for the intent? `None` when the version
    /// string cannot be read — a patched build gets the write, and the
    /// characteristic itself fails loudly if the service is missing.
    fn supports(&self, firmware: &str, intent: Intent) -> Option<bool>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streams_parse_as_a_set_and_reject_typos() {
        assert_eq!(
            Stream::parse("hr,steps").expect("streams"),
            vec![Stream::HeartRate, Stream::Steps]
        );
        // Duplicates collapse; order is the caller's.
        assert_eq!(
            Stream::parse("hr,hr").expect("streams"),
            vec![Stream::HeartRate]
        );
        assert_eq!(Stream::parse("all").expect("streams"), Stream::ALL.to_vec());
        let error = Stream::parse("hr,hartrate").expect_err("typo");
        assert!(error.contains("hartrate"), "{error}");
        assert!(Stream::parse("").is_err());
    }

    #[test]
    fn battery_is_device_state_not_a_signal() {
        assert_eq!(Stream::Battery.kind(), None);
        assert_eq!(Stream::HeartRate.kind(), Some("body.hr.v1"));
    }

    #[test]
    fn the_envelope_carries_the_identity_and_the_age() {
        let signal = Signal {
            stream: Stream::HeartRate,
            value: json!({"bpm": 61}),
            privacy_class: "health",
            retention_class: "raw",
            quality: 1.0,
        };
        let body = signal_body(
            "pt-1",
            "owner",
            "dev.fungos.pinetime",
            "0.1.0",
            &signal,
            1_000,
            7,
        );
        assert_eq!(body["source_device"], "pt-1");
        assert_eq!(body["kind"], "body.hr.v1");
        assert_eq!(body["wearer"], "owner");
        assert_eq!(body["value"]["bpm"], 61);
        assert_eq!(body["signal_id"], "pt-1:body.hr.v1:1000:7");
        assert_eq!(body["privacy_class"], "health");
    }

    #[test]
    fn identity_prefers_the_serial_and_never_invents_one() {
        assert_eq!(device_id(" SN-1 ", None), Some("SN-1".into()));
        assert_eq!(
            device_id("", Some("provisioned")),
            Some("provisioned".into())
        );
        assert_eq!(device_id("  ", Some("  ")), None);
        assert_eq!(device_id("", None), None);
    }
}
