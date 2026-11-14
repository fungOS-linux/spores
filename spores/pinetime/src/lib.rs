//! The PineTime Spore: BLE reads/writes in, typed records out.
//!
//! The radio transport and the lease stay in `soil-ble-host`; this module owns
//! the InfiniTime protocol mapping, the wearer identity, and the signal
//! envelope. It has no sockets, filesystem, wall clock, or ambient authority.

use std::cell::RefCell;

use pinetime_protocol as proto;
use serde_json::{json, Value};
use soil::ble::ble;
use soil::ble::types::Event;
use soil::records::records::{self, Entry, Key};
use soil::time::time;

wit_bindgen::generate!({ path: "wit", world: "pinetime-spore", generate_all });

const SOURCE_SPORE: &str = "dev.fungos.pinetime";

thread_local! {
    static SNAPSHOT: RefCell<Value> = RefCell::new(Value::Null);
    static SEQUENCE: RefCell<u64> = RefCell::new(0);
}

struct Component;
export!(Component);

#[derive(Debug, Clone)]
struct Args {
    command: String,
    wearer: String,
    device: Option<String>,
    scan_ms: u32,
    hold_ms: u64,
    category: String,
    title: String,
    body: String,
    artist: String,
    track: String,
    album: String,
    playing: bool,
    epoch: Option<u64>,
    offset_min: i32,
    icon: String,
    narrative: String,
    distance: String,
    progress: u8,
    temp_c: Option<i16>,
    min_c: Option<i16>,
    max_c: Option<i16>,
    location: String,
    sunrise_min: i16,
    sunset_min: i16,
    forecast: Vec<proto::ForecastDay>,
    read_back: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            command: "watch".to_string(),
            wearer: "owner".to_string(),
            device: None,
            scan_ms: 8_000,
            hold_ms: 5_000,
            category: "simple".to_string(),
            title: String::new(),
            body: String::new(),
            artist: String::new(),
            track: String::new(),
            album: String::new(),
            playing: true,
            epoch: None,
            offset_min: 0,
            icon: String::new(),
            narrative: String::new(),
            distance: String::new(),
            progress: 0,
            temp_c: None,
            min_c: None,
            max_c: None,
            location: String::new(),
            sunrise_min: proto::SUN_UNKNOWN,
            sunset_min: proto::SUN_UNKNOWN,
            forecast: Vec::new(),
            read_back: false,
        }
    }
}

