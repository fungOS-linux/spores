//! Vitals Spore: health fusion over the records device Spores write.
//!
//! No radio, no UI, no ambient clock. A native GPUI host can read the latest
//! record or subscribe to the `vitals.summary.v1` bus frame and own the pixels.

use std::cell::RefCell;

use serde_json::{json, Value};
use soil::records::records::{self, Entry, Key};
use soil::time::time;
use soil::unibus::bus;
use vitals_core as core;

wit_bindgen::generate!({ path: "wit", world: "vitals-spore", generate_all });

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
    now: Option<u64>,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            command: "once".to_string(),
            wearer: "owner".to_string(),
            iterations: 1,
            interval_ms: 1_000,
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
            "seed" | "once" | "loop" => Ok(args),
            other => Err(format!(
                "unknown command {other:?}; expected seed, once, or loop"
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
        "once" => {
            once(now, &args)?;
            Ok(())
        }
        "loop" => run_loop(&args),
        other => Err(format!("unknown command {other:?}")),
    }
}

fn run_loop(args: &Args) -> Result<(), String> {
    let iterations = args.iterations.max(1);
    let mut last = None;
    for iteration in 0..iterations {
        let now = args.now.unwrap_or(time::monotonic_ms().map_err(time_err)?);
        let summary = once(now, args)?;
        last = Some(summary);
        if iteration + 1 < iterations {
            let wait = args.interval_ms.min(u64::from(u32::MAX)) as u32;
            let _ = time::tick(wait).map_err(time_err)?;
        }
    }
    if let Some(summary) = last {
        set_snapshot(json!({
            "ok": true,
            "command": "loop",
            "iterations": iterations,
            "summary": summary,
        }));
    }
    Ok(())
}

fn seed(now: u64) -> Result<u64, String> {
    let records = core::seed(now);
    let mut count = 0;
    for record in &records {
        upsert_record(record)?;
        count += 1;
    }
    Ok(count)
}

fn once(now: u64, args: &Args) -> Result<Value, String> {
    let signals = read_signals()?;
    let vitals = core::fuse(&signals, &args.wearer, now, core::Policy::default());
    let summary = vitals.to_json();

    let summary_id = format!("{}:{now}", args.wearer);
    upsert("vitals.summary", &summary_id, core::SCHEMA, &summary)?;
    upsert("vitals.latest", &args.wearer, core::SCHEMA, &summary)?;
    publish_summary(&summary)?;

    set_snapshot(json!({
        "ok": true,
        "command": "once",
        "summary": summary,
    }));
    Ok(summary)
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

fn publish_summary(summary: &Value) -> Result<(), String> {
    let body = serde_json::to_vec(summary).map_err(|error| format!("summary JSON: {error}"))?;
    bus::publish(core::SCHEMA, &body, None, 60_000)
        .map_err(|error| format!("publish {}: {error:?}", core::SCHEMA))?;
    Ok(())
}

fn upsert_record(record: &core::SeedRecord) -> Result<(), String> {
    upsert(record.collection, &record.id, record.schema, &record.body)
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

fn set_snapshot(value: Value) {
    SNAPSHOT.with(|slot| *slot.borrow_mut() = value);
}

fn set_error(error: &str) {
    set_snapshot(json!({"ok": false, "error": error}));
}

fn time_err(error: time::Error) -> String {
    format!("time: {error:?}")
}
