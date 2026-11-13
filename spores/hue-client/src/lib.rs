//! Philips Hue v1 LAN semantics shared by the CLI and UI Spores.
//!
//! The bridge key is not part of this crate's state when a Geode handle is
//! used: the host substitutes it into the `__CREDENTIAL__` path segment and
//! the guest only ever sees the handle. The URL construction and error
//! decoding here are the one implementation both `hue` and `lights-view` use.

use serde_json::{Value, json};

pub use cli_http::{Http, Method, Request, Response};

/// Largest response the Spore will accept from a bridge.
pub const MAX_BODY: u32 = 1024 * 1024;
/// Longest a bridge request may take before the host gives up.
pub const TIMEOUT_MS: u32 = 10_000;
/// The literal the host replaces with a leased credential path segment.
pub const CREDENTIAL_MARKER: &str = "__CREDENTIAL__";

/// A Hue bridge endpoint plus the credential it should use.
#[derive(Clone, Debug)]
pub struct Bridge {
    base: String,
    key: Option<String>,
    credential: Option<String>,
}

impl Bridge {
    pub fn with_key(base: impl Into<String>, key: impl Into<String>) -> Result<Self, String> {
        let base = normalize_base(base.into())?;
        Ok(Self {
            base,
            key: Some(key.into()),
            credential: None,
        })
    }

    pub fn with_credential(
        base: impl Into<String>,
        credential: impl Into<String>,
    ) -> Result<Self, String> {
        let base = normalize_base(base.into())?;
        Ok(Self {
            base,
            key: None,
            credential: Some(credential.into()),
        })
    }

    /// A bridge with no credential yet, used for the one-time pairing call.
    pub fn pairing(base: impl Into<String>) -> Result<Self, String> {
        Ok(Self {
            base: normalize_base(base.into())?,
            key: None,
            credential: None,
        })
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn credential(&self) -> Option<&str> {
        self.credential.as_deref()
    }

    /// The URL the bridge expects for an API path (`""` is the bridge root).
    pub fn url(&self, path: &str) -> String {
        match (&self.key, &self.credential) {
            (None, Some(_)) => format!("{}/api/{CREDENTIAL_MARKER}{path}", self.base),
            (Some(key), None) => format!("{}/api/{key}{path}", self.base),
            _ => format!("{}/api{path}", self.base),
        }
    }

    pub fn get(&self, http: &mut dyn Http, path: &str) -> Result<Value, String> {
        let response = http.fetch(Request {
            url: self.url(path),
            method: Method::Get,
            headers: vec![("accept".into(), "application/json".into())],
            body: None,
            credential: self.credential.clone(),
        })?;
        decode(response)
    }

    pub fn put(&self, http: &mut dyn Http, path: &str, body: Value) -> Result<Value, String> {
        self.write(http, Method::Put, path, body)
    }

    pub fn post(&self, http: &mut dyn Http, path: &str, body: Value) -> Result<Value, String> {
        self.write(http, Method::Post, path, body)
    }

    fn write(
        &self,
        http: &mut dyn Http,
        method: Method,
        path: &str,
        body: Value,
    ) -> Result<Value, String> {
        let bytes = serde_json::to_vec(&body).map_err(|error| error.to_string())?;
        let response = http.fetch(Request {
            url: self.url(path),
            method,
            headers: vec![
                ("accept".into(), "application/json".into()),
                ("content-type".into(), "application/json".into()),
            ],
            body: Some(bytes),
            credential: self.credential.clone(),
        })?;
        decode(response)
    }

    /// The full bridge state: config, lights, groups and scenes.
    ///
    /// Hue's `/groups` collection omits the built-in all-lights group `0`,
    /// even though `/groups/0` is addressable. Fetch that state too so the UI
    /// can show a real all-lights switch instead of inventing one from the
    /// per-light flags.
    pub fn snapshot(&self, http: &mut dyn Http) -> Result<Value, String> {
        let mut value = self.get(http, "")?;
        let has_zero = value
            .get("groups")
            .and_then(Value::as_object)
            .is_some_and(|groups| groups.contains_key("0"));
        if !has_zero {
            if let Ok(all) = self.get(http, "/groups/0") {
                match value.get_mut("groups").and_then(Value::as_object_mut) {
                    Some(groups) => {
                        groups.insert("0".to_string(), all);
                    }
                    None => {
                        value["groups"] = json!({ "0": all });
                    }
                }
            }
        }
        Ok(value)
    }
}

fn normalize_base(base: String) -> Result<String, String> {
    if !(base.starts_with("http://") || base.starts_with("https://")) {
        return Err("bridge must be an absolute http(s) URL".into());
    }
    Ok(base.trim_end_matches('/').to_owned())
}

/// Decode a bridge response, turning Hue's in-band error array into `Err`.
pub fn decode(response: Response) -> Result<Value, String> {
    if !(200..300).contains(&response.status) {
        return Err(format!("bridge: HTTP {}", response.status));
    }
    let value: Value = serde_json::from_slice(&response.body)
        .map_err(|error| format!("bridge: invalid JSON: {error}"))?;
    if let Some(items) = value.as_array() {
        for item in items {
            if let Some(error) = item.get("error") {
                let description = error
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error");
                return Err(format!("bridge: {description}"));
            }
        }
    }
    Ok(value)
}

/// Numeric-id-sorted entries, so `1, 2, 10` reads in the order a person sees.
pub fn sorted(value: &Value) -> Vec<(String, Value)> {
    let mut entries: Vec<(String, Value)> = value
        .as_object()
        .map(|map| {
            map.iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        })
        .unwrap_or_default();
    entries.sort_by_key(|(key, _)| key.parse::<u64>().unwrap_or(u64::MAX));
    entries
}

pub fn on_off(state: &Value) -> &'static str {
    match state.get("on").and_then(Value::as_bool) {
        Some(true) => "on",
        Some(false) => "off",
        None => "?",
    }
}