impl Args {
    fn parse(raw: &[String]) -> Result<Self, String> {
        let mut args = Self::default();
        let mut index = 0;
        if let Some(first) = raw.first() {
            if !first.starts_with("--") {
                args.command = first.clone();
                index = 1;
            }
        }
        while index < raw.len() {
            let flag = raw[index].as_str();
            // `--read-back` is the one flag that stands alone.
            if flag == "--read-back" {
                args.read_back = true;
                index += 1;
                continue;
            }
            let value = raw
                .get(index + 1)
                .ok_or_else(|| format!("{flag} needs a value"))?;
            match flag {
                "--wearer" => args.wearer = value.clone(),
                "--device" => args.device = Some(value.clone()),
                "--scan-ms" => {
                    args.scan_ms = value
                        .parse()
                        .map_err(|_| format!("--scan-ms: {value:?} is not a number"))?
                }
                "--hold-ms" => {
                    args.hold_ms = value
                        .parse()
                        .map_err(|_| format!("--hold-ms: {value:?} is not a number"))?
                }
                "--category" => args.category = value.clone(),
                "--title" => args.title = value.clone(),
                "--body" => args.body = value.clone(),
                "--artist" => args.artist = value.clone(),
                "--track" => args.track = value.clone(),
                "--album" => args.album = value.clone(),
                "--playing" => args.playing = matches!(value.as_str(), "true" | "1" | "yes"),
                "--epoch" => {
                    args.epoch = Some(
                        value
                            .parse()
                            .map_err(|_| format!("--epoch: {value:?} is not a unix timestamp"))?,
                    )
                }
                "--offset-min" => {
                    args.offset_min = value
                        .parse()
                        .map_err(|_| format!("--offset-min: {value:?} is not a number"))?
                }
                "--icon" => args.icon = value.clone(),
                "--narrative" => args.narrative = value.clone(),
                "--distance" => args.distance = value.clone(),
                "--progress" => {
                    args.progress = value
                        .parse()
                        .map_err(|_| format!("--progress: {value:?} is not a percentage"))?
                }
                "--temp-c" => args.temp_c = Some(degrees(value, "--temp-c")?),
                "--min-c" => args.min_c = Some(degrees(value, "--min-c")?),
                "--max-c" => args.max_c = Some(degrees(value, "--max-c")?),
                "--location" => args.location = value.clone(),
                "--sunrise-min" => args.sunrise_min = count(value, "--sunrise-min")?,
                "--sunset-min" => args.sunset_min = count(value, "--sunset-min")?,
                "--forecast" => args.forecast = forecast(value)?,
                other => return Err(format!("unknown argument {other:?}")),
            }
            index += 2;
        }
        match args.command.as_str() {
            "watch" | "notify" | "media" | "find" | "time" | "nav" | "weather" => Ok(args),
            other => Err(format!(
                "unknown command {other:?}; expected watch, notify, media, find, time, nav, or weather"
            )),
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct Counts {
    heart_rate: u64,
    battery: u64,
    steps: u64,
    motion: u64,
    music: u64,
    calls: u64,
}

impl Counts {
    fn json(self) -> Value {
        json!({
            "heart_rate": self.heart_rate,
            "battery": self.battery,
            "steps": self.steps,
            "motion": self.motion,
            "music": self.music,
            "calls": self.calls,
        })
    }
}

impl Guest for Component {
    fn run(raw: Vec<String>) -> Result<(), String> {
        let result = run_inner(&raw);
        if let Err(error) = &result {
            set_error(error);
        }
        result
    }

    fn snapshot() -> Result<Vec<u8>, String> {
        SNAPSHOT.with(|slot| {
            serde_json::to_vec(&*slot.borrow()).map_err(|error| format!("snapshot JSON: {error}"))
        })
    }
}

fn run_inner(raw: &[String]) -> Result<(), String> {
    let args = Args::parse(raw)?;

    let devices = ble::scan(args.scan_ms).map_err(ble_err)?;
    let target = find_target(&devices, &args)
        .ok_or_else(|| format!("no PineTime found among {} device(s)", devices.len()))?;
    let handle = target.handle;
    let advertised_name = target.name.clone().unwrap_or_default();

    ble::connect(handle).map_err(ble_err)?;
    let result = run_connected(&args, &advertised_name);
    // A Spore should not keep the radio leased after a failed command either.
    let _ = ble::disconnect();
    result
}

fn find_target<'a>(
    devices: &'a [soil::ble::types::Device],
    args: &Args,
) -> Option<&'a soil::ble::types::Device> {
    devices.iter().find(|device| {
        if let Some(wanted) = &args.device {
            device.device_id.as_deref() == Some(wanted.as_str())
                || device.handle.to_string() == *wanted
                || device
                    .name
                    .as_deref()
                    .is_some_and(|name| name.eq_ignore_ascii_case(wanted))
        } else {
            device
                .name
                .as_deref()
                .is_some_and(|name| name.to_ascii_lowercase().contains("pinetime"))
        }
    })
}

fn run_connected(args: &Args, advertised_name: &str) -> Result<(), String> {
    let model = read_string(proto::DIS_MODEL).unwrap_or_else(|_| advertised_name.to_string());
    let firmware = read_string(proto::DIS_FIRMWARE).unwrap_or_default();
    let serial = read_string(proto::DIS_SERIAL).unwrap_or_default();

    // Identity comes from the device, never from the rotating BLE address. If
    // the firmware exposes no serial, an explicit provisioning id is the only
    // honest fallback; inventing one from the platform handle would split the
    // same watch into a new device on every host.
    let device_id = if !serial.is_empty() {
        serial
    } else if let Some(id) = &args.device {
        id.clone()
    } else {
        return Err(
            "PineTime exposed no DIS serial; pass --device <provisioned-id> instead of using the BLE address"
                .to_string(),
        );
    };

    let battery = read_battery()?;
    upsert_device_state(&device_id, &model, &firmware, battery, &args.wearer)?;

    let mut writes: Vec<Written> = Vec::new();
    let counts = match args.command.as_str() {
        "watch" => {
            subscribe_watch()?;
            drain(&device_id, &args.wearer, args.hold_ms, &model, &firmware)?
        }
        "notify" => {
            let category =
                proto::AlertCategory::parse(&args.category).map_err(|error| error.to_string())?;
            let packet = proto::notification_packet(category.as_u8(), &args.title, &args.body);
            proto::ensure_notification_packet(&packet).map_err(|error| error.to_string())?;
            writes.push(write_characteristic(
                proto::NEW_ALERT,
                &packet,
                args.read_back,
            )?);
            Counts::default()
        }
        "media" => {
            write_media(args, &mut writes)?;
            ble::subscribe(proto::MUSIC_EVENT).map_err(ble_err)?;
            drain(&device_id, &args.wearer, args.hold_ms, &model, &firmware)?
        }
        "find" => {
            // Immediate Alert level 2 (high) makes the watch vibrate.
            writes.push(write_characteristic(
                proto::ALERT_LEVEL,
                &[2],
                args.read_back,
            )?);
            Counts::default()
        }
        "time" => {
            // The watch has no timezone of its own: the caller says which local
            // time to display, and the payload carries exactly those fields.
            let packet = proto::current_time_packet(epoch(args)?, args.offset_min)
                .map_err(|error| error.to_string())?;
            writes.push(write_characteristic(
                proto::CURRENT_TIME,
                &packet,
                args.read_back,
            )?);
            Counts::default()
        }
        "nav" => {
            let navigation = proto::Navigation {
                icon: args.icon.clone(),
                narrative: args.narrative.clone(),
                distance: args.distance.clone(),
                progress: args.progress,
            };
            for (uuid, bytes) in navigation.packets().map_err(|error| error.to_string())? {
                writes.push(write_characteristic(uuid, &bytes, args.read_back)?);
            }
            Counts::default()
        }
        "weather" => {
            let icon = proto::WeatherIcon::parse(&args.icon).map_err(|error| error.to_string())?;
            let packet = proto::current_weather_packet(
                epoch(args)?,
                args.temp_c.unwrap_or(0),
                args.min_c.unwrap_or(0),
                args.max_c.unwrap_or(0),
                &args.location,
                icon,
                args.sunrise_min,
                args.sunset_min,
            );
            writes.push(write_characteristic(
                proto::WEATHER_DATA,
                &packet,
                args.read_back,
            )?);
            if !args.forecast.is_empty() {
                let forecast = proto::forecast_packet(epoch(args)?, &args.forecast);
                writes.push(write_characteristic(
                    proto::WEATHER_DATA,
                    &forecast,
                    args.read_back,
                )?);
            }
            Counts::default()
        }
        other => return Err(format!("unknown command {other:?}")),
    };

    let now = time::monotonic_ms().map_err(time_err)?;
    SNAPSHOT.with(|slot| {
        *slot.borrow_mut() = json!({
            "ok": true,
            "command": args.command.clone(),
            "device_id": device_id.clone(),
            "model": model.clone(),
            "firmware": firmware.clone(),
            "battery_percent": battery,
            "wearer": args.wearer.clone(),
            "signals": counts.json(),
            "writes": write_report(&writes),
            "updated_at_monotonic_ms": now,
        });
    });
    Ok(())
}

fn subscribe_watch() -> Result<(), String> {
    for uuid in [
        proto::HEART_RATE_MEASUREMENT,
        proto::STEP_COUNT,
        proto::MOTION_VALUES,
    ] {
        ble::subscribe(uuid).map_err(ble_err)?;
    }
    // Optional on older firmware: a missing music/call service must not make a
    // health sync fail.
    for uuid in [
        proto::BATTERY_LEVEL,
        proto::MUSIC_EVENT,
        proto::NOTIFICATION_EVENT,
    ] {
        if let Err(error) = ble::subscribe(uuid) {
            println!("pinetime: optional subscribe {uuid} failed: {error:?}");
        }
    }
    Ok(())
}

fn drain(
    device_id: &str,
    wearer: &str,
    hold_ms: u64,
    model: &str,
    firmware: &str,
) -> Result<Counts, String> {
    let start = time::monotonic_ms().map_err(time_err)?;
    let deadline = start.saturating_add(hold_ms);
    let mut counts = Counts::default();

    loop {
        let now = time::monotonic_ms().map_err(time_err)?;
        if now >= deadline {
            break;
        }
        let wait = (deadline - now).min(1_000) as u32;
        match ble::next_event(wait).map_err(ble_err)? {
            Some(Event::Notification(notification)) => {
                handle_notification(
                    device_id,
                    wearer,
                    model,
                    firmware,
                    &notification.char_uuid,
                    &notification.data,
                    &mut counts,
                )?;
            }
            Some(Event::Disconnected(reason)) => {
                return Err(format!("PineTime disconnected: {reason}"));
            }
            None => {}
        }
    }
    Ok(counts)
}

fn handle_notification(
    device_id: &str,
    wearer: &str,
    model: &str,
    firmware: &str,
    char_uuid: &str,
    data: &[u8],
    counts: &mut Counts,
) -> Result<(), String> {
    if same(char_uuid, proto::HEART_RATE_MEASUREMENT) {
        let heart_rate = proto::parse_heart_rate(data).map_err(|error| error.to_string())?;
        let value = match heart_rate.bpm {
            Some(bpm) => json!({
                "bpm": bpm,
                "contact": heart_rate.contact,
                "rr_intervals": heart_rate.rr_intervals,
            }),
            None => json!({"bpm": null, "status": "no-measurement"}),
        };
        note_signal(
            device_id,
            wearer,
            "body.hr.v1",
            value,
            "health",
            "raw",
            if heart_rate.bpm.is_some() { 1.0 } else { 0.0 },
        )?;
        counts.heart_rate += 1;
    } else if same(char_uuid, proto::STEP_COUNT) {
        let steps = proto::parse_steps(data).map_err(|error| error.to_string())?;
        note_signal(
            device_id,
            wearer,
            "body.steps.v1",
            json!({"count": steps}),
            "health",
            "summary",
            1.0,
        )?;
        counts.steps += 1;
    } else if same(char_uuid, proto::MOTION_VALUES) {
        let motion = proto::parse_motion(data).map_err(|error| error.to_string())?;
        note_signal(
            device_id,
            wearer,
            "body.motion.v1",
            json!({"x": motion.x, "y": motion.y, "z": motion.z}),
            "health",
            "raw",
            1.0,
        )?;
        counts.motion += 1;
    } else if same(char_uuid, proto::MUSIC_EVENT) {
        let byte = data
            .first()
            .copied()
            .ok_or_else(|| "empty music event".to_string())?;
        let event = proto::parse_music_event(byte).map_err(|error| error.to_string())?;
        note_signal(
            device_id,
            wearer,
            "media.event.v1",
            json!({"event": event.as_str()}),
            "presence",
            "summary",
            1.0,
        )?;
        counts.music += 1;
    } else if same(char_uuid, proto::NOTIFICATION_EVENT) {
        let byte = data
            .first()
            .copied()
            .ok_or_else(|| "empty call response".to_string())?;
        let response = proto::parse_call_response(byte).map_err(|error| error.to_string())?;
        note_signal(
            device_id,
            wearer,
            "call.response.v1",
            json!({"response": response.as_str()}),
            "presence",
            "summary",
            1.0,
        )?;
        counts.calls += 1;
    } else if same(char_uuid, proto::BATTERY_LEVEL) {
        let battery = proto::parse_battery(data).map_err(|error| error.to_string())?;
        // Battery lives on the device state record; it is not a body signal.
        upsert_device_state(device_id, model, firmware, battery, wearer)?;
        counts.battery += 1;
    }
    Ok(())
}

/// One characteristic write, and what a read-back said about it. The read
/// happens immediately: two writes to the same characteristic must not be
/// compared against the second value.
struct Written {
    uuid: String,
    bytes: usize,
    /// `None` when `--read-back` was not asked for.
    verified: Option<Verified>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Verified {
    Matched,
    Mismatched,
    /// The firmware exposes the characteristic as write-only.
    Unsupported,
}

fn write_characteristic(uuid: &str, data: &[u8], read_back: bool) -> Result<Written, String> {
    ble::write(uuid, data).map_err(ble_err)?;
    let verified = if read_back {
        Some(match ble::read(uuid, data.len().max(1) as u32) {
            Ok(value) if value == data => Verified::Matched,
            Ok(_) => Verified::Mismatched,
            Err(_) => Verified::Unsupported,
        })
    } else {
        None
    };
    Ok(Written {
        uuid: uuid.to_string(),
        bytes: data.len(),
        verified,
    })
}

fn write_media(args: &Args, writes: &mut Vec<Written>) -> Result<(), String> {
    let status = [proto::music_status(args.playing)];
    writes.push(write_characteristic(
        proto::MUSIC_STATUS,
        &status,
        args.read_back,
    )?);
    for (uuid, value) in [
        (proto::MUSIC_ARTIST, &args.artist),
        (proto::MUSIC_TRACK, &args.track),
        (proto::MUSIC_ALBUM, &args.album),
    ] {
        if !value.is_empty() {
            writes.push(write_characteristic(
                uuid,
                value.as_bytes(),
                args.read_back,
            )?);
        }
    }
    Ok(())
}

/// What went to the radio, and optionally what the watch read back.
fn write_report(writes: &[Written]) -> Vec<Value> {
    writes
        .iter()
        .map(|written| {
            let mut report = json!({"uuid": written.uuid, "bytes": written.bytes});
            if let Some(verified) = written.verified {
                report["read_back"] = match verified {
                    Verified::Matched => json!(true),
                    Verified::Mismatched => json!("mismatch"),
                    Verified::Unsupported => json!("unsupported"),
                };
            }
            report
        })
        .collect()
}

fn epoch(args: &Args) -> Result<u64, String> {
    args.epoch
        .ok_or_else(|| format!("{} needs --epoch <unix-seconds>", args.command))
}

/// Degrees Celsius on the command line, hundredths in the packet.
fn degrees(value: &str, flag: &str) -> Result<i16, String> {
    let parsed: f32 = value
        .parse()
        .map_err(|_| format!("{flag}: {value:?} is not a temperature"))?;
    let hundredths = (parsed * 100.0).round();
    if !(-32_768.0..=32_767.0).contains(&hundredths) {
        return Err(format!("{flag}: {value} is out of range"));
    }
    Ok(hundredths as i16)
}

/// Minutes since midnight, or one of the service's negative sentinels.
fn count(value: &str, flag: &str) -> Result<i16, String> {
    value
        .parse()
        .map_err(|_| format!("{flag}: {value:?} is not a number of minutes"))
}

/// `min,max,icon;min,max,icon` in whole degrees Celsius.
fn forecast(value: &str) -> Result<Vec<proto::ForecastDay>, String> {
    let mut days = Vec::new();
    for entry in value.split(';').filter(|entry| !entry.trim().is_empty()) {
        let mut fields = entry.split(',');
        let min = fields
            .next()
            .ok_or_else(|| format!("--forecast: {entry:?} is not min,max,icon"))?;
        let max = fields
            .next()
            .ok_or_else(|| format!("--forecast: {entry:?} is not min,max,icon"))?;
        let icon = fields
            .next()
            .ok_or_else(|| format!("--forecast: {entry:?} is not min,max,icon"))?;
        if fields.next().is_some() {
            return Err(format!("--forecast: {entry:?} has too many fields"));
        }
        days.push(proto::ForecastDay {
            min_c: degrees(min, "--forecast min")?,
            max_c: degrees(max, "--forecast max")?,
            icon: proto::WeatherIcon::parse(icon).map_err(|error| error.to_string())?,
        });
    }
    Ok(days)
}

fn read_string(uuid: &str) -> Result<String, String> {
    let bytes = ble::read(uuid, 128).map_err(ble_err)?;
    Ok(String::from_utf8_lossy(&bytes)
        .trim_end_matches('\0')
        .trim()
        .to_string())
}

fn read_battery() -> Result<u8, String> {
    let bytes = ble::read(proto::BATTERY_LEVEL, 1).map_err(ble_err)?;
    proto::parse_battery(&bytes).map_err(|error| error.to_string())
}

fn upsert_device_state(
    device_id: &str,
    model: &str,
    firmware: &str,
    battery: u8,
    wearer: &str,
) -> Result<(), String> {
    let now = time::monotonic_ms().map_err(time_err)?;
    upsert(
        "device.state",
        device_id,
        "device.state.v1",
        &json!({
            "device_id": device_id,
            "model": model,
            "firmware": firmware,
            "battery_percent": battery,
            "wearer": wearer,
            "observed_at_device": now,
            "received_at_host": now,
            "provenance": "ble",
            "source_spore": SOURCE_SPORE,
        }),
    )
}

fn note_signal(
    device_id: &str,
    wearer: &str,
    kind: &str,
    value: Value,
    privacy_class: &str,
    retention_class: &str,
    quality: f32,
) -> Result<(), String> {
    let now = time::monotonic_ms().map_err(time_err)?;
    let sequence = SEQUENCE.with(|cell| {
        let mut value = cell.borrow_mut();
        *value = value.saturating_add(1);
        *value
    });
    let signal_id = format!("{device_id}:{kind}:{now}:{sequence}");
    upsert(
        "signal",
        &signal_id,
        "signal.sample.v1",
        &json!({
            "signal_id": signal_id,
            "kind": kind,
            "schema_version": 1,
            "source_device": device_id,
            "source_spore": SOURCE_SPORE,
            "wearer": wearer,
            "observed_at_device": now,
            "received_at_host": now,
            "monotonic_age_ms": 0,
            "clock_uncertainty_ms": null,
            "sync_epoch": 0,
            "quality": quality,
            "provenance": "ble",
            "model_version": "pinetime-spore@0.1.0",
            "privacy_class": privacy_class,
            "retention_class": retention_class,
            "value": value,
        }),
    )
}

fn upsert(collection: &str, id: &str, schema: &str, body: &Value) -> Result<(), String> {
    let entry = Entry {
        key: Key {
            collection: collection.to_string(),
            id: id.to_string(),
        },
        schema: schema.to_string(),
        revision: 0,
        updated_at_ms: 0,
        origin: String::new(),
        deleted: false,
        body: serde_json::to_vec(body).map_err(|error| format!("record JSON: {error}"))?,
    };
    records::upsert(&entry).map_err(|error| format!("{collection}/{id}: {error:?}"))?;
    Ok(())
}

fn same(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

fn ble_err(error: ble::Error) -> String {
    format!("ble: {error:?}")
}

fn time_err(error: time::Error) -> String {
    format!("time: {error:?}")
}

fn set_error(error: &str) {
    SNAPSHOT.with(|slot| {
        *slot.borrow_mut() = json!({"ok": false, "error": error});
    });
}
