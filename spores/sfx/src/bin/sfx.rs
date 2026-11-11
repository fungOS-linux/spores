//! `sfx`: the command-line face of the SFX Spore.
//!
//! This is the same document and the same capabilities as the Canvas Spore.
//! A CLI process loads the mapping, performs one command, and exits; there is
//! no long-lived output, so `stop` is not a command.
//!
//! Build for the command root with:
//!   cargo build --release --target wasm32-wasip2 \
//!     --no-default-features --features cli --bin sfx

use std::process::ExitCode;

#[cfg(target_arch = "wasm32")]
mod imp {
    use std::io::{Read, Write};
    use std::process::ExitCode;

    use serde_json::json;

    use spore_sfx::decode;
    use spore_sfx::mapping::{self, Mapping, SamplePad};
    use spore_sfx::snapshot;
    use spore_sfx::synth;

    wit_bindgen::generate!({ path: "wit", world: "cli", generate_all });

    use soil::appdata::appdata::{self, Scope, Store};
    use soil::audio::audio::{self, Format, RoomKind};
    use soil::vfs::vfs;

    const RATE: u32 = 48_000;
    const CHANNELS: u8 = 2;
    const MAX_SAMPLE_BYTES: u64 = 32 * 1024 * 1024;
    const READ_CHUNK: u32 = 256 * 1024;
    const SYNTH_PATH: &str = "sfx/synth.lua";
    const MAX_SYNTH_BYTES: u64 = 256 * 1024;

    struct Context {
        mapping: Mapping,
        store: Option<Store>,
        warnings: Vec<String>,
    }

    impl Context {
        fn load() -> Self {
            let mut warnings = Vec::new();
            let store = match appdata::open(Scope::Synced) {
                Ok(store) => Some(store),
                Err(_) => match appdata::open(Scope::Local) {
                    Ok(store) => Some(store),
                    Err(error) => {
                        warnings.push(format!("appdata unavailable: {error:?}"));
                        None
                    }
                },
            };
            let mut mapping = mapping::default_mapping();
            if let Some(store) = &store {
                match store.get(mapping::MAPPING_KEY) {
                    Ok(Some(bytes)) => match mapping::parse(&bytes) {
                        Ok(stored) => mapping = stored,
                        Err(error) => warnings.push(format!("mapping: {error}; using defaults")),
                    },
                    Ok(None) => {}
                    Err(error) => warnings.push(format!("appdata read: {error:?}")),
                }
            }
            if let Ok(entry) = vfs::stat(SYNTH_PATH) {
                if entry.kind == vfs::Kind::File && entry.size <= MAX_SYNTH_BYTES {
                    match vfs::read(SYNTH_PATH, 0, entry.size as u32) {
                        Ok(chunk) => match std::str::from_utf8(&chunk.data) {
                            Ok(source) => {
                                if let Err(error) = spore_sfx::synth::install(source) {
                                    warnings.push(format!("synth: {error}; using defaults"));
                                }
                            }
                            Err(_) => warnings.push("synth: not UTF-8; using defaults".into()),
                        },
                        Err(error) => warnings.push(format!("synth read: {error:?}")),
                    }
                }
            }
            Self {
                mapping,
                store,
                warnings,
            }
        }

        fn save(&mut self) -> Result<(), String> {
            let bytes = mapping::to_bytes(&self.mapping)?;
            let store = self
                .store
                .as_ref()
                .ok_or("appdata is unavailable; the change is only in memory")?;
            store
                .set(mapping::MAPPING_KEY, &bytes)
                .map_err(|error| format!("appdata write: {error:?}"))?;
            Ok(())
        }
    }

