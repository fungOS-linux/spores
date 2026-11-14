//! JARVIS telemetry Spore: Records in, presence/attention out.
//!
//! It never touches a radio or a device protocol. Device Spores write typed
//! signals; this Spore applies the wearer/freshness/fusion policy and publishes
//! latest-value frames for JARVIS to consume.

use std::cell::RefCell;

use jarvis_telemetry_core as core;
use serde_json::{json, Value};
use soil::records::records::{self, Entry, Key};
use soil::time::time;
use soil::unibus::bus;

wit_bindgen::generate!({ path: "wit", world: "jarvis-telemetry-spore", generate_all });

thread_local! {
    static SNAPSHOT: RefCell<Value> = RefCell::new(Value::Null);
}

struct Component;
export!(Component);

#[derive(Debug, Clone)]
struct Args {
    command: String,
    wearer: String,
    iterations: u32,
    interval_ms: u64,
    synthetic: bool,
    now: Option<u64>,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            command: "once".to_string(),
            wearer: "owner".to_string(),
            iterations: 1,
            interval_ms: 1_000,
            synthetic: false,
            now: None,
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
            if flag == "--synthetic" {
                args.synthetic = true;
                index += 1;
                continue;
            }
            let value = raw
                .get(index + 1)
                .ok_or_else(|| format!("{flag} needs a value"))?;
            match flag {
                "--wearer" => args.wearer = value.clone(),
                "--iterations" => {
                    args.iterations = value
                        .parse()
                        .map_err(|_| format!("--iterations: {value:?} is not a number"))?
                }
                "--interval-ms" => {
                    args.interval_ms = value
                        .parse()
                        .map_err(|_| format!("--interval-ms: {value:?} is not a number"))?
                }
                "--now" => {
                    args.now = Some(
                        value
                            .parse()
                            .map_err(|_| format!("--now: {value:?} is not a number"))?,
                    )
                }
                other => return Err(format!("unknown argument {other:?}")),
            }
            index += 2;
        }
        match args.command.as_str() {
            "once" | "seed" | "loop" => Ok(args),
            other => Err(format!(
                "unknown command {other:?}; expected once, seed, or loop"
            )),
        }
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
    let now = args.now.unwrap_or(time::monotonic_ms().map_err(time_err)?);
    match args.command.as_str() {
        "seed" => {
            let count = seed(now)?;
            set_snapshot(json!({
                "ok": true,
                "command": "seed",
                "records": count,
                "now": now,
            }));
            Ok(())
        }
        "once" => run_once(now, &args),
        "loop" => run_loop(&args),
        other => Err(format!("unknown command {other:?}")),
    }
}

fn run_once(now: u64, args: &Args) -> Result<(), String> {
    let signals = read_signals()?;
    let fused = core::fuse(
        &signals,
        &args.wearer,
        now,
        core::Policy::default(),
        args.synthetic,
    );
    publish(&fused)?;
    set_snapshot(json!({
        "ok": true,
        "command": "once",
        "presence": fused.presence,
        "attention": fused.attention,
        "evidence": fused.evidence,
    }));
    Ok(())
}

fn run_loop(args: &Args) -> Result<(), String> {
    let iterations = args.iterations.max(1);
    let mut last = None;
    for iteration in 0..iterations {
        let now = args.now.unwrap_or(time::monotonic_ms().map_err(time_err)?);
        let signals = read_signals()?;
        let fused = core::fuse(
            &signals,
            &args.wearer,
            now,
            core::Policy::default(),
            args.synthetic,
        );
        publish(&fused)?;
        last = Some(fused);
        if iteration + 1 < iterations {
            let wait = args.interval_ms.min(u64::from(u32::MAX)) as u32;
            let _ = time::tick(wait).map_err(time_err)?;
        }
    }
    if let Some(fused) = last {
        set_snapshot(json!({
            "ok": true,
            "command": "loop",
            "iterations": iterations,
            "presence": fused.presence,
            "attention": fused.attention,
            "evidence": fused.evidence,
        }));
    }
    Ok(())
}

fn seed(now: u64) -> Result<u64, String> {
    let records = core::seed(now);
    let mut count = 0;
    for record in &records {
        upsert(record)?;
        count += 1;
    }
    Ok(count)
}

fn read_signals() -> Result<Vec<core::Signal>, String> {
    let mut signals = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..20 {
        let page = records::scan("signal", cursor.as_deref(), 200)
            .map_err(|error| format!("scan signal: {error:?}"))?;
        for entry in page.entries {
            let Ok(body) = serde_json::from_slice::<Value>(&entry.body) else {
                continue;
            };
            if let Some(signal) = core::Signal::from_record(&body) {
                signals.push(signal);
            }
        }
        match page.next {
            Some(next) => cursor = Some(next),
            None => return Ok(signals),
        }
    }
    Err("signal scan exceeded 20 pages".to_string())
}

fn publish(fused: &core::Fused) -> Result<(), String> {
    for (kind, body) in [
        (core::PRESENCE_KIND, &fused.presence),
        (core::ATTENTION_KIND, &fused.attention),
    ] {
        let bytes = core::bus_payload(kind, body)
            .map_err(|error| format!("bus payload {kind}: {error:?}"))?;
        bus::publish(kind, &bytes, None, 60_000)
            .map_err(|error| format!("publish {kind}: {error:?}"))?;
    }
    Ok(())
}

fn upsert(record: &core::SeedRecord) -> Result<(), String> {
    let entry = Entry {
        key: Key {
            collection: record.collection.to_string(),
            id: record.id.clone(),
        },
        schema: record.schema.to_string(),
        revision: 0,
        updated_at_ms: 0,
        origin: String::new(),
        deleted: false,
        body: serde_json::to_vec(&record.body)
            .map_err(|error| format!("seed body JSON: {error}"))?,
    };
    records::upsert(&entry)
        .map_err(|error| format!("{}/{}: {error:?}", record.collection, record.id))?;
    Ok(())
}

fn set_snapshot(value: Value) {
    SNAPSHOT.with(|slot| *slot.borrow_mut() = value);
}

fn set_error(error: &str) {
    set_snapshot(json!({"ok": false, "error": error}));
}

fn time_err(error: time::Error) -> String {
    format!("time: {error:?}")
}
