//! Spotify Spore: a remote for RASpotify and any other Spotify Connect device.
//!
//! The Spore owns Connect semantics: which device a verb addresses, how a URI
//! or an `open.spotify.com` link becomes a Spotify URI, and the command
//! vocabulary an agent can drive. The host owns the OAuth bearer token: the
//! guest asks for the `default` credential handle and never sees the bytes.
//!
//! RASpotify registers itself as a Connect device. `--device raspotify` finds
//! it by name and transfers playback to the Pi before issuing the command, so a
//! sleeping speaker wakes up without a second tool.

#[cfg(target_arch = "wasm32")]
mod guest;

use std::fmt::Write as _;

use serde_json::{Value, json};

pub use cli_http::{Http, Method, Request, Response};

/// The only origin this Spore ever names.
pub const API: &str = "https://api.spotify.com";
pub const MAX_BODY: u32 = 1024 * 1024;
pub const TIMEOUT_MS: u32 = 10_000;

pub fn run(args: &[String], http: &mut dyn Http) -> Result<String, String> {
    Command::parse(args)?.execute(http)
}

struct Command {
    device: Option<String>,
    json: bool,
    action: Action,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Action {
    Status,
    Devices,
    Play(Option<String>),
    Pause,
    Next,
    Previous,
    Volume(u8),
    Transfer(String),
    Shuffle(bool),
    Repeat(Repeat),
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Repeat {
    Off,
    Track,
    Context,
}

impl Repeat {
    fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Track => "track",
            Self::Context => "context",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Device {
    id: String,
    name: String,
    active: bool,
    volume: Option<u64>,
}

impl Command {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut device = None;
        let mut json = false;
        let mut words = Vec::new();

        let mut iter = args.iter();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--device" | "-d" => device = iter.next().cloned(),
                "--json" => json = true,
                "--help" | "-h" => {
                    return Ok(Self {
                        device: None,
                        json,
                        action: Action::Help,
                    });
                }
                _ => words.push(arg.clone()),
            }
        }

        let Some(verb) = words.first().map(String::as_str) else {
            return Err(USAGE.into());
        };
        let operand = words.get(1).cloned();
        if words.len() > 2 {
            return Err(format!("{verb}: too many arguments"));
        }

        let action = match verb {
            "status" | "now" => Action::Status,
            "devices" => Action::Devices,
            "play" | "resume" => Action::Play(operand),
            "pause" => Action::Pause,
            "next" => Action::Next,
            "prev" | "previous" => Action::Previous,
            "volume" | "vol" => {
                let value = operand.ok_or("volume: expected 0-100")?;
                let value: u8 = value
                    .parse()
                    .map_err(|_| format!("volume: {value:?} is not 0-100"))?;
                if value > 100 {
                    return Err(format!("volume: {value} is outside 0-100"));
                }
                Action::Volume(value)
            }
            "transfer" => Action::Transfer(operand.ok_or("transfer: expected a device name")?),
            "shuffle" => match operand.as_deref() {
                Some("on" | "true" | "1") => Action::Shuffle(true),
                Some("off" | "false" | "0") => Action::Shuffle(false),
                _ => return Err("shuffle: expected on or off".into()),
            },
            "repeat" => match operand.as_deref() {
                Some("off") => Action::Repeat(Repeat::Off),
                Some("track") => Action::Repeat(Repeat::Track),
                Some("context") => Action::Repeat(Repeat::Context),
                _ => return Err("repeat: expected off, track or context".into()),
            },
            other => return Err(format!("unknown command: {other}")),
        };

        Ok(Self {
            device,
            json,
            action,
        })
    }

    fn execute(&self, http: &mut dyn Http) -> Result<String, String> {
        match &self.action {
            Action::Help => Ok(USAGE.to_string()),
            Action::Status => self.status(http),
            Action::Devices => self.devices_text(http),
            Action::Play(uri) => self.play(http, uri.as_deref()),
            Action::Pause => self.simple(http, Method::Put, "/v1/me/player/pause", "paused"),
            Action::Next => self.simple(http, Method::Post, "/v1/me/player/next", "next"),
            Action::Previous => {
                self.simple(http, Method::Post, "/v1/me/player/previous", "previous")
            }
            Action::Volume(value) => self.volume(http, *value),
            Action::Transfer(name) => self.transfer(http, name),
            Action::Shuffle(on) => self.shuffle(http, *on),
            Action::Repeat(mode) => self.repeat(http, *mode),
        }
    }

