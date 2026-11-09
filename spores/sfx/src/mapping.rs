//! The event→sound mapping, stored as JSON under one `soil:appdata` key.
//!
//! The mapping is a Spore document, not host configuration. The host only
//! needs the *kinds* in it, which the snapshot exposes; it never needs to
//! understand a pad, a sample, or a gain.
//!
//! Unknown fields are rejected. A mapping with a typo that silently drops the
//! rule for a failed build is worse than one that fails to load: an alert
//! channel that plays the wrong sound is lying.

use serde::{Deserialize, Serialize};

use crate::synth;

pub const MAPPING_KEY: &str = "sfx.mapping@1";
pub const VERSION: u32 = 1;
pub const DEFAULT_GAIN: f32 = 0.85;
pub const MAX_GAIN: f32 = 2.0;
pub const MAX_RULES: usize = 64;
pub const MAX_SAMPLES: usize = 64;

fn default_version() -> u32 {
    VERSION
}

fn default_gain() -> f32 {
    1.0
}

fn default_enabled() -> bool {
    true
}

fn default_category() -> String {
    "sample".into()
}

/// One event kind bound to one pad.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// A `soil:unibus` message kind, e.g. `chat.message.v1`.
    pub kind: String,
    /// A built-in pad id or the id of a [`SamplePad`].
    pub pad: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Per-rule trim, multiplied by the mapping gain.
    #[serde(default = "default_gain")]
    pub gain: f32,
    /// Where this rule should be heard. `None` inherits the mapping room.
    #[serde(default)]
    pub room: Option<String>,
}

/// A sample pad backed by a workspace object.
///
/// `path` is the lookup and `content_id` is the identity. The host resolves the
/// path inside the granted workspace; the Spore verifies the bytes it reads
/// still hash to `content_id` before a sample is allowed to play. A substituted
/// file is refused, not played under the original name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SamplePad {
    pub pad: String,
    #[serde(default)]
    pub label: String,
    /// Canonical workspace-relative path, e.g. `sounds/coin.mp3`.
    pub path: String,
    /// `blake3:<64 hex>`; the whole-content identity the VFS reported.
    pub content_id: String,
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default = "default_gain")]
    pub gain: f32,
}

impl SamplePad {
    pub fn label(&self) -> &str {
        if self.label.is_empty() {
            &self.pad
        } else {
            &self.label
        }
    }
}

/// The whole mapping. `version` is explicit so a future shape can migrate
/// instead of being guessed at.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mapping {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default = "default_gain")]
    pub gain: f32,
    /// Default room for pads that do not override it. `None` is the host
    /// default; `Some("current")` follows the principal's room.
    #[serde(default)]
    pub room: Option<String>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub samples: Vec<SamplePad>,
}

impl Default for Mapping {
    fn default() -> Self {
        default_mapping()
    }
}

/// The mapping a fresh install has: the alerts the desktop already produces,
/// bound to built-in pads. Samples are opt-in, because a sample is user content
/// and a built-in is a guarantee that a sound exists on day one.
pub fn default_mapping() -> Mapping {
    let bindings = [
        ("email.delta.v1", "chime"),
        ("chat.message.v1", "pop"),
        ("notify.publish.v1", "ping"),
        ("call.incoming.v1", "alarm"),
        ("ci.failed.v1", "thud"),
        ("timer.elapsed.v1", "rise"),
    ];
    Mapping {
        version: VERSION,
        gain: DEFAULT_GAIN,
        room: Some("current".into()),
        rules: bindings
            .into_iter()
            .map(|(kind, pad)| Rule {
                kind: kind.into(),
                pad: pad.into(),
                enabled: true,
                gain: 1.0,
                room: None,
            })
            .collect(),
        samples: Vec::new(),
    }
}