pub fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

/// Parse `#RRGGBB` (the leading `#` is optional).
pub fn parse_hex(value: &str) -> Result<(u8, u8, u8), String> {
    let hex = value.strip_prefix('#').unwrap_or(value);
    if hex.len() != 6 {
        return Err(format!("color: {value:?} is not #RRGGBB"));
    }
    let channel = |range: std::ops::Range<usize>| {
        u8::from_str_radix(&hex[range], 16).map_err(|_| format!("color: {value:?} is not #RRGGBB"))
    };
    Ok((channel(0..2)?, channel(2..4)?, channel(4..6)?))
}

/// Convert sRGB to the CIE xy pair the Hue bridge expects.
///
/// The matrix is the sRGB D65 one Hue's developer documentation publishes; the
/// values are rounded to four decimals because the bridge rejects longer ones.
pub fn rgb_to_xy(r: u8, g: u8, b: u8) -> (f64, f64) {
    fn gamma(channel: u8) -> f64 {
        let value = f64::from(channel) / 255.0;
        if value > 0.040_45 {
            ((value + 0.055) / 1.055).powf(2.4)
        } else {
            value / 12.92
        }
    }
    let r = gamma(r);
    let g = gamma(g);
    let b = gamma(b);
    let x = r * 0.664_511 + g * 0.154_324 + b * 0.162_028;
    let y = r * 0.283_881 + g * 0.668_433 + b * 0.047_685;
    let z = r * 0.000_088 + g * 0.072_310 + b * 0.986_039;
    let sum = x + y + z;
    if sum <= f64::EPSILON {
        return (0.0, 0.0);
    }
    (
        (x / sum * 10_000.0).round() / 10_000.0,
        (y / sum * 10_000.0).round() / 10_000.0,
    )
}

/// A group action body for the common UI verbs.
pub fn group_power(on: bool) -> Value {
    json!({ "on": on })
}

pub fn light_power(on: bool) -> Value {
    json!({ "on": on })
}

pub fn brightness(value: u8) -> Value {
    json!({ "bri": value, "on": true })
}

/// A color-temperature body. Hue takes mired, not Kelvin, and rejects values
/// outside `153..=500`.
pub fn color_temp(mired: u16) -> Value {
    json!({ "ct": mired.clamp(153, 500), "on": true })
}

/// A color body for an sRGB swatch. The bridge takes CIE xy.
pub fn color(r: u8, g: u8, b: u8) -> Value {
    let (x, y) = rgb_to_xy(r, g, b);
    json!({ "xy": [x, y], "on": true })
}

/// The inverse of [`rgb_to_xy`], for painting a light's current color.
///
/// Approximate on purpose: the dot next to a light name only has to read as
/// "warm white" or "teal", and the round trip through xy loses saturation
/// that the gamut of a real bulb would clamp anyway.
pub fn xy_to_rgb(x: f64, y: f64) -> (u8, u8, u8) {
    if y <= f64::EPSILON {
        return (255, 255, 255);
    }
    let xr = x / y;
    let yr = 1.0;
    let zr = (1.0 - x - y) / y;
    let r = xr * 3.2406 - yr * 1.5372 - zr * 0.4986;
    let g = -xr * 0.9689 + yr * 1.8758 + zr * 0.0415;
    let b = xr * 0.0557 - yr * 0.2040 + zr * 1.0570;
    let encode = |value: f64| {
        let value = value.clamp(0.0, 1.0);
        let value = if value > 0.003_130_8 {
            1.055 * value.powf(1.0 / 2.4) - 0.055
        } else {
            12.92 * value
        };
        (value * 255.0).round().clamp(0.0, 255.0) as u8
    };
    (encode(r), encode(g), encode(b))
}

