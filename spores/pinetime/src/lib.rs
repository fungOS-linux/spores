//! The PineTime Spore: BLE reads/writes in, typed records out.
//!
//! The radio transport and the lease stay in `soil-ble-host`; this module owns
//! the InfiniTime protocol mapping, the wearer identity, and the signal
//! envelope. It has no sockets, filesystem, wall clock, or ambient authority.

use std::cell::RefCell;

use infinitime as proto;
use serde_json::{json, Value};
use soil::ble::ble;
use soil::ble::types::Event;
use soil::records::records::{self, Entry, Key};
use soil::time::time;
use watch_core::{Intent, Stream, WatchProfile};
use weather_protocol as weather;

wit_bindgen::generate!({ path: "wit", world: "pinetime-spore", generate_all });

const SOURCE_SPORE: &str = "dev.fungos.pinetime";
/// The wire this Spore speaks. A second watch is a second profile, not a fork
/// of this crate.
const PROFILE: proto::Profile = proto::Profile;
const MODEL_VERSION: &str = "pinetime-spore@0.2.0";

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
    streams: Vec<Stream>,
    /// `weather --from-record`: take every weather field from the record.
    from_record: bool,
    /// `--id ID` with `--from-record`; absent means the newest record.
    record_id: Option<String>,
    /// Weather flags the caller passed. They conflict with `--from-record`.
    weather_flags: Vec<String>,
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
            streams: Stream::ALL.to_vec(),
            from_record: false,
            record_id: None,
            weather_flags: Vec::new(),
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
            // `--read-back` and `--from-record` are the flags that stand alone.
            if flag == "--read-back" {
                args.read_back = true;
                index += 1;
                continue;
            }
            if flag == "--from-record" {
                args.from_record = true;
                index += 1;
                continue;
            }
            let value = raw
                .get(index + 1)
                .ok_or_else(|| format!("{flag} needs a value"))?;
            if matches!(
                flag,
                "--epoch"
                    | "--icon"
                    | "--temp-c"
                    | "--min-c"
                    | "--max-c"
                    | "--location"
                    | "--sunrise-min"
                    | "--sunset-min"
                    | "--forecast"
            ) {
                args.weather_flags.push(flag.to_string());
            }
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
                "--streams" => args.streams = Stream::parse(value)?,
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
                "--id" => args.record_id = Some(value.clone()),
                other => return Err(format!("unknown argument {other:?}")),
            }
            index += 2;
        }
        match args.command.as_str() {
            "watch" | "notify" | "media" | "find" | "time" | "nav" | "weather" => {}
            other => {
                return Err(format!(
                    "unknown command {other:?}; expected watch, notify, media, find, time, nav, or weather"
                ))
            }
        }
        if args.from_record && args.command != "weather" {
            return Err("--from-record only applies to weather".to_string());
        }
        if args.record_id.is_some() && !args.from_record {
            return Err("--id needs --from-record".to_string());
        }
        if args.from_record {
            if let Some(flag) = args.weather_flags.first() {
                return Err(format!(
                    "{flag} conflicts with --from-record; the record carries every weather field"
                ));
            }
        }
        Ok(args)
    }
}

