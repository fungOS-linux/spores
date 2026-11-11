//! Hue Spore: a CLI over the Philips Hue bridge's local HTTP API.
//!
//! The Spore owns argument parsing and Hue semantics: which resource a verb
//! addresses, how `#RRGGBB` becomes CIE xy, and how the bridge's
//! `[{"error": …}]` replies become exit-worthy failures. The host owns the
//! connection: it decides which origins are reachable and caps every request
//! and response.
//!
//! The application key is a LAN-scoped bridge key, not a cloud token. Hue's v1
//! API puts it in the URL path, so it arrives as an argument; the host still
//! refuses any origin it was not told to allow.

#[cfg(target_arch = "wasm32")]
mod guest;

use std::fmt::Write as _;

use serde_json::{Value, json};

pub use cli_http::{Http, Method, Request, Response};

/// Largest response the Spore will accept from a bridge.
pub const MAX_BODY: u32 = 1024 * 1024;
/// Longest a bridge request may take before the host gives up.
pub const TIMEOUT_MS: u32 = 10_000;

pub fn run(args: &[String], http: &mut dyn Http) -> Result<String, String> {
    Command::parse(args)?.execute(http)
}

struct Command {
    bridge: String,
    key: String,
    credential: Option<String>,
    device: String,
    json: bool,
    target: Target,
    action: Action,
}