/// Parse a stored mapping. A malformed document is an error; the caller keeps
/// the last good (or default) mapping and surfaces the reason.
pub fn parse(bytes: &[u8]) -> Result<Mapping, String> {
    let mapping: Mapping =
        serde_json::from_slice(bytes).map_err(|error| format!("mapping JSON: {error}"))?;
    if mapping.version != VERSION {
        return Err(format!(
            "mapping version {} is not supported (want {VERSION})",
            mapping.version
        ));
    }
    Ok(mapping)
}

pub fn to_bytes(mapping: &Mapping) -> Result<Vec<u8>, String> {
    serde_json::to_vec_pretty(mapping).map_err(|error| format!("mapping JSON: {error}"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Error,
    Warning,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Error => "error",
            Level::Warning => "warning",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    pub level: Level,
    pub subject: String,
    pub message: String,
}

impl Issue {
    fn error(subject: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: Level::Error,
            subject: subject.into(),
            message: message.into(),
        }
    }
}

impl Mapping {
    pub fn rule_for(&self, kind: &str) -> Option<&Rule> {
        self.rules.iter().find(|rule| rule.kind == kind)
    }

    pub fn sample_for(&self, pad: &str) -> Option<&SamplePad> {
        self.samples.iter().find(|sample| sample.pad == pad)
    }

    pub fn pad_known(&self, pad: &str) -> bool {
        synth::find(pad).is_some() || self.sample_for(pad).is_some()
    }

    /// The room one rule should use: rule override, then mapping default. The
    /// host resolves the returned name; `None` is its own default.
    pub fn room_for<'a>(&'a self, rule: Option<&'a Rule>) -> Option<&'a str> {
        rule.and_then(|rule| rule.room.as_deref())
            .or(self.room.as_deref())
    }

    /// The kinds the host should subscribe to. Disabled rules are omitted: a
    /// muted alert should not make the host do work it will throw away.
    pub fn subscribed_kinds(&self) -> Vec<String> {
        let mut kinds = Vec::new();
        for rule in &self.rules {
            if rule.enabled && !kinds.contains(&rule.kind) {
                kinds.push(rule.kind.clone());
            }
        }
        kinds
    }

    /// Bind a kind to a pad, replacing an existing binding and re-enabling it.
    pub fn bind(&mut self, kind: &str, pad: &str) -> Result<(), String> {
        if !valid_kind(kind) {
            return Err(format!("event kind {kind:?} is not a namespaced kind"));
        }
        if !valid_pad_id(pad) {
            return Err(format!("pad id {pad:?} is not a pad id"));
        }
        if !self.pad_known(pad) {
            return Err(format!("pad {pad:?} does not exist"));
        }
        if let Some(rule) = self.rules.iter_mut().find(|rule| rule.kind == kind) {
            rule.pad = pad.into();
            rule.enabled = true;
            return Ok(());
        }
        if self.rules.len() >= MAX_RULES {
            return Err(format!("the mapping is limited to {MAX_RULES} rules"));
        }
        self.rules.push(Rule {
            kind: kind.into(),
            pad: pad.into(),
            enabled: true,
            gain: 1.0,
            room: None,
        });
        Ok(())
    }

    pub fn remove_rule(&mut self, kind: &str) -> bool {
        let before = self.rules.len();
        self.rules.retain(|rule| rule.kind != kind);
        self.rules.len() != before
    }

    pub fn set_enabled(&mut self, kind: &str, enabled: bool) -> Result<(), String> {
        let rule = self
            .rules
            .iter_mut()
            .find(|rule| rule.kind == kind)
            .ok_or_else(|| format!("no rule for {kind:?}"))?;
        rule.enabled = enabled;
        Ok(())
    }

    /// Everything a reviewer (or the board) should know before trusting this
    /// mapping. Errors mean a pad is unusable; warnings mean a rule is.
    pub fn validate(&self) -> Vec<Issue> {
        let mut issues = Vec::new();
        if !self.gain.is_finite() || !(0.0..=MAX_GAIN).contains(&self.gain) {
            issues.push(Issue::error(
                "mapping.gain",
                format!("gain {} is outside 0..={MAX_GAIN}", self.gain),
            ));
        }
        if let Some(room) = &self.room {
            if !valid_room(room) {
                issues.push(Issue::error(
                    "mapping.room",
                    format!("room {room:?} is not a room name"),
                ));
            }
        }
        if self.rules.len() > MAX_RULES {
            issues.push(Issue::error(
                "mapping.rules",
                format!("{} rules exceeds the {MAX_RULES} limit", self.rules.len()),
            ));
        }
        if self.samples.len() > MAX_SAMPLES {
            issues.push(Issue::error(
                "mapping.samples",
                format!(
                    "{} samples exceeds the {MAX_SAMPLES} limit",
                    self.samples.len()
                ),
            ));
        }

        let mut kinds = std::collections::BTreeSet::new();
        for rule in &self.rules {
            if !valid_kind(&rule.kind) {
                issues.push(Issue::error(
                    format!("rule {}", rule.kind),
                    "event kind is not a namespaced kind",
                ));
            }
            if !kinds.insert(rule.kind.as_str()) {
                issues.push(Issue::error(
                    format!("rule {}", rule.kind),
                    "the same event kind is bound twice; only the first is used",
                ));
            }
            if !self.pad_known(&rule.pad) {
                issues.push(Issue::error(
                    format!("rule {}", rule.kind),
                    format!("pad {:?} does not exist", rule.pad),
                ));
            }
            if !rule.gain.is_finite() || !(0.0..=MAX_GAIN).contains(&rule.gain) {
                issues.push(Issue::error(
                    format!("rule {}", rule.kind),
                    format!("gain {} is outside 0..={MAX_GAIN}", rule.gain),
                ));
            }
            if let Some(room) = &rule.room {
                if !valid_room(room) {
                    issues.push(Issue::error(
                        format!("rule {}", rule.kind),
                        format!("room {room:?} is not a room name"),
                    ));
                }
            }
        }

        let mut pads = std::collections::BTreeSet::new();
        for sample in &self.samples {
            if !valid_pad_id(&sample.pad) {
                issues.push(Issue::error(
                    format!("sample {}", sample.pad),
                    "pad id is not a pad id",
                ));
            }
            if !pads.insert(sample.pad.as_str()) {
                issues.push(Issue::error(
                    format!("sample {}", sample.pad),
                    "the same pad id is defined twice; only the first is used",
                ));
            }
            if synth::find(&sample.pad).is_some() {
                issues.push(Issue::error(
                    format!("sample {}", sample.pad),
                    "pad id shadows a built-in pad",
                ));
            }
            if !valid_path(&sample.path) {
                issues.push(Issue::error(
                    format!("sample {}", sample.pad),
                    format!("path {:?} is not workspace-relative", sample.path),
                ));
            }
            if !valid_content_id(&sample.content_id) {
                issues.push(Issue::error(
                    format!("sample {}", sample.pad),
                    format!("content id {:?} is not blake3:<64 hex>", sample.content_id),
                ));
            }
            if !sample.gain.is_finite() || !(0.0..=MAX_GAIN).contains(&sample.gain) {
                issues.push(Issue::error(
                    format!("sample {}", sample.pad),
                    format!("gain {} is outside 0..={MAX_GAIN}", sample.gain),
                ));
            }
        }
        issues
    }
}

pub fn valid_kind(kind: &str) -> bool {
    !kind.is_empty()
        && kind.len() <= 128
        && kind.chars().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-' | '_' | ':' | '/')
        })
        && kind
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

