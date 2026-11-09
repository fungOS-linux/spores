//! The guest side: `soil:appdata`, `soil:vfs`, `soil:audio`.
//!
//! The host calls `snapshot` for the board and `act` for one intent. `run` is
//! the command-shaped entrypoint for a host that invokes the module once per
//! event. State lives here because a Spore owns its document; the view never
//! holds a pointer to a file or a sound.

use std::cell::RefCell;

use serde_json::Value;

wit_bindgen::generate!({ path: "wit", world: "sfx", generate_all });

use soil::appdata::appdata::{self, Scope, Store};
use soil::audio::audio::{self, Format, RoomKind};
use soil::vfs::vfs;

use crate::mapping::{self, Mapping, SamplePad};
use crate::snapshot::{RoomInfo, Status};
use crate::{actions::action_parts, decode, snapshot, synth};

const RATE: u32 = 48_000;
const CHANNELS: u8 = 2;
const MAX_SAMPLE_BYTES: u64 = 32 * 1024 * 1024;
const READ_CHUNK: u32 = 256 * 1024;
const SYNTH_PATH: &str = "sfx/synth.lua";
const MAX_SYNTH_BYTES: u64 = 256 * 1024;

struct Output {
    inner: audio::Output,
    requested: Option<String>,
    room: String,
    channels: u8,
}

impl Output {
    fn play(&mut self, mono: &[f32], gain: f32) -> Result<(), String> {
        let frames = decode::interleave(mono, self.channels, gain);
        let channels = usize::from(self.channels);
        let mut offset = 0;
        while offset < frames.len() {
            let accepted =
                self.inner
                    .write(&frames[offset..])
                    .map_err(|error| format!("audio write: {error:?}"))? as usize;
            if accepted == 0 {
                self.inner
                    .drain(50)
                    .map_err(|error| format!("audio drain: {error:?}"))?;
            } else {
                offset += accepted * channels;
            }
        }
        Ok(())
    }
}

struct State {
    store: Option<Store>,
    mapping: Mapping,
    status: Status,
    output: Option<Output>,
}

impl State {
    fn load() -> Self {
        let mut status = Status::default();
        let (scope, store) = match appdata::open(Scope::Synced) {
            Ok(store) => (Some("synced"), Some(store)),
            Err(_) => match appdata::open(Scope::Local) {
                Ok(store) => (Some("local"), Some(store)),
                Err(error) => {
                    status.mapping_error = Some(format!("appdata unavailable: {error:?}"));
                    (None, None)
                }
            },
        };
        status.scope = scope.map(str::to_string);

        let mut mapping = mapping::default_mapping();
        if let Some(store) = &store {
            match store.get(mapping::MAPPING_KEY) {
                Ok(Some(bytes)) => match mapping::parse(&bytes) {
                    Ok(stored) => mapping = stored,
                    Err(error) => status.mapping_error = Some(error),
                },
                Ok(None) => {}
                Err(error) => status.mapping_error = Some(format!("appdata read: {error:?}")),
            }
            status.generation = store.generation().ok();
        }
        refresh_rooms(&mut status);
        refresh_synth(&mut status);
        Self {
            store,
            mapping,
            status,
            output: None,
        }
    }
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::load());
}

fn with_state<T>(f: impl FnOnce(&mut State) -> Result<T, String>) -> Result<T, String> {
    STATE.with(|cell| f(&mut cell.borrow_mut()))
}

fn refresh_rooms(status: &mut Status) {
    match audio::rooms() {
        Ok(rooms) => {
            status.rooms = rooms
                .into_iter()
                .map(|room| RoomInfo {
                    name: room.name,
                    label: room.label,
                    kind: match room.kind {
                        RoomKind::Local => "local".into(),
                        RoomKind::Remote => "remote".into(),
                    },
                    live: room.live,
                    principal: room.principal,
                })
                .collect();
            status.audio_error = None;
        }
        Err(error) => status.audio_error = Some(format!("rooms: {error:?}")),
    }
    match audio::current_room() {
        Ok(room) => status.current_room = room.map(|room| room.name),
        Err(error) => {
            if status.audio_error.is_none() {
                status.audio_error = Some(format!("current room: {error:?}"));
            }
        }
    }
}