#[derive(Debug, Default, Clone)]
struct Counts(std::collections::BTreeMap<&'static str, u64>);

impl Counts {
    fn bump(&mut self, stream: Stream) {
        *self.0.entry(stream.as_str()).or_default() += 1;
    }

    fn json(&self) -> Value {
        json!(self.0)
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
    let device_id = watch_core::device_id(&serial, args.device.as_deref()).ok_or_else(|| {
        "PineTime exposed no DIS serial; pass --device <provisioned-id> instead of using the BLE address"
            .to_string()
    })?;

    let intent = intent_of(&args.command);
    if let Some(intent) = intent {
        if PROFILE.supports(&firmware, intent) == Some(false) {
            println!(
                "pinetime: {} is not supported by firmware {firmware:?}; writing anyway and letting the characteristic answer",
                intent.as_str()
            );
        }
    }

    let battery = read_battery()?;
    upsert_device_state(&device_id, &model, &firmware, battery, &args.wearer)?;

    let mut writes: Vec<Written> = Vec::new();
    let counts = match args.command.as_str() {
        "watch" => {
            subscribe(&args.streams)?;
            drain(
                &device_id,
                &args.wearer,
                args.hold_ms,
                &model,
                &firmware,
                &args.streams,
            )?
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
            drain(
                &device_id,
                &args.wearer,
                args.hold_ms,
                &model,
                &firmware,
                &args.streams,
            )?
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
            let (current, forecast) = if args.from_record {
                weather_from_record(args)?
            } else {
                weather_from_flags(args)?
            };
            writes.push(write_characteristic(
                proto::WEATHER_DATA,
                &current,
                args.read_back,
            )?);
            if let Some(forecast) = forecast {
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
            "profile": PROFILE.name(),
            "streams": args.streams.iter().map(|stream| stream.as_str()).collect::<Vec<_>>(),
            "signals": counts.json(),
            "writes": write_report(&writes),
            "updated_at_monotonic_ms": now,
        });
    });
    Ok(())
}

/// The characteristics a stream comes from.
fn characteristic(stream: Stream) -> &'static str {
    match stream {
        Stream::HeartRate => proto::HEART_RATE_MEASUREMENT,
        Stream::Steps => proto::STEP_COUNT,
        Stream::Motion => proto::MOTION_VALUES,
        Stream::Media => proto::MUSIC_EVENT,
        Stream::Calls => proto::NOTIFICATION_EVENT,
        Stream::Battery => proto::BATTERY_LEVEL,
    }
}