pub fn valid_pad_id(pad: &str) -> bool {
    !pad.is_empty()
        && pad.len() <= 64
        && pad
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_'))
        && pad
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

/// A room name the host can resolve. `current` is a reserved sentinel; the rest
/// are host-owned ids such as `home-pi` or `lab`. The Spore never treats one as
/// an address.
pub fn valid_room(room: &str) -> bool {
    !room.is_empty()
        && room.len() <= 64
        && room
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.'))
        && room
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

/// A canonical VFS path: relative, forward slashes, no `.`/`..`, no drive or
/// backslash. The Spore names the object; the host owns the workspace root.
pub fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 1024
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// `blake3:<64 lowercase hex>`, the identity `soil:vfs` reports.
pub fn valid_content_id(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn the_default_mapping_is_valid_and_covers_the_desktop() {
        let mapping = default_mapping();
        assert!(mapping.validate().is_empty(), "{:?}", mapping.validate());
        assert_eq!(mapping.rules.len(), 6);
        assert!(mapping.rule_for("chat.message.v1").is_some());
        assert_eq!(mapping.subscribed_kinds().len(), 6);
    }

    #[test]
    fn a_round_trip_preserves_the_document() {
        let mut mapping = default_mapping();
        mapping.samples.push(sample("intro"));
        mapping.bind("build.failed.v1", "zap").unwrap();
        let bytes = to_bytes(&mapping).unwrap();
        assert_eq!(parse(&bytes).unwrap(), mapping);
    }

    #[test]
    fn an_unknown_field_is_refused_rather_than_dropped() {
        let json = br#"{"version":1,"gain":0.8,"rules":[],"samples":[],"enabeld":true}"#;
        assert!(parse(json).is_err());
    }

    #[test]
    fn binding_replaces_and_reenables_one_kind() {
        let mut mapping = default_mapping();
        mapping.set_enabled("chat.message.v1", false).unwrap();
        mapping.bind("chat.message.v1", "coin").unwrap();
        let rule = mapping.rule_for("chat.message.v1").unwrap();
        assert_eq!(rule.pad, "coin");
        assert!(rule.enabled);
        assert_eq!(
            mapping
                .rules
                .iter()
                .filter(|rule| rule.kind == "chat.message.v1")
                .count(),
            1
        );
    }

    #[test]
    fn a_binding_to_an_unknown_pad_is_refused() {
        let mut mapping = default_mapping();
        assert!(mapping.bind("chat.message.v1", "nope").is_err());
    }

    #[test]
    fn disabled_rules_are_not_subscribed() {
        let mut mapping = default_mapping();
        mapping.set_enabled("email.delta.v1", false).unwrap();
        assert!(!mapping
            .subscribed_kinds()
            .contains(&"email.delta.v1".to_string()));
    }

    #[test]
    fn a_sample_without_a_cid_is_an_error() {
        let mut mapping = default_mapping();
        let mut pad = sample("intro");
        pad.content_id = String::new();
        mapping.samples.push(pad);
        let issues = mapping.validate();
        assert!(issues.iter().any(|issue| issue.level == Level::Error));
        assert!(issues
            .iter()
            .any(|issue| issue.message.contains("content id")));
    }

    #[test]
    fn a_sample_may_not_shadow_a_builtin_or_escape_the_workspace() {
        let mut mapping = default_mapping();
        let mut shadow = sample("ping");
        shadow.pad = "ping".into();
        shadow.path = "../ping.mp3".into();
        mapping.samples.push(shadow);
        let issues = mapping.validate();
        assert!(issues.iter().any(|issue| issue.message.contains("shadows")));
        assert!(issues
            .iter()
            .any(|issue| issue.message.contains("workspace-relative")));
    }

    #[test]
    fn paths_and_kinds_are_namespaced() {
        assert!(valid_path("sounds/a.mp3"));
        assert!(!valid_path("/sounds/a.mp3"));
        assert!(!valid_path("sounds/../a.mp3"));
        assert!(!valid_path("sounds\\a.mp3"));
        assert!(valid_kind("email.delta.v1"));
        assert!(!valid_kind("Email Delta"));
        assert!(valid_content_id(&format!("blake3:{}", "a".repeat(64))));
        assert!(!valid_content_id(&format!("blake3:{}", "A".repeat(64))));
    }
}