/// A blackbody approximation of a mired value, for the same dot.
pub fn mired_to_rgb(mired: u16) -> (u8, u8, u8) {
    let kelvin = f64::from(mired.clamp(153, 500)).recip() * 1_000_000.0;
    let t = kelvin / 100.0;
    let red = if t <= 66.0 {
        255.0
    } else {
        329.698_727_446 * (t - 60.0).powf(-0.133_204_759_2)
    };
    let green = if t <= 66.0 {
        99.470_802_586_1 * t.ln() - 161.119_568_166_1
    } else {
        288.122_169_528_3 * (t - 60.0).powf(-0.075_514_849_2)
    };
    let blue = if t >= 66.0 {
        255.0
    } else if t <= 19.0 {
        0.0
    } else {
        138.517_731_223_1 * (t - 10.0).ln() - 305.044_792_730_7
    };
    let channel = |value: f64| value.clamp(0.0, 255.0) as u8;
    (channel(red), channel(green), channel(blue))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    struct MockHttp {
        requests: Vec<Request>,
        responses: VecDeque<Response>,
    }

    impl MockHttp {
        fn new(responses: impl IntoIterator<Item = Response>) -> Self {
            Self {
                requests: Vec::new(),
                responses: responses.into_iter().collect(),
            }
        }
    }

    impl Http for MockHttp {
        fn fetch(&mut self, request: Request) -> Result<Response, String> {
            self.requests.push(request);
            self.responses
                .pop_front()
                .ok_or_else(|| "no response".into())
        }
    }

    fn response(status: u16, body: &str) -> Response {
        Response {
            status,
            body: body.as_bytes().to_vec(),
        }
    }

    #[test]
    fn credential_mode_uses_the_marker_and_handle() {
        let bridge =
            Bridge::with_credential("http://hue.local/", "geode://home/hue/bridge").unwrap();
        assert_eq!(
            bridge.url("/groups/0"),
            "http://hue.local/api/__CREDENTIAL__/groups/0"
        );
        let mut http = MockHttp::new([response(200, "{}")]);
        bridge.get(&mut http, "/groups/0").unwrap();
        assert_eq!(
            http.requests[0].credential.as_deref(),
            Some("geode://home/hue/bridge")
        );
    }

    #[test]
    fn key_mode_puts_the_key_in_the_path() {
        let bridge = Bridge::with_key("http://bridge", "abc").unwrap();
        assert_eq!(bridge.url("/lights"), "http://bridge/api/abc/lights");
    }

    #[test]
    fn hue_error_array_becomes_a_failure() {
        let error = decode(response(
            200,
            r#"[{"error":{"description":"link button not pressed"}}]"#,
        ))
        .unwrap_err();
        assert!(error.contains("link button not pressed"));
    }

    #[test]
    fn snapshot_fetches_the_builtin_all_lights_group() {
        let bridge = Bridge::with_key("http://bridge", "abc").unwrap();
        let mut http = MockHttp::new([
            response(200, r#"{"lights":{},"groups":{},"scenes":{}}"#),
            response(
                200,
                r#"{"name":"All lights","lights":["1"],"action":{"on":true}}"#,
            ),
        ]);
        let value = bridge.snapshot(&mut http).unwrap();
        assert_eq!(http.requests[0].url, "http://bridge/api/abc");
        assert_eq!(http.requests[1].url, "http://bridge/api/abc/groups/0");
        assert_eq!(value["groups"]["0"]["name"], "All lights");
    }

    #[test]
    fn ids_sort_numerically() {
        let value = json!({"10": {}, "2": {}, "1": {}});
        let keys: Vec<_> = sorted(&value).into_iter().map(|(key, _)| key).collect();
        assert_eq!(keys, ["1", "2", "10"]);
    }

    #[test]
    fn rgb_conversion_matches_the_bridge_contract() {
        assert_eq!(rgb_to_xy(0, 0, 0), (0.0, 0.0));
        let (x, y) = rgb_to_xy(255, 0, 0);
        assert!((x - 0.7006).abs() < 0.001, "{x}");
        assert!((y - 0.2993).abs() < 0.001, "{y}");
    }

    #[test]
    fn color_bodies_carry_xy_and_switch_the_light_on() {
        let body = color(255, 0, 0);
        assert_eq!(body["on"], true);
        let x = body["xy"][0].as_f64().unwrap();
        let y = body["xy"][1].as_f64().unwrap();
        assert!((x - 0.7006).abs() < 0.001, "{x}");
        assert!((y - 0.2993).abs() < 0.001, "{y}");
    }

    #[test]
    fn color_temperature_is_clamped_to_the_bridge_range() {
        assert_eq!(color_temp(366)["ct"], 366);
        assert_eq!(color_temp(0)["ct"], 153);
        assert_eq!(color_temp(900)["ct"], 500);
    }

    #[test]
    fn painting_a_color_survives_the_round_trip() {
        let (r, g, b) = xy_to_rgb(0.7, 0.3);
        assert!(r > 180 && g < 80 && b < 80, "{r} {g} {b}");
        let (r, g, b) = xy_to_rgb(0.0, 0.0);
        assert_eq!((r, g, b), (255, 255, 255));
    }

    #[test]
    fn mired_reads_warm_at_one_end_and_cool_at_the_other() {
        // 153 mired is ~6500 K, which is the white end: the channels sit
        // together. 500 mired is ~2000 K and should be visibly amber.
        let (r, _, b) = mired_to_rgb(153);
        assert!((i32::from(r) - i32::from(b)).abs() < 20, "cool end {r} {b}");
        let (r, _, b) = mired_to_rgb(500);
        assert!(i32::from(r) - i32::from(b) > 100, "warm end {r} {b}");
    }
}