/// Subscribe only to the streams the wearer asked for. The health streams are
/// required — without them a watch session has nothing to show — while music,
/// calls and battery are optional, because older firmware may not expose them.
fn subscribe(streams: &[Stream]) -> Result<(), String> {
    for stream in streams {
        let uuid = characteristic(*stream);
        match ble::subscribe(uuid) {
            Ok(()) => {}
            Err(error) if matches!(stream, Stream::HeartRate | Stream::Steps | Stream::Motion) => {
                return Err(ble_err(error));
            }
            Err(error) => println!("pinetime: optional subscribe {uuid} failed: {error:?}"),
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
    streams: &[Stream],
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
                    streams,
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
    streams: &[Stream],
    counts: &mut Counts,
) -> Result<(), String> {
    // Battery is device state, not a body signal: it belongs on the device
    // record and never on the signal stream.
    if same(char_uuid, proto::BATTERY_LEVEL) {
        if !streams.contains(&Stream::Battery) {
            return Ok(());
        }
        let battery = proto::parse_battery(data).map_err(|error| error.to_string())?;
        upsert_device_state(device_id, model, firmware, battery, wearer)?;
        counts.bump(Stream::Battery);
        return Ok(());
    }
    let Some(signal) = PROFILE.decode(char_uuid, data)? else {
        return Ok(());
    };
    // `--streams` narrows what the wearer asked for; a notification for a
    // stream that was not selected is not an error, just not wanted.
    if !streams.contains(&signal.stream) {
        return Ok(());
    }
    note_signal(device_id, wearer, &signal)?;
    counts.bump(signal.stream);
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

/// The intent a command expresses, for the profile's firmware gate. Commands
/// that use services present since the first release have none.
fn intent_of(command: &str) -> Option<Intent> {
    match command {
        "watch" => Some(Intent::Watch),
        "notify" => Some(Intent::Notify),
        "media" => Some(Intent::Media),
        "find" => Some(Intent::Find),
        "time" => Some(Intent::Time),
        "nav" => Some(Intent::Nav),
        "weather" => Some(Intent::Weather),
        _ => None,
    }
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

/// The frames the flags path writes: current conditions, then the forecast the
/// caller typed. Every value is the caller's; nothing here reads a record.
fn weather_from_flags(args: &Args) -> Result<(Vec<u8>, Option<Vec<u8>>), String> {
    let icon = proto::WeatherIcon::parse(&args.icon).map_err(|error| error.to_string())?;
    let epoch = epoch(args)?;
    let required = |value: Option<i16>, flag: &str| {
        value
            .ok_or_else(|| format!("weather needs {flag} (or --from-record with a weather record)"))
    };
    let current = proto::current_weather_packet(
        epoch,
        required(args.temp_c, "--temp-c")?,
        required(args.min_c, "--min-c")?,
        required(args.max_c, "--max-c")?,
        &args.location,
        icon,
        args.sunrise_min,
        args.sunset_min,
    );
    let forecast =
        (!args.forecast.is_empty()).then(|| proto::forecast_packet(epoch, &args.forecast));
    Ok((current, forecast))
}

/// The frames the record path writes: the same builders, fed from the
/// `weather.current.v1` record instead of from flags.
fn weather_from_record(args: &Args) -> Result<(Vec<u8>, Option<Vec<u8>>), String> {
    let entry = match &args.record_id {
        Some(id) => records::get(&Key {
            collection: weather::COLLECTION.to_string(),
            id: id.clone(),
        })
        .map_err(|error| format!("weather.current/{id}: {error:?}"))?,
        None => newest_weather_record()?,
    };
    if entry.deleted {
        return Err(format!("weather.current/{} is deleted", entry.key.id));
    }
    if entry.schema != weather::SCHEMA {
        return Err(format!(
            "weather.current/{} is {}, not {}",
            entry.key.id,
            entry.schema,
            weather::SCHEMA
        ));
    }
    let record = weather::CurrentWeather::from_body(&entry.body)
        .map_err(|error| format!("weather.current/{}: {error}", entry.key.id))?;
    weather_frames(&record).map_err(|error| format!("weather.current/{}: {error}", entry.key.id))
}

/// The newest `weather.current.v1` record. The host's revision stamp orders
/// samples across runs; the id breaks ties so two runs agree on the choice.
fn newest_weather_record() -> Result<Entry, String> {
    const PAGE: u32 = 512;
    const MAX_PAGES: usize = 8;
    let mut newest: Option<Entry> = None;
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let page = records::scan(weather::COLLECTION, cursor.as_deref(), PAGE)
            .map_err(|error| format!("weather.current: {error:?}"))?;
        for entry in page.entries {
            if entry.deleted || entry.schema != weather::SCHEMA {
                continue;
            }
            let newer = newest.as_ref().is_none_or(|best| {
                (entry.updated_at_ms, &entry.key.id) > (best.updated_at_ms, &best.key.id)
            });
            if newer {
                newest = Some(entry);
            }
        }
        match page.next {
            Some(next) => cursor = Some(next),
            None => {
                return newest.ok_or_else(|| {
                    "no weather.current.v1 record: run `weather now` first".to_string()
                })
            }
        }
    }
    Err(format!(
        "weather.current holds more than {} records; pass --id",
        PAGE as usize * MAX_PAGES
    ))
}

/// Record to wire. The frame layout stays in `infinitime`; this only decides
/// which values the watch gets.
fn weather_frames(record: &weather::CurrentWeather) -> Result<(Vec<u8>, Option<Vec<u8>>), String> {
    let timestamp = u64::try_from(record.timestamp).map_err(|_| {
        format!(
            "timestamp {} is before 1970 and does not fit the wire",
            record.timestamp
        )
    })?;
    let icon =
        proto::WeatherIcon::parse(record.icon.as_str()).map_err(|error| error.to_string())?;
    let current = proto::current_weather_packet(
        timestamp,
        hundredths(record.temp_c, "temp_c")?,
        hundredths(record.min_c, "min_c")?,
        hundredths(record.max_c, "max_c")?,
        &record.location,
        icon,
        record.sunrise_min,
        record.sunset_min,
    );
    let forecast = if record.forecast.is_empty() {
        None
    } else {
        let days = record
            .forecast
            .iter()
            .map(|day| {
                Ok(proto::ForecastDay {
                    min_c: hundredths(day.min_c, "forecast.min_c")?,
                    max_c: hundredths(day.max_c, "forecast.max_c")?,
                    icon: proto::WeatherIcon::parse(day.icon.as_str())
                        .map_err(|error| error.to_string())?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Some(proto::forecast_packet(timestamp, &days))
    };
    Ok((current, forecast))
}

/// Degrees Celsius to the wire's hundredths, refusing a value it cannot carry.
fn hundredths(celsius: f64, field: &str) -> Result<i16, String> {
    let hundredths = (celsius * 100.0).round();
    if !(-32_768.0..=32_767.0).contains(&hundredths) {
        return Err(format!("{field}: {celsius} C does not fit the wire"));
    }
    Ok(hundredths as i16)
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
    let body = watch_core::device_state_body(
        device_id,
        wearer,
        SOURCE_SPORE,
        model,
        firmware,
        battery,
        now,
    );
    upsert("device.state", device_id, "device.state.v1", &body)
}

fn note_signal(device_id: &str, wearer: &str, signal: &watch_core::Signal) -> Result<(), String> {
    let now = time::monotonic_ms().map_err(time_err)?;
    let sequence = SEQUENCE.with(|cell| {
        let mut value = cell.borrow_mut();
        *value = value.saturating_add(1);
        *value
    });
    let body = watch_core::signal_body(
        device_id,
        wearer,
        SOURCE_SPORE,
        MODEL_VERSION,
        signal,
        now,
        sequence,
    );
    let signal_id = body["signal_id"].as_str().unwrap_or_default().to_string();
    upsert("signal", &signal_id, "signal.sample.v1", &body)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// One `weather.current.v1` body as the weather Spore writes it: Seattle,
    /// 2026-11-13T20:45 local, five days.
    fn record_body() -> serde_json::Value {
        json!({
            "schema_version": 1,
            "location": "Seattle",
            "latitude": 47.6062,
            "longitude": -122.3321,
            "timestamp": 1_794_602_700i64,
            "utc_offset_seconds": -28_800,
            "timezone": "America/Los_Angeles",
            "temp_c": 12.8,
            "min_c": 8.1,
            "max_c": 14.2,
            "icon": "rain",
            "sunrise_min": 432,
            "sunset_min": 998,
            "source": "open-meteo",
            "model_version": "weather-spore@0.1.0",
            "fetched_at_monotonic_ms": 5_000,
            "forecast": [
                {"date": "2026-11-13", "min_c": 8.1, "max_c": 14.2, "icon": "rain"},
                {"date": "2026-11-14", "min_c": 7.4, "max_c": 13.1, "icon": "clouds"},
                {"date": "2026-11-15", "min_c": 5.2, "max_c": 11.0, "icon": "clouds-rain"},
                {"date": "2026-11-16", "min_c": -1.0, "max_c": 4.5, "icon": "snow"},
                {"date": "2026-11-17", "min_c": 0.5, "max_c": 6.0, "icon": "clear"},
            ],
        })
    }

    fn record() -> weather::CurrentWeather {
        let body = serde_json::to_vec(&record_body()).expect("body");
        weather::CurrentWeather::from_body(&body).expect("record")
    }

    #[test]
    fn a_weather_record_becomes_the_53_byte_frame() {
        let (current, _) = weather_frames(&record()).expect("frames");

        let mut expected = vec![0u8, 1];
        expected.extend_from_slice(&1_794_602_700u64.to_le_bytes());
        expected.extend_from_slice(&1280i16.to_le_bytes());
        expected.extend_from_slice(&810i16.to_le_bytes());
        expected.extend_from_slice(&1420i16.to_le_bytes());
        let mut location = [0u8; 32];
        location[..7].copy_from_slice(b"Seattle");
        expected.extend_from_slice(&location);
        expected.push(5); // rain
        expected.extend_from_slice(&432i16.to_le_bytes());
        expected.extend_from_slice(&998i16.to_le_bytes());

        assert_eq!(current.len(), 53);
        assert_eq!(current, expected);
    }

    #[test]
    fn a_weather_record_becomes_the_36_byte_forecast_frame() {
        let (_, forecast) = weather_frames(&record()).expect("frames");
        let forecast = forecast.expect("the record carries a forecast");

        let mut expected = vec![1u8, 0];
        expected.extend_from_slice(&1_794_602_700u64.to_le_bytes());
        expected.push(5);
        for (min, max, icon) in [
            (810i16, 1420i16, 5u8),
            (740, 1310, 2),
            (520, 1100, 4),
            (-100, 450, 7),
            (50, 600, 0),
        ] {
            expected.extend_from_slice(&min.to_le_bytes());
            expected.extend_from_slice(&max.to_le_bytes());
            expected.push(icon);
        }

        assert_eq!(forecast.len(), 36);
        assert_eq!(forecast, expected);
    }

    #[test]
    fn a_record_without_forecast_writes_no_forecast_frame() {
        let mut body = record_body();
        body["forecast"] = json!([]);
        let record = weather::CurrentWeather::from_body(&serde_json::to_vec(&body).unwrap())
            .expect("record");
        let (current, forecast) = weather_frames(&record).expect("frames");
        assert_eq!(current.len(), 53);
        assert_eq!(forecast, None);
    }

    #[test]
    fn a_polar_record_keeps_the_absent_sentinel() {
        let mut body = record_body();
        body["sunrise_min"] = json!(-2);
        body["sunset_min"] = json!(-2);
        let record = weather::CurrentWeather::from_body(&serde_json::to_vec(&body).unwrap())
            .expect("record");
        let (current, _) = weather_frames(&record).expect("frames");
        assert_eq!(&current[49..51], &(-2i16).to_le_bytes());
        assert_eq!(&current[51..53], &(-2i16).to_le_bytes());
        assert_ne!(&current[49..51], &0i16.to_le_bytes());
    }

    #[test]
    fn the_record_and_the_wire_agree_on_the_sun_sentinels() {
        assert_eq!(weather::SUN_ABSENT, proto::SUN_ABSENT);
        assert_eq!(proto::SUN_UNKNOWN, -1);
    }

    #[test]
    fn every_record_icon_name_is_a_wire_icon() {
        for icon in weather::Icon::ALL {
            assert!(
                proto::WeatherIcon::parse(icon.as_str()).is_ok(),
                "the wire does not know {:?}",
                icon.as_str()
            );
        }
        // The one icon the provider never selects still has a wire id.
        assert_eq!(
            proto::WeatherIcon::parse("heavy-clouds"),
            Ok(proto::WeatherIcon::HeavyClouds)
        );
    }

    #[test]
    fn a_record_with_an_unknown_icon_never_reaches_the_wire() {
        // The name-to-icon table is the record crate's, so the refusal happens
        // when the body is read rather than during the frame build.
        let mut body = record_body();
        body["icon"] = json!("hail");
        let error =
            weather::CurrentWeather::from_body(&serde_json::to_vec(&body).unwrap()).unwrap_err();
        assert_eq!(error.to_string(), "unknown weather icon \"hail\"");
    }

    #[test]
    fn a_timestamp_before_1970_is_refused() {
        let mut body = record_body();
        body["timestamp"] = json!(-1);
        let record = weather::CurrentWeather::from_body(&serde_json::to_vec(&body).unwrap())
            .expect("record");
        let error = weather_frames(&record).unwrap_err();
        assert!(error.contains("before 1970"), "{error}");
    }

    #[test]
    fn a_temperature_that_does_not_fit_is_refused() {
        let mut body = record_body();
        body["temp_c"] = json!(400.0);
        let record = weather::CurrentWeather::from_body(&serde_json::to_vec(&body).unwrap())
            .expect("record");
        let error = weather_frames(&record).unwrap_err();
        assert_eq!(error, "temp_c: 400 C does not fit the wire");
    }

    fn argv(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn from_record_takes_no_weather_flags() {
        let error = Args::parse(&argv("weather --from-record --epoch 1")).unwrap_err();
        assert_eq!(
            error,
            "--epoch conflicts with --from-record; the record carries every weather field"
        );
    }

    #[test]
    fn an_id_needs_from_record() {
        let error = Args::parse(&argv("weather --id seattle")).unwrap_err();
        assert_eq!(error, "--id needs --from-record");
    }

    #[test]
    fn from_record_only_applies_to_weather() {
        let error = Args::parse(&argv("notify --from-record")).unwrap_err();
        assert_eq!(error, "--from-record only applies to weather");
    }

    #[test]
    fn the_flags_path_still_parses_its_own_values() {
        let args =
            Args::parse(&argv("weather --from-record --id seattle --read-back")).expect("args");
        assert!(args.from_record);
        assert_eq!(args.record_id.as_deref(), Some("seattle"));

        let args = Args::parse(&argv(
            "weather --epoch 1794602700 --icon rain --temp-c 12.8 --min-c 8.1 --max-c 14.2 --location Seattle",
        ))
        .expect("args");
        assert!(!args.from_record);
        let (current, forecast) = weather_from_flags(&args).expect("frames");
        assert_eq!(current.len(), 53);
        assert_eq!(forecast, None);
    }

    #[test]
    fn the_flags_path_refuses_a_missing_temperature() {
        let args = Args::parse(&argv(
            "weather --epoch 1794602700 --icon rain --temp-c 12.8 --max-c 14.2",
        ))
        .expect("args");
        let error = weather_from_flags(&args).unwrap_err();
        assert_eq!(
            error,
            "weather needs --min-c (or --from-record with a weather record)"
        );
    }
}
