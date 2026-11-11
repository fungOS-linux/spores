//! `lights-host`: a loopback browser host for the Lights Lua view.
//!
//! The host owns the bridge connection. It asks the existing `hue` Spore for
//! state through `soil-http`, renders the Lua view through `tools/shot.lua`, and
//! routes the view's `lights.*` intents back through that same Spore. The browser
//! only sees a scene document and hit regions.

use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::PathBuf,
    process::Command,
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use tiny_http::{Header, Method, Response, Server, StatusCode};

const PAGE: &str = include_str!("page.html");
const CACHE_TTL: Duration = Duration::from_secs(2);
const SCENE_LIMIT: usize = 4;
const SCENE_PRIORITY: &[&str] = &[
    "Relax",
    "Movie",
    "Dinner",
    "Rest",
    "Read",
    "Nightlight",
    "Tokyo",
    "Energize",
    "Sleepy",
];

struct Config {
    bridge: String,
    credential: String,
    geode_socket: PathBuf,
    geode_purpose: String,
    soil_http: PathBuf,
    module: PathBuf,
    sdk: PathBuf,
    view: PathBuf,
    theme: PathBuf,
    renderer: PathBuf,
    lua: String,
}

struct State {
    config: Config,
    selected: Mutex<String>,
    cache: Mutex<Option<(Instant, Value)>>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("lights-host: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut args = env::args().skip(1);
    let mut bridge = None;
    let mut credential = "geode://home/hue/bridge".to_string();
    let mut geode_socket = home_path(".geode/geode.sock");
    let mut geode_purpose = "hue.control".to_string();
    let mut soil_http = PathBuf::from("soil-http");
    let mut module = PathBuf::from("spores/hue/module.wasm");
    let mut sdk = PathBuf::from(".");
    let mut root = PathBuf::from("spores/lights");
    let mut view = None;
    let mut theme = None;
    let mut renderer = None;
    let mut lua = env::var("TEND_LUA").unwrap_or_else(|_| "lua".to_string());
    let mut bind = "127.0.0.1:8814".to_string();
    let mut selected = "81".to_string();
    let mut action = None;

    while let Some(flag) = args.next() {
        let mut value =
            || -> Result<String> { args.next().ok_or_else(|| anyhow!("{flag} needs a value")) };
        match flag.as_str() {
            "--bridge" => bridge = Some(value()?),
            "--credential" => credential = value()?,
            "--geode-socket" => geode_socket = PathBuf::from(value()?),
            "--geode-purpose" => geode_purpose = value()?,
            "--soil-http" => soil_http = PathBuf::from(value()?),
            "--module" => module = PathBuf::from(value()?),
            "--sdk" => sdk = PathBuf::from(value()?),
            "--root" => root = PathBuf::from(value()?),
            "--view" => view = Some(PathBuf::from(value()?)),
            "--theme" => theme = Some(PathBuf::from(value()?)),
            "--renderer" => renderer = Some(PathBuf::from(value()?)),
            "--lua" => lua = value()?,
            "--bind" => bind = value()?,
            "--selected" => selected = value()?,
            "--action" => action = Some(value()?),
            "--help" | "-h" => {
                println!(
                    "lights-host --bridge URL [--credential geode://home/hue/bridge] \\\n                     [--geode-socket PATH] [--geode-purpose PURPOSE] \\\n                     [--soil-http PATH] [--module module.wasm] [--sdk DIR] \\\n                     [--root spores/lights] [--view FILE] [--theme FILE] \\\n                     [--renderer tools/shot.lua] [--bind 127.0.0.1:8814] \\\n                     [--action JSON]"
                );
                return Ok(());
            }
            other => bail!("unknown flag {other}"),
        }
    }

    let bridge = bridge.ok_or_else(|| anyhow!("--bridge http://192.168.10.15 is required"))?;
    let view = view.unwrap_or_else(|| root.join("ui/lights.lua"));
    let theme = theme.unwrap_or_else(|| PathBuf::from("spores/themes/graphite.lua"));
    let renderer = renderer.unwrap_or_else(|| root.join("../../tools/shot.lua"));
    let state = State {
        config: Config {
            bridge,
            credential,
            geode_socket,
            geode_purpose,
            soil_http,
            module,
            sdk,
            view,
            theme,
            renderer,
            lua,
        },
        selected: Mutex::new(String::new()),
        cache: Mutex::new(None),
    };
    *state.selected.lock().expect("selected lock") = selected;

    if let Some(action) = action {
        let call: Value = serde_json::from_str(&action).context("parse --action JSON")?;
        state.apply(&call)?;
        println!("applied");
        return Ok(());
    }

    let server = Server::http(&bind).map_err(|error| anyhow!("bind {bind}: {error}"))?;
    println!(
        "lights-host: http://{bind}/  (bridge {})",
        state.config.bridge
    );
    for request in server.incoming_requests() {
        if let Err(error) = handle(request, &state) {
            eprintln!("lights-host: {error:#}");
        }
    }
    Ok(())
}

fn handle(mut request: tiny_http::Request, state: &State) -> Result<()> {
    let target = request.url().to_string();
    let path = target.split('?').next().unwrap_or("/");
    match (request.method(), path) {
        (Method::Get, "/") => respond(request, 200, "text/html; charset=utf-8", PAGE),
        (Method::Get, "/health") => respond(request, 200, "text/plain", "ok\n"),
        (Method::Get, "/scene.json") => {
            let query = target.split_once('?').map_or("", |(_, query)| query);
            let width = query_value(query, "w").unwrap_or(1200).clamp(320, 4096);
            let height = query_value(query, "h").unwrap_or(820).clamp(320, 4096);
            match state.render(width, height) {
                Ok(scene) => respond(request, 200, "application/json", &scene),
                Err(error) => respond(request, 500, "text/plain", &format!("{error:#}")),
            }
        }
        (Method::Post, "/action") => {
            let mut body = String::new();
            request
                .as_reader()
                .read_to_string(&mut body)
                .context("read action body")?;
            match serde_json::from_str::<Value>(&body)
                .context("parse action JSON")
                .and_then(|call| state.apply(&call))
            {
                Ok(()) => respond(request, 200, "application/json", "{\"ok\":true}"),
                Err(error) => respond(request, 400, "text/plain", &format!("{error:#}")),
            }
        }
        _ => respond(request, 404, "text/plain", "not found\n"),
    }
}

fn respond(request: tiny_http::Request, status: u16, content_type: &str, body: &str) -> Result<()> {
    let header = Header::from_bytes(&b"content-type"[..], content_type.as_bytes())
        .map_err(|_| anyhow!("invalid content-type"))?;
    let response = Response::from_string(body)
        .with_status_code(StatusCode(status))
        .with_header(header);
    request.respond(response).context("respond")
}

impl State {
    fn snapshot(&self) -> Result<Value> {
        if let Some((at, cached)) = self.cache.lock().expect("cache lock").as_ref() {
            if at.elapsed() < CACHE_TTL {
                return Ok(cached.clone());
            }
        }
        let snapshot = self.build_snapshot()?;
        *self.cache.lock().expect("cache lock") = Some((Instant::now(), snapshot.clone()));
        Ok(snapshot)
    }