/// Load pad definitions from the workspace. A missing file is normal (the
/// embedded copy is the default); a malformed file is reported and leaves the
/// previous definitions in place.
fn refresh_synth(status: &mut Status) {
    let entry = match vfs::stat(SYNTH_PATH) {
        Ok(entry) => entry,
        Err(_) => return,
    };
    if entry.kind != vfs::Kind::File || entry.size > MAX_SYNTH_BYTES {
        status.synth_error = Some(format!("{SYNTH_PATH} is not a usable synth document"));
        return;
    }
    let chunk = match vfs::read(SYNTH_PATH, 0, entry.size as u32) {
        Ok(chunk) => chunk,
        Err(error) => {
            status.synth_error = Some(format!("read {SYNTH_PATH}: {error:?}"));
            return;
        }
    };
    match std::str::from_utf8(&chunk.data) {
        Ok(source) => match synth::install(source) {
            Ok(()) => status.synth_error = None,
            Err(error) => status.synth_error = Some(error),
        },
        Err(_) => status.synth_error = Some(format!("{SYNTH_PATH} is not UTF-8")),
    }
}

fn save(state: &mut State) -> Result<(), String> {
    let bytes = mapping::to_bytes(&state.mapping)?;
    let generation = {
        let store = state
            .store
            .as_ref()
            .ok_or_else(|| "appdata is unavailable; the change is only in memory".to_string())?;
        store
            .set(mapping::MAPPING_KEY, &bytes)
            .map_err(|error| format!("appdata write: {error:?}"))?
    };
    state.status.generation = Some(generation);
    state.status.mapping_error = None;
    Ok(())
}

fn ensure_output(state: &mut State, requested: Option<&str>) -> Result<(), String> {
    let requested = requested.map(str::to_string);
    let stale = match &state.output {
        None => true,
        Some(output) => {
            if output.requested != requested {
                true
            } else if requested.as_deref() == Some("current") {
                // The principal can walk from the lab to the kitchen with the
                // board open. A `current` sink has to follow them. If the host
                // cannot name a current room, keep the sink it already opened
                // rather than reopening on every press.
                match state.status.current_room.as_deref() {
                    Some(current) => output.room != current,
                    None => false,
                }
            } else {
                false
            }
        }
    };
    if stale {
        if let Some(output) = state.output.take() {
            let _ = output.inner.close();
        }
        let opened = audio::open(
            Format {
                sample_rate: RATE,
                channels: CHANNELS,
            },
            requested.as_deref(),
        )
        .map_err(|error| format!("audio open {:?}: {error:?}", requested))?;
        let room = opened
            .status()
            .map(|status| status.room)
            .unwrap_or_else(|_| requested.clone().unwrap_or_default());
        state.output = Some(Output {
            inner: opened,
            requested,
            room,
            channels: CHANNELS,
        });
    }
    Ok(())
}

fn load_sample(sample: &SamplePad) -> Result<Vec<f32>, String> {
    let entry =
        vfs::stat(&sample.path).map_err(|error| format!("vfs stat {}: {error:?}", sample.path))?;
    if entry.kind != vfs::Kind::File {
        return Err(format!("{} is not a file", sample.path));
    }
    if entry.size > MAX_SAMPLE_BYTES {
        return Err(format!("{} is larger than the sample limit", sample.path));
    }
    if entry.content_id != sample.content_id {
        return Err(format!(
            "{} changed (expected {}, found {})",
            sample.path, sample.content_id, entry.content_id
        ));
    }

    let mut bytes = Vec::with_capacity(entry.size.min(MAX_SAMPLE_BYTES) as usize);
    let mut offset = 0u64;
    while offset < entry.size {
        let length = (entry.size - offset).min(u64::from(READ_CHUNK)) as u32;
        let chunk = vfs::read(&sample.path, offset, length)
            .map_err(|error| format!("vfs read {}: {error:?}", sample.path))?;
        if chunk.content_id != sample.content_id {
            return Err(format!("{} changed while it was read", sample.path));
        }
        if chunk.data.is_empty() {
            break;
        }
        bytes.extend_from_slice(&chunk.data);
        offset += chunk.data.len() as u64;
        if chunk.eof {
            break;
        }
    }

    let decoded = decode::decode(&bytes, Some(&sample.path))
        .map_err(|error| format!("{}: {error}", sample.path))?;
    let mut mono = decode::resample(&decoded.samples, decoded.sample_rate, RATE);
    decode::fade_edges(&mut mono, RATE, 3);
    Ok(mono)
}