    pub fn main() -> ExitCode {
        match run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("sfx: {error}");
                ExitCode::FAILURE
            }
        }
    }

    fn run() -> Result<(), String> {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let mut rest = Vec::new();
        let mut room = None;
        let mut json_out = false;
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--room" => {
                    room = Some(args.get(index + 1).cloned().ok_or("--room needs a name")?);
                    index += 2;
                }
                "--json" => {
                    json_out = true;
                    index += 1;
                }
                "-h" | "--help" => {
                    print_help();
                    return Ok(());
                }
                value if value.starts_with('-') => {
                    return Err(format!("unknown option {value}"));
                }
                _ => {
                    rest.extend_from_slice(&args[index..]);
                    break;
                }
            }
        }

        let Some(command) = rest.first().map(String::as_str) else {
            print_help();
            return Ok(());
        };
        let mut context = Context::load();
        for warning in &context.warnings {
            eprintln!("sfx: warning: {warning}");
        }

        match command {
            "list" => list(&context, json_out),
            "rooms" => rooms(json_out),
            "mapping" => {
                let value = serde_json::to_value(&context.mapping)
                    .map_err(|error| format!("mapping JSON: {error}"))?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&value)
                        .map_err(|error| format!("mapping JSON: {error}"))?
                );
                Ok(())
            }
            "load" | "import" => {
                let mut bytes = Vec::new();
                std::io::stdin()
                    .read_to_end(&mut bytes)
                    .map_err(|error| format!("read mapping: {error}"))?;
                let mapping = mapping::parse(&bytes)?;
                context.mapping = mapping;
                context.save()?;
                println!("mapping loaded");
                Ok(())
            }
            "play" => {
                let pad = rest.get(1).ok_or("play needs a PAD")?;
                play_pad(&context.mapping, pad, room.as_deref(), 1.0)
            }
            "render" => {
                // A one-shot WAV on stdout. The browser sheet uses this to
                // pre-render pads once, then mixes them in Web Audio instead
                // of opening a CoreAudio stream for every press.
                let pad = rest.get(1).ok_or("render needs a PAD")?;
                let mono = materialise(&context.mapping, pad)?;
                let bytes = wav(&mono);
                std::io::stdout()
                    .write_all(&bytes)
                    .map_err(|error| format!("write WAV: {error}"))?;
                Ok(())
            }
            "event" | "test" => {
                let kind = rest.get(1).ok_or("event needs a KIND")?;
                play_event(&context.mapping, kind, room.as_deref())
            }
            "selftest" => {
                for pad in ["blip", "ping", "chime"] {
                    play_pad(&context.mapping, pad, room.as_deref(), 1.0)?;
                }
                Ok(())
            }
            "bind" => {
                let kind = rest.get(1).ok_or("bind needs KIND PAD")?;
                let pad = rest.get(2).ok_or("bind needs KIND PAD")?;
                context.mapping.bind(kind, pad)?;
                context.save()?;
                println!("bound {kind} -> {pad}");
                Ok(())
            }
            "unbind" => {
                let kind = rest.get(1).ok_or("unbind needs KIND")?;
                if !context.mapping.remove_rule(kind) {
                    return Err(format!("no rule for {kind:?}"));
                }
                context.save()?;
                println!("removed {kind}");
                Ok(())
            }
            "rule-room" => {
                let kind = rest.get(1).ok_or("rule-room needs KIND ROOM|none")?;
                let value = rest.get(2).ok_or("rule-room needs KIND ROOM|none")?;
                let room = if value == "none" {
                    None
                } else {
                    if !mapping::valid_room(value) {
                        return Err(format!("room {value:?} is not a room name"));
                    }
                    Some(value.clone())
                };
                let rule = context
                    .mapping
                    .rules
                    .iter_mut()
                    .find(|rule| rule.kind == *kind)
                    .ok_or_else(|| format!("no rule for {kind:?}"))?;
                rule.room = room;
                context.save()?;
                println!("rule {kind} room: {value}");
                Ok(())
            }
            "rule-gain" => {
                let kind = rest.get(1).ok_or("rule-gain needs KIND NUMBER")?;
                let value = rest.get(2).ok_or("rule-gain needs KIND NUMBER")?;
                let gain: f32 = value.parse().map_err(|_| "gain is not a number")?;
                if !gain.is_finite() || !(0.0..=mapping::MAX_GAIN).contains(&gain) {
                    return Err(format!("gain {gain} is outside 0..={}", mapping::MAX_GAIN));
                }
                let rule = context
                    .mapping
                    .rules
                    .iter_mut()
                    .find(|rule| rule.kind == *kind)
                    .ok_or_else(|| format!("no rule for {kind:?}"))?;
                rule.gain = gain;
                context.save()?;
                println!("rule {kind} gain: {gain}");
                Ok(())
            }
            "sample" => sample(&mut context, &rest[1..]),
            "validate" => {
                let issues = context.mapping.validate();
                if issues.is_empty() {
                    println!("mapping is valid");
                    return Ok(());
                }
                for issue in &issues {
                    println!(
                        "{}: {}: {}",
                        issue.level.as_str(),
                        issue.subject,
                        issue.message
                    );
                }
                if issues
                    .iter()
                    .any(|issue| issue.level == mapping::Level::Error)
                {
                    return Err("mapping has errors".into());
                }
                Ok(())
            }
            "reset" => {
                context.mapping = mapping::default_mapping();
                context.save()?;
                println!("mapping reset to defaults");
                Ok(())
            }
            "enable" | "disable" => {
                let kind = rest.get(1).ok_or("enable/disable needs KIND")?;
                context.mapping.set_enabled(kind, command == "enable")?;
                context.save()?;
                println!("{command}d {kind}");
                Ok(())
            }
            "room" => {
                let value = rest.get(1).ok_or("room needs a NAME or 'none'")?;
                if value == "none" {
                    context.mapping.room = None;
                } else {
                    if !mapping::valid_room(value) {
                        return Err(format!("room {value:?} is not a room name"));
                    }
                    context.mapping.room = Some(value.clone());
                }
                context.save()?;
                println!("default room: {}", value);
                Ok(())
            }
            "gain" => {
                let value = rest.get(1).ok_or("gain needs a number")?;
                let gain: f32 = value.parse().map_err(|_| "gain is not a number")?;
                if !gain.is_finite() || !(0.0..=mapping::MAX_GAIN).contains(&gain) {
                    return Err(format!("gain {gain} is outside 0..={}", mapping::MAX_GAIN));
                }
                context.mapping.gain = gain;
                context.save()?;
                println!("mapping gain: {gain}");
                Ok(())
            }
            other => Err(format!("unknown command {other:?}")),
        }
    }

    fn print_help() {
        println!(
            "sfx - the event-driven soundboard\n\n\
             usage: sfx [--room NAME] [--json] COMMAND [ARGS]\n\n\
             commands:\n\
               list             pads and event rules\n\
               rooms            rooms this host may send to\n\
               mapping          print the stored mapping JSON\n\
               load             read mapping JSON from stdin and save it\n\
               play PAD         play one pad\n\
               render PAD       write the pad as a mono WAV on stdout\n\
               event KIND       play the pad mapped to an event kind\n\
               test KIND        alias for event\n\
               selftest         play blip, ping, chime\n\
               bind KIND PAD    bind an event kind to a pad\n\
               unbind KIND      remove an event rule\n\
               rule-room KIND ROOM|none  per-rule room override\n\
               rule-gain KIND NUMBER     per-rule trim\n\
               sample add PAD PATH [--label L] [--gain N]\n\
               sample remove PAD          remove a sample pad\n\
               validate          check the stored mapping\n\
               reset             restore the default mapping\n\
               enable KIND      enable a rule\n\
               disable KIND     mute a rule\n\
               room NAME|none   set the mapping default room\n\
               gain NUMBER      set the mapping gain (0..=2)\n\n\
             The mapping is stored in appdata under sfx.mapping@1.\n\
             Pads come from the built-in synth and from `soil:vfs` samples.\n\
             EVENT is the Unibus kind, e.g. chat.message.v1."
        );
    }

    fn list(context: &Context, json_out: bool) -> Result<(), String> {
        if json_out {
            let pads: Vec<_> = synth::BUILTINS
                .iter()
                .map(|pad| {
                    json!({
                        "id": pad.id,
                        "label": pad.label,
                        "category": pad.category,
                        "source": "builtin",
                        "duration_ms": synth::duration_ms(pad.id, RATE),
                        "available": true,
                    })
                })
                .chain(context.mapping.samples.iter().map(|sample| {
                    let reason = snapshot::sample_reason(&context.mapping, sample);
                    json!({
                        "id": sample.pad,
                        "label": sample.label(),
                        "category": sample.category,
                        "source": "sample",
                        "path": sample.path,
                        "content_id": sample.content_id,
                        "available": reason.is_none(),
                        "reason": reason,
                    })
                }))
                .collect();
            let rules: Vec<_> = context
                .mapping
                .rules
                .iter()
                .map(|rule| {
                    json!({
                        "kind": rule.kind,
                        "pad": rule.pad,
                        "enabled": rule.enabled,
                        "gain": rule.gain,
                        "room": rule.room,
                    })
                })
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "gain": context.mapping.gain,
                    "room": context.mapping.room,
                    "pads": pads,
                    "rules": rules,
                }))
                .map_err(|error| format!("JSON: {error}"))?
            );
            return Ok(());
        }

        println!("pads");
        for pad in synth::BUILTINS {
            let duration = synth::duration_ms(pad.id, RATE).unwrap_or(0);
            println!(
                "  {:<8} {:<12} {:<6} builtin  {:>4} ms",
                pad.id, pad.label, pad.category, duration
            );
        }
        for sample in &context.mapping.samples {
            let reason = snapshot::sample_reason(&context.mapping, sample);
            match reason {
                Some(reason) => println!(
                    "  {:<8} {:<12} {:<6} sample   unavailable: {reason}",
                    sample.pad,
                    sample.label(),
                    sample.category
                ),
                None => println!(
                    "  {:<8} {:<12} {:<6} sample   {}",
                    sample.pad,
                    sample.label(),
                    sample.category,
                    sample.path
                ),
            }
        }
        println!("rules");
        for rule in &context.mapping.rules {
            let room = rule
                .room
                .as_deref()
                .or(context.mapping.room.as_deref())
                .unwrap_or("host-default");
            println!(
                "  {:<22} -> {:<8} {:<7} gain {:.2}  room {}",
                rule.kind,
                rule.pad,
                if rule.enabled { "enabled" } else { "muted" },
                rule.gain,
                room
            );
        }
        Ok(())
    }

    fn sample(context: &mut Context, args: &[String]) -> Result<(), String> {
        let Some(command) = args.first().map(String::as_str) else {
            return Err("sample needs add PAD PATH or remove PAD".into());
        };
        match command {
            "add" => {
                let pad = args.get(1).ok_or("sample add needs PAD PATH")?;
                let path = args.get(2).ok_or("sample add needs PAD PATH")?;
                let mut label = String::new();
                let mut category = "sample".to_string();
                let mut gain = 1.0f32;
                let mut index = 3;
                while index < args.len() {
                    match args[index].as_str() {
                        "--label" => {
                            label = args.get(index + 1).cloned().ok_or("--label")?;
                            index += 2;
                        }
                        "--category" => {
                            category = args.get(index + 1).cloned().ok_or("--category")?;
                            index += 2;
                        }
                        "--gain" => {
                            gain = args
                                .get(index + 1)
                                .ok_or("--gain")?
                                .parse()
                                .map_err(|_| "--gain is not a number")?;
                            index += 2;
                        }
                        other => return Err(format!("unknown sample option {other}")),
                    }
                }
                if !gain.is_finite() || !(0.0..=mapping::MAX_GAIN).contains(&gain) {
                    return Err(format!("gain {gain} is outside 0..={}", mapping::MAX_GAIN));
                }
                let entry =
                    vfs::stat(path).map_err(|error| format!("vfs stat {path}: {error:?}"))?;
                if entry.kind != vfs::Kind::File {
                    return Err(format!("{path} is not a file"));
                }
                if entry.size > MAX_SAMPLE_BYTES {
                    return Err(format!("{path} is larger than the sample limit"));
                }
                let sample = SamplePad {
                    pad: pad.clone(),
                    label,
                    path: path.clone(),
                    content_id: entry.content_id,
                    category,
                    gain,
                };
                if context.mapping.sample_for(&sample.pad).is_some() {
                    return Err(format!("sample pad {:?} already exists", sample.pad));
                }
                if let Some(reason) = snapshot::sample_reason(&context.mapping, &sample) {
                    return Err(reason);
                }
                context.mapping.samples.push(sample);
                context.save()?;
                println!("added sample {pad} -> {path}");
                Ok(())
            }
            "remove" => {
                let pad = args.get(1).ok_or("sample remove needs PAD")?;
                let before = context.mapping.samples.len();
                context.mapping.samples.retain(|sample| sample.pad != *pad);
                if context.mapping.samples.len() == before {
                    return Err(format!("no sample pad {pad:?}"));
                }
                context.save()?;
                println!("removed sample {pad}");
                Ok(())
            }
            other => Err(format!("unknown sample command {other}")),
        }
    }

    fn rooms(json_out: bool) -> Result<(), String> {
        let rooms = audio::rooms().map_err(|error| format!("rooms: {error:?}"))?;
        let current = audio::current_room()
            .map_err(|error| format!("current room: {error:?}"))?
            .map(|room| room.name);
        if json_out {
            let values: Vec<_> = rooms
                .iter()
                .map(|room| {
                    json!({
                        "name": room.name,
                        "label": room.label,
                        "kind": match room.kind {
                            RoomKind::Local => "local",
                            RoomKind::Remote => "remote",
                        },
                        "live": room.live,
                        "principal": room.principal,
                        "current": current.as_deref() == Some(room.name.as_str()),
                    })
                })
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({"rooms": values, "current": current}))
                    .map_err(|error| format!("JSON: {error}"))?
            );
            return Ok(());
        }
        println!("rooms");
        for room in rooms {
            println!(
                "  {:<12} {:<16} {:<6} live={:<3} current={}",
                room.name,
                room.label,
                match room.kind {
                    RoomKind::Local => "local",
                    RoomKind::Remote => "remote",
                },
                if room.live { "yes" } else { "no" },
                if current.as_deref() == Some(room.name.as_str()) {
                    "yes"
                } else {
                    "no"
                }
            );
        }
        Ok(())
    }

    fn play_event(mapping: &Mapping, kind: &str, room: Option<&str>) -> Result<(), String> {
        let rule = mapping
            .rule_for(kind)
            .ok_or_else(|| format!("no rule for {kind:?}"))?;
        if !rule.enabled {
            return Err(format!("{kind} is muted"));
        }
        let room = room
            .map(str::to_string)
            .or_else(|| mapping.room_for(Some(rule)).map(str::to_string));
        play_pad(mapping, &rule.pad, room.as_deref(), rule.gain)
    }

    fn play_pad(
        mapping: &Mapping,
        pad: &str,
        room: Option<&str>,
        gain_scale: f32,
    ) -> Result<(), String> {
        let mono = materialise(mapping, pad)?;
        if mono.is_empty() {
            return Err(format!("pad {pad:?} is empty"));
        }
        let sample_gain = mapping
            .sample_for(pad)
            .map(|sample| sample.gain)
            .unwrap_or(1.0);
        let gain = (mapping.gain * sample_gain * gain_scale).clamp(0.0, mapping::MAX_GAIN);
        let frames = decode::interleave(&mono, CHANNELS, gain);
        let output = audio::open(
            Format {
                sample_rate: RATE,
                channels: CHANNELS,
            },
            room,
        )
        .map_err(|error| format!("audio open {room:?}: {error:?}"))?;
        let mut offset = 0;
        let channels = usize::from(CHANNELS);
        while offset < frames.len() {
            let accepted = output
                .write(&frames[offset..])
                .map_err(|error| format!("audio write: {error:?}"))?
                as usize;
            if accepted == 0 {
                output
                    .drain(50)
                    .map_err(|error| format!("audio drain: {error:?}"))?;
            } else {
                offset += accepted * channels;
            }
        }
        output
            .drain(5_000)
            .map_err(|error| format!("audio drain: {error:?}"))?;
        let status = output
            .status()
            .map_err(|error| format!("audio status: {error:?}"))?;
        output
            .close()
            .map_err(|error| format!("audio close: {error:?}"))?;
        println!(
            "played {pad} -> {} ({} Hz, {} ch)",
            status.room, status.format.sample_rate, status.format.channels
        );
        Ok(())
    }

    /// Mono 16-bit PCM WAV. The browser sheet sends this to Web Audio; the
    /// guest's playback path still uses the live `soil:audio` sink.
    fn wav(samples: &[f32]) -> Vec<u8> {
        let data_len = (samples.len() * 2) as u32;
        let mut out = Vec::with_capacity(44 + data_len as usize);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&RATE.to_le_bytes());
        out.extend_from_slice(&(RATE * 2).to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        for sample in samples {
            let value = (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
            out.extend_from_slice(&value.to_le_bytes());
        }
        out
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

    fn load_sample(sample: &SamplePad) -> Result<Vec<f32>, String> {
        let entry = vfs::stat(&sample.path)
            .map_err(|error| format!("vfs stat {}: {error:?}", sample.path))?;
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
}

#[cfg(target_arch = "wasm32")]
fn main() -> ExitCode {
    imp::main()
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> ExitCode {
    eprintln!("sfx: build this command for wasm32-wasip2 (see spores/sfx/README.md)");
    ExitCode::FAILURE
}