    fn build_snapshot(&self) -> Result<Value> {
        let status = self.hue_json(&["--json", "status"])?;
        let groups = self.hue_json(&["--json", "groups"])?;
        let lights = self.hue_json(&["--json", "lights"])?;
        let scenes = self.hue_json(&["--json", "scenes"])?;

        let bridge = status
            .get("bridge")
            .and_then(|bridge| bridge.get("config"))
            .cloned()
            .or_else(|| status.get("bridge").cloned())
            .unwrap_or(Value::Null);
        let light_map = lights.as_object().cloned().unwrap_or_default();
        let group_map = groups.as_object().cloned().unwrap_or_default();
        let scene_map = scenes.as_object().cloned().unwrap_or_default();

        let mut rooms = Vec::new();
        let mut ids: Vec<&String> = group_map.keys().collect();
        ids.sort_by_key(|id| id.parse::<u64>().unwrap_or(u64::MAX));
        for id in ids {
            let group = &group_map[id];
            let members: Vec<String> = group["lights"]
                .as_array()
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            let mut on = 0u64;
            let mut brightness = 0u64;
            let mut lit = 0u64;
            for member in &members {
                let Some(light) = light_map.get(member) else {
                    continue;
                };
                let state = &light["state"];
                if state["on"].as_bool() == Some(true) {
                    on += 1;
                    lit += 1;
                    brightness += state["bri"].as_u64().unwrap_or(0);
                }
            }
            let bri = group["action"]["bri"]
                .as_u64()
                .unwrap_or_else(|| if lit == 0 { 0 } else { brightness / lit });
            rooms.push(json!({
                "id": id,
                "name": group["name"].as_str().unwrap_or("room"),
                "on": on,
                "total": members.len(),
                "bri_pct": percent(bri),
                "lights": members,
            }));
        }

        let selected = {
            let wanted = self.selected.lock().expect("selected lock").clone();
            if rooms
                .iter()
                .any(|room| room["id"].as_str() == Some(wanted.as_str()))
            {
                wanted
            } else if rooms.iter().any(|room| room["id"].as_str() == Some("81")) {
                "81".to_string()
            } else {
                rooms
                    .first()
                    .and_then(|room| room["id"].as_str())
                    .unwrap_or("0")
                    .to_string()
            }
        };
        *self.selected.lock().expect("selected lock") = selected.clone();

        let room_name = |id: &str| {
            rooms
                .iter()
                .find(|room| room["id"].as_str() == Some(id))
                .and_then(|room| room["name"].as_str())
                .unwrap_or("group")
                .to_string()
        };

        let mut by_group: BTreeMap<String, Vec<(usize, String, Value)>> = BTreeMap::new();
        for (id, scene) in &scene_map {
            let Some(group_id) = scene["group"].as_str() else {
                continue;
            };
            let name = scene["name"].as_str().unwrap_or("Scene").to_string();
            let rank = SCENE_PRIORITY
                .iter()
                .position(|candidate| *candidate == name)
                .unwrap_or(SCENE_PRIORITY.len());
            by_group.entry(group_id.to_string()).or_default().push((
                rank,
                id.clone(),
                json!({
                    "id": id,
                    "name": name,
                    "group_id": group_id,
                    "group": room_name(group_id),
                }),
            ));
        }
        let mut scenes_out = Vec::new();
        for entries in by_group.values_mut() {
            entries.sort_by(|left, right| {
                left.0
                    .cmp(&right.0)
                    .then_with(|| left.2["name"].as_str().cmp(&right.2["name"].as_str()))
            });
            let mut names = BTreeSet::new();
            let mut shown = 0;
            for (_, _, scene) in entries.iter() {
                if shown >= SCENE_LIMIT {
                    break;
                }
                let name = scene["name"].as_str().unwrap_or("Scene").to_string();
                if names.insert(name) {
                    scenes_out.push(scene.clone());
                    shown += 1;
                }
            }
        }

        let mut lights_out = Vec::new();
        let mut light_ids: Vec<&String> = light_map.keys().collect();
        light_ids.sort_by_key(|id| id.parse::<u64>().unwrap_or(u64::MAX));
        for id in light_ids {
            let light = &light_map[id];
            let state = &light["state"];
            let ct = state["ct"].as_u64().unwrap_or(0);
            lights_out.push(json!({
                "id": id,
                "name": light["name"].as_str().unwrap_or("light"),
                "on": state["on"].as_bool().unwrap_or(false),
                "bri": state["bri"].as_u64().unwrap_or(0),
                "temperature_k": if ct > 0 { Some(1_000_000 / ct) } else { None },
            }));
        }

        Ok(json!({
            "title": "Lights",
            "selected": selected,
            "bridge": {
                "name": bridge["name"].as_str().unwrap_or("Hue Bridge"),
                "model": bridge["modelid"].as_str().unwrap_or("Hue Bridge"),
                "online": true,
                "lights": light_map.len(),
            },
            "rooms": rooms,
            "scenes": scenes_out,
            "lights": lights_out,
        }))
    }