/// Which Hue resource a verb addresses. Group `0` is the bridge's built-in
/// "all lights" group, so it is the default.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Target {
    Group(String),
    Light(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Action {
    Status,
    Lights,
    Groups,
    Scenes,
    On,
    Off,
    Toggle,
    Brightness(u8),
    Color(u8, u8, u8),
    Scene(String),
    Pair,
    Help,
}

impl Command {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut bridge = None;
        let mut key = None;
        let mut credential = None;
        let mut device = None;
        let mut json = false;
        let mut group = None;
        let mut light = None;
        let mut words = Vec::new();

        let mut iter = args.iter();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--bridge" => bridge = iter.next().cloned(),
                "--key" => key = iter.next().cloned(),
                "--credential" => credential = iter.next().cloned(),
                "--device" => device = iter.next().cloned(),
                "--group" => group = iter.next().cloned(),
                "--light" => light = iter.next().cloned(),
                "--json" => json = true,
                "--help" | "-h" => {
                    return Ok(Self {
                        bridge: String::new(),
                        key: String::new(),
                        credential: None,
                        device: "spores#hue".into(),
                        json,
                        target: Target::Group("0".into()),
                        action: Action::Help,
                    });
                }
                _ => words.push(arg.clone()),
            }
        }

        let Some(verb) = words.first().map(String::as_str) else {
            return Err("usage: hue --bridge URL (--key KEY | --credential HANDLE) <status|on|off|toggle|bri N|color #RRGGBB|scene NAME|lights|groups|scenes|pair>".into());
        };
        let operand = words.get(1).cloned();
        if words.len() > 2 {
            return Err(format!("{verb}: too many arguments"));
        }

        let action = match verb {
            "status" => Action::Status,
            "lights" => Action::Lights,
            "groups" => Action::Groups,
            "scenes" => Action::Scenes,
            "on" => Action::On,
            "off" => Action::Off,
            "toggle" => Action::Toggle,
            "pair" => Action::Pair,
            "bri" | "brightness" => {
                let value = operand.clone().ok_or("bri: expected 0-254")?;
                let value: u8 = value
                    .parse()
                    .map_err(|_| format!("bri: {value:?} is not 0-254"))?;
                Action::Brightness(value)
            }
            "color" | "colour" => {
                let value = operand.clone().ok_or("color: expected #RRGGBB")?;
                let (r, g, b) = parse_hex(&value)?;
                Action::Color(r, g, b)
            }
            "scene" => Action::Scene(operand.clone().ok_or("scene: expected a scene name")?),
            other => return Err(format!("unknown command: {other}")),
        };

        let bridge =
            bridge.ok_or("--bridge is required, for example --bridge http://192.168.1.10")?;
        if !(bridge.starts_with("http://") || bridge.starts_with("https://")) {
            return Err("bridge must be an absolute http(s) URL".into());
        }
        let bridge = bridge.trim_end_matches('/').to_owned();

        if action == Action::Pair {
            if key.is_some() || credential.is_some() {
                return Err("pair does not take --key or --credential".into());
            }
            return Ok(Self {
                bridge,
                key: String::new(),
                credential: None,
                device: device.or(operand).unwrap_or_else(|| "spores#hue".into()),
                json,
                target: Target::Group("0".into()),
                action,
            });
        }

        let (key, credential) = match (key, credential) {
            (Some(key), None) if !key.is_empty() => (key, None),
            (None, Some(credential)) if !credential.is_empty() => (String::new(), Some(credential)),
            (Some(_), Some(_)) => {
                return Err("--key and --credential are mutually exclusive".into());
            }
            _ => {
                return Err(
                    "--key or --credential is required (the bridge application key)".into(),
                );
            }
        };
        let target = match (light, group) {
            (Some(_), Some(_)) => return Err("--light and --group are mutually exclusive".into()),
            (Some(light), None) => Target::Light(light),
            (None, group) => Target::Group(group.unwrap_or_else(|| "0".into())),
        };

        Ok(Self {
            bridge,
            key,
            credential,
            device: device.unwrap_or_else(|| "spores#hue".into()),
            json,
            target,
            action,
        })
    }

    fn execute(&self, http: &mut dyn Http) -> Result<String, String> {
        match &self.action {
            Action::Help => Ok(HELP.to_string()),
            Action::Status => self.status(http),
            Action::Lights => self.lights(http),
            Action::Groups => self.groups(http),
            Action::Scenes => self.scenes(http),
            Action::On => self.set_power(http, true),
            Action::Off => self.set_power(http, false),
            Action::Toggle => self.toggle(http),
            Action::Brightness(value) => self.set_brightness(http, *value),
            Action::Color(r, g, b) => self.set_color(http, *r, *g, *b),
            Action::Scene(name) => self.recall_scene(http, name),
            Action::Pair => self.pair(http),
        }
    }

    fn url(&self, path: &str) -> String {
        if self.credential.is_some() {
            format!("{}/api/__CREDENTIAL__{path}", self.bridge)
        } else {
            format!("{}/api/{}{path}", self.bridge, self.key)
        }
    }

    fn get(&self, http: &mut dyn Http, path: &str) -> Result<Value, String> {
        let response = http.fetch(Request {
            url: self.url(path),
            method: Method::Get,
            headers: vec![("accept".into(), "application/json".into())],
            body: None,
            credential: self.credential.clone(),
        })?;
        decode(response)
    }

    fn put(&self, http: &mut dyn Http, path: &str, body: Value) -> Result<Value, String> {
        let bytes = serde_json::to_vec(&body).map_err(|error| error.to_string())?;
        let response = http.fetch(Request {
            url: self.url(path),
            method: Method::Put,
            headers: vec![
                ("accept".into(), "application/json".into()),
                ("content-type".into(), "application/json".into()),
            ],
            body: Some(bytes),
            credential: self.credential.clone(),
        })?;
        decode(response)
    }

    fn pair(&self, http: &mut dyn Http) -> Result<String, String> {
        let body = serde_json::to_vec(&json!({ "devicetype": self.device }))
            .map_err(|error| error.to_string())?;
        let response = http.fetch(Request {
            url: format!("{}/api", self.bridge),
            method: Method::Post,
            headers: vec![("content-type".into(), "application/json".into())],
            body: Some(body),
            credential: None,
        })?;
        let value = decode(response)?;
        if self.json {
            return Ok(pretty(&value));
        }
        let key = value
            .as_array()
            .and_then(|items| items.first())
            .and_then(|item| item.get("success"))
            .and_then(|success| success.get("username"))
            .and_then(Value::as_str)
            .ok_or("bridge did not return an application key")?;
        Ok(format!(
            "paired {}\napplication key: {key}\nstore it as geode://home/hue/bridge",
            self.device
        ))
    }

    fn status(&self, http: &mut dyn Http) -> Result<String, String> {
        let config = self.get(http, "")?;
        let all = self.get(http, "/groups/0")?;
        if self.json {
            return Ok(pretty(&json!({ "bridge": config, "all": all })));
        }
        let name = config
            .get("config")
            .and_then(|config| config.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("hue bridge");
        let lights = config
            .get("lights")
            .and_then(Value::as_object)
            .map_or(0, serde_json::Map::len);
        let groups = config
            .get("groups")
            .and_then(Value::as_object)
            .map_or(0, serde_json::Map::len);
        let action = all.get("action").cloned().unwrap_or(Value::Null);
        Ok(format!(
            "{name}\nlights: {lights}\ngroups: {groups}\nall: {} bri: {}",
            on_off(&action),
            action.get("bri").and_then(Value::as_u64).unwrap_or(0),
        ))
    }

    fn lights(&self, http: &mut dyn Http) -> Result<String, String> {
        let lights = self.get(http, "/lights")?;
        if self.json {
            return Ok(pretty(&lights));
        }
        let mut out = String::new();
        for (id, light) in sorted(&lights) {
            let name = light.get("name").and_then(Value::as_str).unwrap_or("?");
            let state = light.get("state").cloned().unwrap_or(Value::Null);
            let _ = writeln!(
                out,
                "{id}\t{name}\t{}\tbri {}",
                on_off(&state),
                state.get("bri").and_then(Value::as_u64).unwrap_or(0),
            );
        }
        Ok(out.trim_end().to_string())
    }

    fn groups(&self, http: &mut dyn Http) -> Result<String, String> {
        let groups = self.get(http, "/groups")?;
        if self.json {
            return Ok(pretty(&groups));
        }
        let mut out = String::new();
        for (id, group) in sorted(&groups) {
            let name = group.get("name").and_then(Value::as_str).unwrap_or("?");
            let action = group.get("action").cloned().unwrap_or(Value::Null);
            let count = group
                .get("lights")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            let _ = writeln!(
                out,
                "{id}\t{name}\t{}\tbri {}\t{count} lights",
                on_off(&action),
                action.get("bri").and_then(Value::as_u64).unwrap_or(0),
            );
        }
        Ok(out.trim_end().to_string())
    }

    fn scenes(&self, http: &mut dyn Http) -> Result<String, String> {
        let scenes = self.get(http, "/scenes")?;
        if self.json {
            return Ok(pretty(&scenes));
        }
        let mut out = String::new();
        for (id, scene) in sorted(&scenes) {
            let name = scene.get("name").and_then(Value::as_str).unwrap_or("?");
            let group = scene.get("group").and_then(Value::as_str).unwrap_or("-");
            let _ = writeln!(out, "{id}\t{name}\tgroup {group}");
        }
        Ok(out.trim_end().to_string())
    }

    fn action_path(&self) -> String {
        match &self.target {
            Target::Group(id) => format!("/groups/{id}/action"),
            Target::Light(id) => format!("/lights/{id}/state"),
        }
    }

    fn state_path(&self) -> String {
        match &self.target {
            Target::Group(id) => format!("/groups/{id}"),
            Target::Light(id) => format!("/lights/{id}"),
        }
    }

    fn set_power(&self, http: &mut dyn Http, on: bool) -> Result<String, String> {
        let response = self.put(http, &self.action_path(), json!({ "on": on }))?;
        if self.json {
            return Ok(pretty(&response));
        }
        Ok(format!(
            "{}: {}",
            self.label(),
            if on { "on" } else { "off" }
        ))
    }

    fn toggle(&self, http: &mut dyn Http) -> Result<String, String> {
        let state = self.get(http, &self.state_path())?;
        let key = match self.target {
            Target::Group(_) => "action",
            Target::Light(_) => "state",
        };
        let on = state
            .get(key)
            .and_then(|state| state.get("on"))
            .and_then(Value::as_bool)
            .ok_or_else(|| format!("{}: bridge did not report a power state", self.label()))?;
        self.set_power(http, !on)
    }

    fn set_brightness(&self, http: &mut dyn Http, value: u8) -> Result<String, String> {
        if value > 254 {
            return Err(format!("bri: {value} is outside 0-254"));
        }
        let response = self.put(http, &self.action_path(), json!({ "bri": value }))?;
        if self.json {
            return Ok(pretty(&response));
        }
        Ok(format!("{}: brightness {value}", self.label()))
    }

    fn set_color(&self, http: &mut dyn Http, r: u8, g: u8, b: u8) -> Result<String, String> {
        let (x, y) = rgb_to_xy(r, g, b);
        let response = self.put(http, &self.action_path(), json!({ "xy": [x, y] }))?;
        if self.json {
            return Ok(pretty(&response));
        }
        Ok(format!(
            "{}: #{r:02x}{g:02x}{b:02x} -> xy ({x}, {y})",
            self.label()
        ))
    }

    fn recall_scene(&self, http: &mut dyn Http, name: &str) -> Result<String, String> {
        let scenes = self.get(http, "/scenes")?;
        let wanted = name.to_ascii_lowercase();
        let found = sorted(&scenes).into_iter().find(|(_, scene)| {
            scene
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|candidate| candidate.to_ascii_lowercase() == wanted)
        });
        let Some((id, scene)) = found else {
            return Err(format!("scene: no scene named {name:?}"));
        };
        let response = self.put(http, &self.action_path(), json!({ "scene": id }))?;
        if self.json {
            return Ok(pretty(&response));
        }
        let label = scene.get("name").and_then(Value::as_str).unwrap_or(name);
        Ok(format!("{}: scene {label}", self.label()))
    }

    fn label(&self) -> String {
        match &self.target {
            Target::Group(id) => format!("group {id}"),
            Target::Light(id) => format!("light {id}"),
        }
    }
}