fn materialise(mapping: &Mapping, pad: &str) -> Result<Vec<f32>, String> {
    if let Some(samples) = synth::render(pad, RATE) {
        return Ok(samples);
    }
    let sample = mapping
        .sample_for(pad)
        .ok_or_else(|| format!("unknown pad {pad:?}"))?;
    load_sample(sample)
}

fn play_pad(
    state: &mut State,
    pad: &str,
    room: Option<&str>,
    gain_scale: f32,
) -> Result<(), String> {
    let mono = materialise(&state.mapping, pad)?;
    if mono.is_empty() {
        return Err(format!("pad {pad:?} is empty"));
    }
    let sample_gain = state
        .mapping
        .sample_for(pad)
        .map(|sample| sample.gain)
        .unwrap_or(1.0);
    let gain = (state.mapping.gain * sample_gain * gain_scale).clamp(0.0, mapping::MAX_GAIN);

    ensure_output(state, room)?;
    {
        let output = state.output.as_mut().ok_or("audio is not open")?;
        output.play(&mono, gain)?;
        state.status.opened_room = Some(output.room.clone());
    }
    state.status.last_pad = Some(pad.to_string());
    state.status.last_error = None;
    Ok(())
}

fn play_event(state: &mut State, kind: &str) -> Result<(), String> {
    let rule = state
        .mapping
        .rule_for(kind)
        .cloned()
        .ok_or_else(|| format!("no rule for {kind:?}"))?;
    if !rule.enabled {
        return Err(format!("{kind} is muted"));
    }
    let room = state.mapping.room_for(Some(&rule)).map(str::to_string);
    play_pad(state, &rule.pad, room.as_deref(), rule.gain)?;
    state.status.last_kind = Some(kind.to_string());
    state.status.selected = Some(rule.pad.clone());
    Ok(())
}

fn stop(state: &mut State) {
    if let Some(output) = state.output.take() {
        let _ = output.inner.close();
    }
    state.status.opened_room = None;
}