    fn render(&self, width: u32, height: u32) -> Result<String> {
        let snapshot = self.snapshot()?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let base = env::temp_dir().join(format!("lights-host-{}-{stamp}", std::process::id()));
        let snapshot_path = base.with_extension("lua");
        let scene_path = base.with_extension("json");
        fs::write(&snapshot_path, format!("return {}\n", lua_value(&snapshot)))
            .context("write snapshot")?;
        let output = Command::new(&self.config.lua)
            .arg(&self.config.renderer)
            .arg(&self.config.view)
            .arg(&snapshot_path)
            .arg(&self.config.theme)
            .arg(&scene_path)
            .arg(width.to_string())
            .arg(height.to_string())
            .env("FUNGOS_SDK", &self.config.sdk)
            .output()
            .with_context(|| {
                format!("run {} {}", self.config.lua, self.config.renderer.display())
            })?;
        let _ = fs::remove_file(&snapshot_path);
        if !output.status.success() {
            let _ = fs::remove_file(&scene_path);
            bail!(
                "renderer failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let scene = fs::read_to_string(&scene_path).context("read scene")?;
        let _ = fs::remove_file(&scene_path);
        Ok(scene)
    }

    fn apply(&self, call: &Value) -> Result<()> {
        let action = call["action"]
            .as_str()
            .ok_or_else(|| anyhow!("action is required"))?;
        let body = call.get("body").cloned().unwrap_or_else(|| json!({}));
        match action {
            "lights.room" => {
                let id = body["id"]
                    .as_str()
                    .ok_or_else(|| anyhow!("lights.room needs id"))?;
                *self.selected.lock().expect("selected lock") = id.to_string();
            }
            "lights.room_power" => {
                let id = body["group_id"]
                    .as_str()
                    .ok_or_else(|| anyhow!("lights.room_power needs group_id"))?;
                let on = body["on"].as_bool().unwrap_or(false);
                self.hue(&["--group", id, if on { "on" } else { "off" }])?;
            }
            "lights.power" => {
                let id = body["light_id"]
                    .as_str()
                    .ok_or_else(|| anyhow!("lights.power needs light_id"))?;
                let on = body["on"].as_bool().unwrap_or(false);
                self.hue(&["--light", id, if on { "on" } else { "off" }])?;
            }
            "lights.brightness" => {
                let id = body["light_id"]
                    .as_str()
                    .ok_or_else(|| anyhow!("lights.brightness needs light_id"))?;
                let level = body["level"]
                    .as_u64()
                    .ok_or_else(|| anyhow!("lights.brightness needs level"))?;
                let bri = ((level.min(100) as f64) * 254.0 / 100.0)
                    .round()
                    .clamp(1.0, 254.0) as u64;
                self.hue(&["--light", id, "bri", &bri.to_string()])?;
                // A preset is an invitation to see the change, not a silent
                // write to a lamp that is still off.
                self.hue(&["--light", id, "on"])?;
            }
            "lights.scene" => {
                let scene = body["scene_id"]
                    .as_str()
                    .ok_or_else(|| anyhow!("lights.scene needs scene_id"))?;
                let group = body["group_id"]
                    .as_str()
                    .ok_or_else(|| anyhow!("lights.scene needs group_id"))?;
                self.hue(&["--group", group, "scene-id", scene])?;
            }
            "lights.all_off" => {
                self.hue(&["--group", "0", "off"])?;
            }
            other => bail!("unknown action {other}"),
        }
        *self.cache.lock().expect("cache lock") = None;
        Ok(())
    }

    fn hue_json(&self, args: &[&str]) -> Result<Value> {
        let output = self.hue(args)?;
        serde_json::from_str(output.trim()).with_context(|| format!("decode {args:?}"))
    }

    fn hue(&self, args: &[&str]) -> Result<String> {
        let mut command = Command::new(&self.config.soil_http);
        command
            .arg("--allow")
            .arg(origin(&self.config.bridge))
            .arg("--geode-socket")
            .arg(&self.config.geode_socket)
            .arg("--geode-purpose")
            .arg(&self.config.geode_purpose)
            .arg(&self.config.module)
            .arg("--bridge")
            .arg(&self.config.bridge)
            .arg("--credential")
            .arg(&self.config.credential)
            .args(args);
        let output = command
            .output()
            .with_context(|| format!("run {}", self.config.soil_http.display()))?;
        if !output.status.success() {
            bail!(
                "hue failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

fn origin(bridge: &str) -> String {
    let bridge = bridge.trim_end_matches('/');
    if let Some(rest) = bridge.strip_prefix("http://") {
        if rest.contains(':') {
            bridge.to_string()
        } else {
            format!("{bridge}:80")
        }
    } else if let Some(rest) = bridge.strip_prefix("https://") {
        if rest.contains(':') {
            bridge.to_string()
        } else {
            format!("{bridge}:443")
        }
    } else {
        bridge.to_string()
    }
}

fn percent(bri: u64) -> u64 {
    ((bri as f64) * 100.0 / 254.0).round() as u64
}

fn query_value(query: &str, key: &str) -> Option<u32> {
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == key).then(|| value.parse().ok()).flatten()
    })
}

fn home_path(relative: &str) -> PathBuf {
    env::var_os("HOME").map_or_else(
        || PathBuf::from(relative),
        |home| PathBuf::from(home).join(relative),
    )
}

fn lua_value(value: &Value) -> String {
    match value {
        Value::Null => "nil".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => lua_string(value),
        Value::Array(values) => format!(
            "{{{}}}",
            values.iter().map(lua_value).collect::<Vec<_>>().join(",")
        ),
        Value::Object(values) => {
            let mut parts = Vec::new();
            for (key, value) in values {
                parts.push(format!("[{}]={}", lua_string(key), lua_value(value)));
            }
            format!("{{{}}}", parts.join(","))
        }
    }
}

fn lua_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins_default_ports() {
        assert_eq!(origin("http://192.168.10.15"), "http://192.168.10.15:80");
        assert_eq!(origin("https://example.com"), "https://example.com:443");
        assert_eq!(
            origin("http://192.168.10.15:8080"),
            "http://192.168.10.15:8080"
        );
    }

    #[test]
    fn lua_values_quote_strings_and_nested_tables() {
        let value = json!({"name": "Window \"Lamp\"", "on": true, "ids": ["3", "8"]});
        let lua = lua_value(&value);
        assert!(lua.contains("[\"name\"]=\"Window \\\"Lamp\\\"\""), "{lua}");
        assert!(lua.contains("on\"]=true"), "{lua}");
        assert!(lua.contains("[\"ids\"]={\"3\",\"8\"}"), "{lua}");
    }

    #[test]
    fn brightness_levels_map_to_the_hue_range() {
        assert_eq!(percent(254), 100);
        assert_eq!(percent(0), 0);
    }
}
