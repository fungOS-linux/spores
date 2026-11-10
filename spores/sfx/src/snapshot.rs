//! The board snapshot. One JSON value, no callbacks: the Lua view is a pure
//! function of this document, and every fact in it is one the Spore can justify.

use serde_json::{json, Value};

use crate::mapping::{self, Mapping};
use crate::synth;

/// Twelve built-ins plus twelve samples fit the key row without inventing a
/// chord: `1`..`=` then `q`..`]`.
pub const KEYS: &[&str] = &[
    "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "-", "=", "q", "w", "e", "r", "t", "y", "u",
    "i", "o", "p", "[", "]",
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoomInfo {
    pub name: String,
    pub label: String,
    pub kind: String,
    pub live: bool,
    pub principal: Option<String>,
}

/// What the guest knows that the mapping does not. Filled by the guest before
/// each snapshot; the view never reaches for host state itself.
#[derive(Clone, Debug, Default)]
pub struct Status {
    pub scope: Option<String>,
    pub generation: Option<u64>,
    pub mapping_error: Option<String>,
    pub last_pad: Option<String>,
    pub last_kind: Option<String>,
    pub last_error: Option<String>,
    pub bind: Option<String>,
    pub selected: Option<String>,
    pub current_room: Option<String>,
    pub opened_room: Option<String>,
    pub audio_error: Option<String>,
    pub synth_error: Option<String>,
    pub rooms: Vec<RoomInfo>,
}

pub fn snapshot_json(mapping: &Mapping, status: &Status) -> Value {
    let issues: Vec<Value> = mapping
        .validate()
        .into_iter()
        .map(|issue| {
            json!({
                "level": issue.level.as_str(),
                "subject": issue.subject,
                "message": issue.message,
            })
        })
        .collect();

    let pads: Vec<Value> = synth::BUILTINS
        .iter()
        .map(|builtin| {
            json!({
                "id": builtin.id,
                "label": builtin.label,
                "category": builtin.category,
                "source": "builtin",
                "available": true,
                "reason": Value::Null,
                "duration_ms": synth::duration_ms(builtin.id, 48_000),
                "selected": status.selected.as_deref() == Some(builtin.id),
                "room": mapping.room,
            })
        })
        .chain(mapping.samples.iter().map(|sample| {
            let reason = sample_reason(mapping, sample);
            json!({
                "id": sample.pad,
                "label": sample.label(),
                "category": sample.category,
                "source": "sample",
                "available": reason.is_none(),
                "reason": reason,
                "duration_ms": Value::Null,
                "selected": status.selected.as_deref() == Some(sample.pad.as_str()),
                "room": mapping.room,
                "path": sample.path,
                "content_id": sample.content_id,
            })
        }))
        .enumerate()
        .map(|(index, mut pad)| {
            pad["key"] = json!(KEYS.get(index).copied());
            pad
        })
        .collect();

    let rules: Vec<Value> = mapping
        .rules
        .iter()
        .map(|rule| {
            let known = mapping.pad_known(&rule.pad);
            let label = mapping
                .sample_for(&rule.pad)
                .map(|sample| sample.label().to_string())
                .or_else(|| synth::find(&rule.pad).map(|builtin| builtin.label.to_string()))
                .unwrap_or_else(|| rule.pad.clone());
            json!({
                "kind": rule.kind,
                "pad": rule.pad,
                "label": label,
                "enabled": rule.enabled,
                "available": known,
                "gain": rule.gain,
                "room": rule.room,
            })
        })
        .collect();

    let enabled = mapping.rules.iter().filter(|rule| rule.enabled).count();
    let rooms: Vec<Value> = status
        .rooms
        .iter()
        .map(|room| {
            json!({
                "name": room.name,
                "label": room.label,
                "kind": room.kind,
                "live": room.live,
                "principal": room.principal,
                "current": status.current_room.as_deref() == Some(room.name.as_str()),
            })
        })
        .collect();
    let live_rooms = status.rooms.iter().filter(|room| room.live).count();

    json!({
        "title": "SFX",
        "counts": {
            "pads": pads.len(),
            "rules": rules.len(),
            "enabled": enabled,
            "live_rooms": live_rooms,
            "issues": issues.len(),
        },
        "mapping": {
            "version": mapping.version,
            "scope": status.scope,
            "generation": status.generation,
            "gain": mapping.gain,
            "room": mapping.room,
            "error": status.mapping_error,
        },
        "status": {
            "output": "48 kHz stereo",
            "opened_room": status.opened_room,
            "current_room": status.current_room,
            "last_pad": status.last_pad,
            "last_kind": status.last_kind,
            "last_error": status.last_error,
            "bind": status.bind,
            "selected": status.selected,
            "audio_error": status.audio_error,
            "synth_error": status.synth_error,
        },
        "rooms": rooms,
        "pads": pads,
        "rules": rules,
        "issues": issues,
    })
}

/// Why a sample pad cannot be used, or `None` when it is usable. The same rule
/// is applied at load time; the snapshot must not paint a pad playable that the
/// trigger path will refuse.
pub fn sample_reason(mapping: &Mapping, sample: &mapping::SamplePad) -> Option<String> {
    if !mapping::valid_pad_id(&sample.pad) {
        return Some("pad id is not a pad id".into());
    }
    if synth::find(&sample.pad).is_some() {
        return Some("pad id shadows a built-in".into());
    }
    if mapping
        .samples
        .iter()
        .filter(|candidate| candidate.pad == sample.pad)
        .count()
        > 1
    {
        return Some("pad id is defined twice".into());
    }
    if !mapping::valid_path(&sample.path) {
        return Some("path is not workspace-relative".into());
    }
    if !mapping::valid_content_id(&sample.content_id) {
        return Some("missing blake3 content id".into());
    }
    if !sample.gain.is_finite() || !(0.0..=mapping::MAX_GAIN).contains(&sample.gain) {
        return Some("gain is out of range".into());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::SamplePad;

    fn sample(pad: &str) -> SamplePad {
        SamplePad {
            pad: pad.into(),
            label: String::new(),
            path: format!("sounds/{pad}.mp3"),
            content_id: format!("blake3:{}", "a".repeat(64)),
            category: "sample".into(),
            gain: 1.0,
        }
    }

    #[test]
    fn the_default_snapshot_lists_every_builtin_and_rule() {
        let mapping = mapping::default_mapping();
        let value = snapshot_json(&mapping, &Status::default());
        let pads = value["pads"].as_array().unwrap();
        assert_eq!(pads.len(), synth::BUILTINS.len());
        assert_eq!(value["rules"].as_array().unwrap().len(), 6);
        assert_eq!(value["counts"]["enabled"], 6);
        assert_eq!(pads[0]["key"], "1");
        assert_eq!(pads[0]["available"], true);
    }

    #[test]
    fn a_sample_without_a_cid_is_visibly_unavailable() {
        let mut mapping = mapping::default_mapping();
        let mut pad = sample("intro");
        pad.content_id = String::new();
        mapping.samples.push(pad);
        let value = snapshot_json(&mapping, &Status::default());
        let pads = value["pads"].as_array().unwrap();
        let intro = pads.iter().find(|pad| pad["id"] == "intro").unwrap();
        assert_eq!(intro["available"], false);
        assert!(intro["reason"].as_str().unwrap().contains("content id"));
    }

    #[test]
    fn rooms_and_the_current_room_are_reported() {
        let mapping = mapping::default_mapping();
        let status = Status {
            current_room: Some("home-pi".into()),
            rooms: vec![RoomInfo {
                name: "home-pi".into(),
                label: "Home Pi".into(),
                kind: "remote".into(),
                live: true,
                principal: Some("ajmwagar".into()),
            }],
            ..Status::default()
        };
        let value = snapshot_json(&mapping, &status);
        assert_eq!(value["rooms"][0]["current"], true);
        assert_eq!(value["counts"]["live_rooms"], 1);
        assert_eq!(value["status"]["current_room"], "home-pi");
    }
}