const HELP: &str = "\
usage: hue --bridge URL (--key KEY | --credential HANDLE) [--group ID | --light ID] [--json] COMMAND
       hue --bridge URL pair [DEVICE]

commands:
  pair [DEVICE]       press the bridge link button, then mint an application key
  status              bridge name, light/group counts, all-lights state
  lights              list lights
  groups              list groups
  scenes              list scenes
  on | off            switch the target on or off
  toggle              invert the target's current power state
  bri N               set brightness 0-254
  color #RRGGBB       set colour as CIE xy
  scene NAME          recall a scene by name (case-insensitive)

examples:
  hue --bridge http://192.168.1.10 pair
  hue --bridge http://192.168.1.10 --credential geode://home/hue/bridge on
  hue --bridge http://192.168.1.10 --key $HUE_KEY --light 3 color #ff8800
  hue --bridge http://192.168.1.10 --key $HUE_KEY --group 1 scene Relax";

fn parse_hex(value: &str) -> Result<(u8, u8, u8), String> {
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
    (round4(x / sum), round4(y / sum))
}

fn round4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

fn decode(response: Response) -> Result<Value, String> {
    if !(200..300).contains(&response.status) {
        return Err(format!("bridge: HTTP {}", response.status));
    }
    let value: Value = serde_json::from_slice(&response.body)
        .map_err(|error| format!("bridge: invalid JSON: {error}"))?;
    // Hue reports application errors inside a 200 response.
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

fn sorted(value: &Value) -> Vec<(String, Value)> {
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

fn on_off(state: &Value) -> &'static str {
    match state.get("on").and_then(Value::as_bool) {
        Some(true) => "on",
        Some(false) => "off",
        None => "?",
    }
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
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
        fn new(responses: impl IntoIterator<Item = Value>) -> Self {
            Self {
                requests: Vec::new(),
                responses: responses
                    .into_iter()
                    .map(|value| Response {
                        status: 200,
                        body: serde_json::to_vec(&value).unwrap(),
                    })
                    .collect(),
            }
        }
    }

    impl Http for MockHttp {
        fn fetch(&mut self, request: Request) -> Result<Response, String> {
            self.requests.push(request);
            self.responses
                .pop_front()
                .ok_or_else(|| "no response".to_string())
        }
    }

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    fn base(line: &str) -> Vec<String> {
        format!("--bridge http://bridge.local --key testkey {line}")
            .split_whitespace()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn turns_a_group_on_with_a_put() {
        let mut http = MockHttp::new([json!([{ "success": { "/groups/0/action/on": true } }])]);
        let output = run(&base("on"), &mut http).unwrap();
        assert_eq!(output, "group 0: on");
        let request = &http.requests[0];
        assert_eq!(request.method, Method::Put);
        assert_eq!(
            request.url,
            "http://bridge.local/api/testkey/groups/0/action"
        );
        assert_eq!(request.body.as_deref(), Some(br#"{"on":true}"#.as_slice()));
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| name == "content-type" && value == "application/json")
        );
    }

    #[test]
    fn a_light_targets_the_light_state_endpoint() {
        let mut http = MockHttp::new([json!([{ "success": {} }])]);
        let mut argv = base("off");
        argv.extend(args("--light 3"));
        run(&argv, &mut http).unwrap();
        assert_eq!(
            http.requests[0].url,
            "http://bridge.local/api/testkey/lights/3/state"
        );
        assert_eq!(
            http.requests[0].body.as_deref(),
            Some(br#"{"on":false}"#.as_slice())
        );
    }

    #[test]
    fn toggle_reads_the_state_then_writes_the_inverse() {
        let mut http = MockHttp::new([
            json!({ "action": { "on": false } }),
            json!([{ "success": {} }]),
        ]);
        let output = run(&base("toggle"), &mut http).unwrap();
        assert_eq!(output, "group 0: on");
        assert_eq!(http.requests.len(), 2);
        assert_eq!(http.requests[1].method, Method::Put);
        assert_eq!(
            http.requests[1].body.as_deref(),
            Some(br#"{"on":true}"#.as_slice())
        );
    }

    #[test]
    fn bridge_errors_become_failures() {
        let mut http = MockHttp::new([json!([{
            "error": { "type": 1, "description": "unauthorized user" }
        }])]);
        let error = run(&base("on"), &mut http).unwrap_err();
        assert_eq!(error, "bridge: unauthorized user");
    }

    #[test]
    fn scene_names_are_case_insensitive() {
        let mut http = MockHttp::new([
            json!({ "1": { "name": "Relax", "group": "1" } }),
            json!([{ "success": {} }]),
        ]);
        let mut argv = base("scene relax");
        argv.push("--group".into());
        argv.push("1".into());
        let output = run(&argv, &mut http).unwrap();
        assert_eq!(output, "group 1: scene Relax");
        assert_eq!(
            http.requests[1].body.as_deref(),
            Some(br#"{"scene":"1"}"#.as_slice())
        );
    }

    #[test]
    fn unknown_scene_is_reported() {
        let mut http = MockHttp::new([json!({ "1": { "name": "Relax" } })]);
        let error = run(&base("scene Party"), &mut http).unwrap_err();
        assert!(error.contains("no scene named"), "{error}");
    }

    #[test]
    fn brightness_is_bounded() {
        let mut http = MockHttp::new([json!([{ "success": {} }])]);
        assert!(run(&base("bri 254"), &mut http).is_ok());
        assert!(run(&base("bri 255"), &mut http).is_err());
    }

    #[test]
    fn colour_is_converted_to_xy() {
        let mut http = MockHttp::new([json!([{ "success": {} }])]);
        run(&base("color #ff0000"), &mut http).unwrap();
        let body: Value =
            serde_json::from_slice(http.requests[0].body.as_deref().unwrap()).unwrap();
        let xy = body.get("xy").unwrap().as_array().unwrap();
        assert!(xy[0].as_f64().unwrap() > 0.6, "{body}");
        assert!(xy[1].as_f64().unwrap() < 0.4, "{body}");
    }

    #[test]
    fn red_lands_in_the_red_corner_and_black_is_zero() {
        let (x, y) = rgb_to_xy(255, 0, 0);
        assert!(x > 0.6 && y < 0.4, "{x},{y}");
        assert_eq!(rgb_to_xy(0, 0, 0), (0.0, 0.0));
    }

    #[test]
    fn lights_are_listed_in_numeric_order() {
        let mut http = MockHttp::new([json!({
            "10": { "name": "ten", "state": { "on": true, "bri": 10 } },
            "2": { "name": "two", "state": { "on": false, "bri": 2 } }
        })]);
        let output = run(&base("lights"), &mut http).unwrap();
        let lines: Vec<&str> = output.lines().collect();
        assert!(lines[0].starts_with("2\t"), "{output}");
        assert!(lines[1].starts_with("10\t"), "{output}");
    }

    #[test]
    fn missing_configuration_is_explained() {
        let error = run(&args("status"), &mut MockHttp::new([])).unwrap_err();
        assert!(error.contains("--bridge"), "{error}");
        let error = run(&args("--bridge http://b status"), &mut MockHttp::new([])).unwrap_err();
        assert!(error.contains("--key"), "{error}");
    }

    #[test]
    fn pair_posts_the_device_type_and_returns_the_key() {
        let mut http = MockHttp::new([json!([{
            "success": { "username": "paired-key" }
        }])]);
        let output = run(
            &args("--bridge http://bridge.local pair spores#hue"),
            &mut http,
        )
        .unwrap();
        assert!(output.contains("paired-key"), "{output}");
        let request = &http.requests[0];
        assert_eq!(request.method, Method::Post);
        assert_eq!(request.url, "http://bridge.local/api");
        assert_eq!(
            request.body.as_deref(),
            Some(br#"{"devicetype":"spores#hue"}"#.as_slice())
        );
        assert_eq!(request.credential, None);
    }

    #[test]
    fn a_credential_handle_replaces_the_key_in_the_path() {
        let mut http = MockHttp::new([
            json!({ "config": { "name": "bridge" }, "lights": {}, "groups": {} }),
            json!({ "action": { "on": false } }),
        ]);
        let output = run(
            &args("--bridge http://bridge.local --credential geode://home/hue/bridge status"),
            &mut http,
        )
        .unwrap();
        assert!(output.contains("bridge"), "{output}");
        assert_eq!(
            http.requests[0].url,
            "http://bridge.local/api/__CREDENTIAL__"
        );
        assert_eq!(
            http.requests[0].credential.as_deref(),
            Some("geode://home/hue/bridge")
        );
    }

    #[test]
    fn key_and_credential_are_mutually_exclusive() {
        let error = run(
            &args("--bridge http://bridge.local --key k --credential geode://home/hue/bridge on"),
            &mut MockHttp::new([]),
        )
        .unwrap_err();
        assert!(error.contains("mutually exclusive"), "{error}");
    }
}