    fn get(&self, http: &mut dyn Http, path: &str) -> Result<Value, String> {
        self.send(http, Method::Get, path, None)
    }

    fn send(
        &self,
        http: &mut dyn Http,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, String> {
        let bytes = body
            .map(|body| serde_json::to_vec(&body).map_err(|error| error.to_string()))
            .transpose()?;
        let mut headers = vec![("accept".to_string(), "application/json".to_string())];
        if bytes.is_some() {
            headers.push(("content-type".to_string(), "application/json".to_string()));
        }
        let response = http.fetch(Request {
            url: format!("{API}{path}"),
            method,
            headers,
            body: bytes,
            credential: Some("default".into()),
        })?;
        decode(response)
    }

    fn devices(&self, http: &mut dyn Http) -> Result<Vec<Device>, String> {
        let value = self.get(http, "/v1/me/player/devices")?;
        let Some(array) = value.get("devices").and_then(Value::as_array) else {
            return Err("spotify: devices reply has no device list".into());
        };
        Ok(array
            .iter()
            .filter_map(|device| {
                Some(Device {
                    id: device.get("id")?.as_str()?.to_owned(),
                    name: device.get("name")?.as_str()?.to_owned(),
                    active: device
                        .get("is_active")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    volume: device.get("volume_percent").and_then(Value::as_u64),
                })
            })
            .collect())
    }

    /// Resolve `--device` to a concrete device, transferring playback to it
    /// when it is idle. Without `--device` the Web API acts on whatever is
    /// already active, which is the least surprising default.
    fn target(&self, http: &mut dyn Http) -> Result<Option<Device>, String> {
        let Some(wanted) = &self.device else {
            return Ok(None);
        };
        let device = self.pick(http, Some(wanted))?;
        if !device.active {
            self.transfer_id(http, &device.id, false)?;
        }
        Ok(Some(device))
    }

    fn pick(&self, http: &mut dyn Http, wanted: Option<&str>) -> Result<Device, String> {
        let devices = self.devices(http)?;
        let Some(wanted) = wanted else {
            return devices
                .iter()
                .find(|device| device.active)
                .cloned()
                .ok_or_else(|| "spotify: no active device".to_string());
        };
        let needle = wanted.to_ascii_lowercase();
        let matches: Vec<Device> = devices
            .into_iter()
            .filter(|device| device.name.to_ascii_lowercase().contains(&needle))
            .collect();
        match matches.len() {
            0 => Err(format!(
                "spotify: no device matches {wanted:?}; try `spotify devices`"
            )),
            1 => Ok(matches.into_iter().next().expect("one match")),
            _ => {
                let names = matches
                    .iter()
                    .map(|device| device.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                Err(format!("{wanted:?} matches more than one device: {names}"))
            }
        }
    }

    fn transfer_id(&self, http: &mut dyn Http, id: &str, play: bool) -> Result<(), String> {
        self.send(
            http,
            Method::Put,
            "/v1/me/player",
            Some(json!({ "device_ids": [id], "play": play })),
        )?;
        Ok(())
    }

    fn play(&self, http: &mut dyn Http, uri: Option<&str>) -> Result<String, String> {
        let device = self.target(http)?;
        let path = with_device("/v1/me/player/play", device.as_ref().map(|d| d.id.as_str()));
        let body = match uri {
            Some(uri) => {
                let uri = normalize_uri(uri)?;
                if uri.starts_with("spotify:track:") || uri.starts_with("spotify:episode:") {
                    Some(json!({ "uris": [uri] }))
                } else {
                    Some(json!({ "context_uri": uri }))
                }
            }
            None => None,
        };
        self.send(http, Method::Put, &path, body.clone())?;
        Ok(match body {
            Some(body) => {
                let target = body
                    .get("uris")
                    .and_then(|uris| uris.get(0))
                    .or_else(|| body.get("context_uri"))
                    .and_then(Value::as_str)
                    .unwrap_or("playback");
                format!("playing {target}")
            }
            None => "playing".into(),
        })
    }

    fn simple(
        &self,
        http: &mut dyn Http,
        method: Method,
        path: &str,
        label: &str,
    ) -> Result<String, String> {
        let device = self.target(http)?;
        let path = with_device(path, device.as_ref().map(|d| d.id.as_str()));
        self.send(http, method, &path, None)?;
        Ok(match device {
            Some(device) => format!("{label} on {}", device.name),
            None => label.to_string(),
        })
    }

    fn volume(&self, http: &mut dyn Http, value: u8) -> Result<String, String> {
        let device = self.target(http)?;
        let mut path = format!("/v1/me/player/volume?volume_percent={value}");
        if let Some(device) = &device {
            let _ = write!(path, "&device_id={}", device.id);
        }
        self.send(http, Method::Put, &path, None)?;
        Ok(format!("volume {value}"))
    }

    fn shuffle(&self, http: &mut dyn Http, on: bool) -> Result<String, String> {
        let device = self.target(http)?;
        let mut path = format!("/v1/me/player/shuffle?state={on}");
        if let Some(device) = &device {
            let _ = write!(path, "&device_id={}", device.id);
        }
        self.send(http, Method::Put, &path, None)?;
        Ok(format!("shuffle {}", if on { "on" } else { "off" }))
    }

    fn repeat(&self, http: &mut dyn Http, mode: Repeat) -> Result<String, String> {
        let device = self.target(http)?;
        let mut path = format!("/v1/me/player/repeat?state={}", mode.as_str());
        if let Some(device) = &device {
            let _ = write!(path, "&device_id={}", device.id);
        }
        self.send(http, Method::Put, &path, None)?;
        Ok(format!("repeat {}", mode.as_str()))
    }

    fn transfer(&self, http: &mut dyn Http, name: &str) -> Result<String, String> {
        let device = self.pick(http, Some(name))?;
        self.transfer_id(http, &device.id, false)?;
        Ok(format!("transferred to {}", device.name))
    }

    fn status(&self, http: &mut dyn Http) -> Result<String, String> {
        let value = self.get(http, "/v1/me/player")?;
        if value.is_null() {
            return Ok("nothing is playing".into());
        }
        if self.json {
            return Ok(pretty(&value));
        }
        let device = value.get("device");
        let item = value.get("item");
        let playing = value
            .get("is_playing")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let progress = value
            .get("progress_ms")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let duration = item
            .and_then(|item| item.get("duration_ms"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let track = item
            .and_then(|item| item.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("?");
        let artists = item
            .and_then(|item| item.get("artists"))
            .and_then(Value::as_array)
            .map(|artists| {
                artists
                    .iter()
                    .filter_map(|artist| artist.get("name").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        let album = item
            .and_then(|item| item.get("album"))
            .and_then(|album| album.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("?");
        let device_name = device
            .and_then(|device| device.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("no device");
        let volume = device
            .and_then(|device| device.get("volume_percent"))
            .and_then(Value::as_u64);
        let shuffle = value
            .get("shuffle_state")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let repeat = value
            .get("repeat_state")
            .and_then(Value::as_str)
            .unwrap_or("off");

        let mut out = String::new();
        let _ = write!(out, "device: {device_name}");
        if let Some(volume) = volume {
            let _ = write!(out, " ({volume}%)");
        }
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "state: {} {} / {}",
            if playing { "playing" } else { "paused" },
            clock(progress),
            clock(duration),
        );
        if artists.is_empty() {
            let _ = writeln!(out, "track: {track}");
        } else {
            let _ = writeln!(out, "track: {track} — {artists}");
        }
        let _ = writeln!(out, "album: {album}");
        let _ = write!(
            out,
            "shuffle: {}  repeat: {repeat}",
            if shuffle { "on" } else { "off" }
        );
        Ok(out)
    }

    fn devices_text(&self, http: &mut dyn Http) -> Result<String, String> {
        let devices = self.devices(http)?;
        if self.json {
            let value = serde_json::to_value(
                devices
                    .iter()
                    .map(|device| {
                        json!({
                            "id": device.id,
                            "name": device.name,
                            "active": device.active,
                            "volume": device.volume,
                        })
                    })
                    .collect::<Vec<_>>(),
            )
            .map_err(|error| error.to_string())?;
            return Ok(pretty(&value));
        }
        let mut out = String::new();
        for device in devices {
            let _ = writeln!(
                out,
                "{}\t{}\t{}\t{}",
                if device.active { "*" } else { " " },
                device.name,
                device
                    .volume
                    .map(|volume| format!("{volume}%"))
                    .unwrap_or_else(|| "-".into()),
                device.id,
            );
        }
        Ok(out.trim_end().to_string())
    }
}

const USAGE: &str = "\
usage: spotify [--device NAME] [--json] COMMAND

commands:
  status              current device, track, progress, shuffle/repeat
  devices             list Connect devices (RASpotify appears here)
  play [URI]          resume, or play a track/album/playlist URI or link
  pause | next | prev transport control
  volume N            set volume 0-100
  transfer NAME       move playback to a device by name
  shuffle on|off      toggle shuffle
  repeat off|track|context
\n\
examples:
  spotify devices
  spotify --device raspotify play
  spotify --device raspotify volume 40
  spotify play https://open.spotify.com/album/1DFixLWuPkv3KT3TnV35m3";

fn with_device(path: &str, device: Option<&str>) -> String {
    match device {
        Some(id) => format!("{path}?device_id={id}"),
        None => path.to_string(),
    }
}

fn normalize_uri(input: &str) -> Result<String, String> {
    if let Some(rest) = input.strip_prefix("spotify:") {
        let mut parts = rest.split(':');
        let (Some(kind), Some(id)) = (parts.next(), parts.next()) else {
            return Err(format!("spotify: {input:?} is not a Spotify URI"));
        };
        if kind.is_empty() || id.is_empty() {
            return Err(format!("spotify: {input:?} is not a Spotify URI"));
        }
        return Ok(format!("spotify:{kind}:{id}"));
    }
    let link = input
        .strip_prefix("https://open.spotify.com/")
        .or_else(|| input.strip_prefix("http://open.spotify.com/"))
        .or_else(|| input.strip_prefix("open.spotify.com/"));
    let Some(link) = link else {
        return Err(format!(
            "spotify: {input:?} is not a Spotify URI or open.spotify.com link"
        ));
    };
    let path = link.split(['?', '#']).next().unwrap_or(link);
    let mut parts = path.split('/');
    let (Some(kind), Some(id)) = (parts.next(), parts.next()) else {
        return Err(format!(
            "spotify: {input:?} is not a Spotify URI or open.spotify.com link"
        ));
    };
    if kind.is_empty() || id.is_empty() {
        return Err(format!(
            "spotify: {input:?} is not a Spotify URI or open.spotify.com link"
        ));
    }
    Ok(format!("spotify:{kind}:{id}"))
}

fn decode(response: Response) -> Result<Value, String> {
    if response.status == 204 {
        return Ok(Value::Null);
    }
    if !(200..300).contains(&response.status) {
        let message = serde_json::from_slice::<Value>(&response.body)
            .ok()
            .and_then(|value| {
                value
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| String::from_utf8_lossy(&response.body).trim().to_string());
        let hint = if response.status == 401 {
            " (the host's Spotify token may have expired)"
        } else {
            ""
        };
        return Err(format!(
            "spotify: HTTP {}{hint}: {message}",
            response.status
        ));
    }
    if response.body.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_slice(&response.body)
        .map_err(|error| format!("spotify: invalid JSON: {error}"))
}

fn clock(ms: u64) -> String {
    let seconds = ms / 1000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
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
        fn new(responses: impl IntoIterator<Item = (u16, Value)>) -> Self {
            Self {
                requests: Vec::new(),
                responses: responses
                    .into_iter()
                    .map(|(status, value)| Response {
                        status,
                        body: if value.is_null() {
                            Vec::new()
                        } else {
                            serde_json::to_vec(&value).unwrap()
                        },
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

    fn devices_reply(active: bool) -> Value {
        json!({
            "devices": [{
                "id": "pi123",
                "name": "raspotify (home-pi)",
                "is_active": active,
                "volume_percent": 40
            }]
        })
    }

    #[test]
    fn devices_marks_the_active_one() {
        let mut http = MockHttp::new([(200, devices_reply(true))]);
        let output = run(&args("devices"), &mut http).unwrap();
        assert!(output.starts_with("*\traspotify"), "{output}");
        assert!(output.contains("40%"), "{output}");
    }

    #[test]
    fn play_wakes_an_idle_pi_and_targets_it() {
        let mut http = MockHttp::new([
            (200, devices_reply(false)),
            (204, Value::Null),
            (204, Value::Null),
        ]);
        let output = run(&args("--device raspotify play"), &mut http).unwrap();
        assert_eq!(output, "playing");
        assert_eq!(http.requests.len(), 3);
        assert_eq!(http.requests[1].url, "https://api.spotify.com/v1/me/player");
        assert_eq!(
            http.requests[1].body.as_deref(),
            Some(br#"{"device_ids":["pi123"],"play":false}"#.as_slice())
        );
        assert_eq!(
            http.requests[2].url,
            "https://api.spotify.com/v1/me/player/play?device_id=pi123"
        );
    }

    #[test]
    fn play_a_track_sends_uris() {
        let mut http = MockHttp::new([(200, devices_reply(true)), (204, Value::Null)]);
        run(
            &args("--device raspotify play spotify:track:abc"),
            &mut http,
        )
        .unwrap();
        let body: Value =
            serde_json::from_slice(http.requests[1].body.as_deref().unwrap()).unwrap();
        assert_eq!(body, json!({ "uris": ["spotify:track:abc"] }));
    }

    #[test]
    fn an_open_spotify_link_becomes_a_context_uri() {
        let mut http = MockHttp::new([(200, devices_reply(true)), (204, Value::Null)]);
        run(
            &args("--device raspotify play https://open.spotify.com/album/1DFixLWuPkv3KT3TnV35m3?si=x"),
            &mut http,
        )
        .unwrap();
        let body: Value =
            serde_json::from_slice(http.requests[1].body.as_deref().unwrap()).unwrap();
        assert_eq!(
            body,
            json!({ "context_uri": "spotify:album:1DFixLWuPkv3KT3TnV35m3" })
        );
    }

    #[test]
    fn ambiguous_devices_are_rejected() {
        let mut http = MockHttp::new([(
            200,
            json!({
                "devices": [
                    { "id": "a", "name": "raspotify kitchen", "is_active": true },
                    { "id": "b", "name": "raspotify office", "is_active": false }
                ]
            }),
        )]);
        let error = run(&args("--device raspotify play"), &mut http).unwrap_err();
        assert!(error.contains("more than one device"), "{error}");
    }

    #[test]
    fn a_pause_without_a_device_uses_the_active_one() {
        let mut http = MockHttp::new([(204, Value::Null)]);
        let output = run(&args("pause"), &mut http).unwrap();
        assert_eq!(output, "paused");
        assert_eq!(http.requests.len(), 1);
        assert_eq!(
            http.requests[0].url,
            "https://api.spotify.com/v1/me/player/pause"
        );
    }

    #[test]
    fn status_renders_device_and_track() {
        let mut http = MockHttp::new([(
            200,
            json!({
                "is_playing": true,
                "progress_ms": 83_000,
                "shuffle_state": false,
                "repeat_state": "context",
                "device": { "name": "raspotify (home-pi)", "volume_percent": 40 },
                "item": {
                    "name": "Dead Wax",
                    "duration_ms": 225_000,
                    "artists": [{ "name": "The Needle" }],
                    "album": { "name": "Long Player" }
                }
            }),
        )]);
        let output = run(&args("status"), &mut http).unwrap();
        assert!(
            output.contains("device: raspotify (home-pi) (40%)"),
            "{output}"
        );
        assert!(output.contains("state: playing 1:23 / 3:45"), "{output}");
        assert!(output.contains("track: Dead Wax — The Needle"), "{output}");
        assert!(output.contains("repeat: context"), "{output}");
    }

    #[test]
    fn nothing_playing_is_a_state_not_an_error() {
        let mut http = MockHttp::new([(204, Value::Null)]);
        let output = run(&args("status"), &mut http).unwrap();
        assert_eq!(output, "nothing is playing");
    }

    #[test]
    fn auth_errors_name_the_token() {
        let mut http = MockHttp::new([(
            401,
            json!({ "error": { "status": 401, "message": "The access token expired" } }),
        )]);
        let error = run(&args("status"), &mut http).unwrap_err();
        assert!(error.contains("401"), "{error}");
        assert!(error.contains("token may have expired"), "{error}");
        assert!(error.contains("The access token expired"), "{error}");
    }

    #[test]
    fn volume_is_bounded() {
        let mut http = MockHttp::new([(204, Value::Null)]);
        assert!(run(&args("volume 100"), &mut http).is_ok());
        assert!(run(&args("volume 101"), &mut http).is_err());
    }

    #[test]
    fn uris_are_normalized() {
        assert_eq!(normalize_uri("spotify:track:x").unwrap(), "spotify:track:x");
        assert_eq!(
            normalize_uri("open.spotify.com/playlist/abc").unwrap(),
            "spotify:playlist:abc"
        );
        assert!(normalize_uri("https://example.com/track/x").is_err());
    }

    #[test]
    fn transfer_picks_by_substring() {
        let mut http = MockHttp::new([(200, devices_reply(false)), (204, Value::Null)]);
        let output = run(&args("transfer home-pi"), &mut http).unwrap();
        assert_eq!(output, "transferred to raspotify (home-pi)");
        assert_eq!(http.requests[1].url, "https://api.spotify.com/v1/me/player");
    }
}