fn act_value(state: &mut State, name: &str, params: &Value) -> Result<(), String> {
    let arg = |key: &str| -> Result<&str, String> {
        params
            .get(key)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("action {name} needs {key:?}"))
    };
    match name {
        "sfx.trigger" => {
            let pad = arg("pad")?;
            let room = params
                .get("room")
                .and_then(Value::as_str)
                .map(str::to_string);
            if let Some(kind) = state.status.bind.take() {
                state.mapping.bind(&kind, pad)?;
                save(state)?;
            }
            let room = room.or_else(|| state.mapping.room.clone());
            play_pad(state, pad, room.as_deref(), 1.0)?;
            state.status.selected = Some(pad.to_string());
            Ok(())
        }
        "sfx.event" | "sfx.test" => {
            let kind = arg("kind")?;
            play_event(state, kind)
        }
        "sfx.bind" => {
            let kind = arg("kind")?;
            if let Some(pad) = params.get("pad").and_then(Value::as_str) {
                state.mapping.bind(kind, pad)?;
                save(state)?;
                state.status.bind = None;
            } else {
                state.status.bind = Some(kind.to_string());
            }
            Ok(())
        }
        "sfx.cancel_bind" => {
            state.status.bind = None;
            Ok(())
        }
        "sfx.select" => {
            state.status.selected = Some(arg("pad")?.to_string());
            Ok(())
        }
        "sfx.stop" => {
            stop(state);
            Ok(())
        }
        "sfx.enabled" => {
            let kind = arg("kind")?;
            let enabled = params
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| "sfx.enabled needs enabled".to_string())?;
            state.mapping.set_enabled(kind, enabled)?;
            save(state)
        }
        "sfx.room" => {
            let room = params
                .get("room")
                .and_then(Value::as_str)
                .map(str::to_string);
            if let Some(room) = &room {
                if !mapping::valid_room(room) {
                    return Err(format!("room {room:?} is not a room name"));
                }
            }
            state.mapping.room = room;
            save(state)?;
            stop(state);
            Ok(())
        }
        "sfx.rule_room" => {
            let kind = arg("kind")?;
            let room = params
                .get("room")
                .and_then(Value::as_str)
                .map(str::to_string);
            if let Some(room) = &room {
                if !mapping::valid_room(room) {
                    return Err(format!("room {room:?} is not a room name"));
                }
            }
            let rule = state
                .mapping
                .rules
                .iter_mut()
                .find(|rule| rule.kind == kind)
                .ok_or_else(|| format!("no rule for {kind:?}"))?;
            rule.room = room;
            save(state)?;
            stop(state);
            Ok(())
        }
        "sfx.gain" => {
            let gain = params
                .get("gain")
                .and_then(Value::as_f64)
                .ok_or_else(|| "sfx.gain needs gain".to_string())? as f32;
            if !gain.is_finite() || !(0.0..=mapping::MAX_GAIN).contains(&gain) {
                return Err(format!(
                    "gain {gain} is outside 0..={MAX_GAIN}",
                    MAX_GAIN = mapping::MAX_GAIN
                ));
            }
            state.mapping.gain = gain;
            save(state)
        }
        other => Err(format!("unknown action {other:?}")),
    }
}

fn snapshot_bytes(state: &mut State) -> Result<Vec<u8>, String> {
    refresh_rooms(&mut state.status);
    serde_json::to_vec(&snapshot::snapshot_json(&state.mapping, &state.status))
        .map_err(|error| format!("snapshot JSON: {error}"))
}

struct Component;

export!(Component);

impl Guest for Component {
    fn run(args: Vec<String>) -> Result<(), String> {
        let command = args.first().map(String::as_str).unwrap_or("");
        with_state(|state| {
            // `run` is the command-shaped entrypoint; it must see the same
            // current room and pad definitions an `act` would, not the values
            // from install time.
            refresh_rooms(&mut state.status);
            refresh_synth(&mut state.status);
            match command {
                "" | "help" => Ok(()),
                "play" => {
                    let pad = args.get(1).ok_or("play needs a pad")?;
                    let room = state.mapping.room.clone();
                    play_pad(state, pad, room.as_deref(), 1.0)
                }
                "event" => {
                    let kind = args.get(1).ok_or("event needs a kind")?;
                    play_event(state, kind)
                }
                "stop" => {
                    stop(state);
                    Ok(())
                }
                "selftest" => {
                    for pad in ["blip", "ping", "chime"] {
                        let room = state.mapping.room.clone();
                        play_pad(state, pad, room.as_deref(), 1.0)?;
                    }
                    Ok(())
                }
                other => Err(format!("unknown command {other:?}")),
            }
        })
    }

    fn snapshot() -> Result<Vec<u8>, String> {
        with_state(snapshot_bytes)
    }

    fn act(action: String) -> Result<Vec<u8>, String> {
        with_state(|state| {
            // Presence can change between two actions; resolve the room before
            // the action, not only when the next snapshot is painted.
            refresh_rooms(&mut state.status);
            refresh_synth(&mut state.status);
            state.status.last_error = None;
            let value: Value =
                serde_json::from_str(&action).map_err(|error| format!("action JSON: {error}"))?;
            let (name, params) = action_parts(&value)?;
            if let Err(error) = act_value(state, name, &params) {
                state.status.last_error = Some(error);
            }
            snapshot_bytes(state)
        })
    }
}
