//! `pulse` — the host-side half of the Pulse console.
//!
//! The Spore's view cannot read a file or a clock, so the snapshot it paints is
//! produced here (or by the host) and the edits it emits are applied here. One
//! reducer serves both: the CLI and the view send the same `Intent` JSON.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use pulse::{Applied, Intent, Schedule, Store, Tab, future_text, iso};

const USAGE: &str = "\
usage: pulse --records DIR <command> [options]

commands:
  list                     one row per cron: health, schedule, next fire, target
  hooks                    one row per Confluence webhook: health, 24h counts
  snapshot                 emit the Lua snapshot ui/console.lua renders
  apply --intent JSON      apply one intent and print what changed
  set ID --schedule S      create or update a cron
      [--name N] [--target T] [--enable|--disable]
  toggle ID                pause or resume a cron
  delete ID                tombstone a cron
  validate SCHEDULE        describe a schedule and its next three fires

options:
  --records DIR            the records directory (required)
  --now MS                 the instant to render at (default: now)
  --tab crons|ingress      snapshot tab (default: crons)
  --select ID              selected cron
  --hook SLUG              selected webhook
  --json                   print JSON instead of Lua (snapshot)
  --out PATH               write the snapshot to a file instead of stdout";

fn main() -> ExitCode {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    match run(raw) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("pulse: {error}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Default)]
struct Args {
    positionals: Vec<String>,
    flags: BTreeMap<String, Vec<String>>,
}

impl Args {
    fn flag(&self, name: &str) -> Option<&str> {
        self.flags
            .get(name)
            .and_then(|values| values.first())
            .map(String::as_str)
    }

    fn has(&self, name: &str) -> bool {
        self.flags.contains_key(name)
    }

    fn required(&self, name: &str) -> Result<&str, String> {
        self.flag(name)
            .ok_or_else(|| format!("--{name} is required"))
    }

    fn records(&self) -> Result<&Path, String> {
        self.required("records").map(Path::new)
    }

    fn now_ms(&self) -> Result<i64, String> {
        match self.flag("now") {
            Some(value) => value
                .parse::<i64>()
                .map_err(|_| format!("--now {value:?} is not milliseconds")),
            None => {
                let elapsed = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|error| format!("clock: {error}"))?;
                Ok(elapsed.as_millis() as i64)
            }
        }
    }
}

fn parse_args(raw: Vec<String>) -> Result<Args, String> {
    let mut args = Args::default();
    let mut iter = raw.into_iter();
    while let Some(arg) = iter.next() {
        if let Some(name) = arg.strip_prefix("--") {
            if matches!(name, "json" | "enable" | "disable" | "help") {
                args.flags
                    .entry(name.to_owned())
                    .or_default()
                    .push(String::new());
            } else {
                let value = iter
                    .next()
                    .ok_or_else(|| format!("--{name} needs a value"))?;
                args.flags.entry(name.to_owned()).or_default().push(value);
            }
        } else {
            args.positionals.push(arg);
        }
    }
    Ok(args)
}

fn run(raw: Vec<String>) -> Result<(), String> {
    let args = parse_args(raw)?;
    let command = args
        .positionals
        .first()
        .map(String::as_str)
        .unwrap_or("help");
    match command {
        "help" | "-h" | "--help" => {
            println!("{USAGE}");
            Ok(())
        }
        "list" => list(&args),
        "hooks" => hooks(&args),
        "snapshot" => snapshot(&args),
        "apply" => apply(&args),
        "set" => set(&args),
        "toggle" => toggle(&args),
        "delete" => delete(&args),
        "validate" => validate(&args),
        other => Err(format!("unknown command {other:?}\n\n{USAGE}")),
    }
}

fn store(args: &Args) -> Result<Store, String> {
    Store::load(args.records()?)
}

fn list(args: &Args) -> Result<(), String> {
    let store = store(args)?;
    let now = args.now_ms()?;
    let snapshot = store.snapshot(now, Tab::Crons, None, None);
    if args.has("json") {
        println!(
            "{}",
            serde_json::to_string_pretty(&snapshot)
                .map_err(|error| format!("encode snapshot: {error}"))?
        );
        return Ok(());
    }
    println!(
        "{:<9} {:<22} {:<17} {:<11} TARGET",
        "HEALTH", "ID", "SCHEDULE", "NEXT"
    );
    for row in snapshot["crons"].as_array().cloned().unwrap_or_default() {
        println!(
            "{:<9} {:<22} {:<17} {:<11} {}",
            row["health_text"].as_str().unwrap_or(""),
            row["id"].as_str().unwrap_or(""),
            row["schedule"].as_str().unwrap_or(""),
            row["next_text"].as_str().unwrap_or(""),
            row["target"].as_str().unwrap_or("")
        );
    }
    let summary = &snapshot["summary"];
    println!(
        "\n{} crons · {} overdue · {} failing · {} paused · {} hooks failing",
        summary["crons"],
        summary["overdue"],
        summary["failing"],
        summary["paused"],
        summary["hooks_failing"]
    );
    Ok(())
}

fn hooks(args: &Args) -> Result<(), String> {
    let store = store(args)?;
    let now = args.now_ms()?;
    let snapshot = store.snapshot(now, Tab::Ingress, None, None);
    if args.has("json") {
        println!(
            "{}",
            serde_json::to_string_pretty(&snapshot)
                .map_err(|error| format!("encode snapshot: {error}"))?
        );
        return Ok(());
    }
    println!(
        "{:<18} {:<24} {:<9} {:<8} {:<9} LAST",
        "HEALTH", "SLUG", "PROVIDER", "OK/24H", "BAD/24H"
    );
    for row in snapshot["hooks"].as_array().cloned().unwrap_or_default() {
        let last = if row["last"].is_null() {
            "never".to_owned()
        } else {
            format!(
                "{} {}",
                row["last"]["text"].as_str().unwrap_or(""),
                row["last"]["status"].as_str().unwrap_or("")
            )
        };
        println!(
            "{:<18} {:<24} {:<9} {:<8} {:<9} {}",
            row["health_text"].as_str().unwrap_or(""),
            row["slug"].as_str().unwrap_or(""),
            row["provider"].as_str().unwrap_or(""),
            row["accepted_24h"],
            row["rejected_24h"],
            last
        );
    }
    Ok(())
}

fn snapshot(args: &Args) -> Result<(), String> {
    let store = store(args)?;
    let now = args.now_ms()?;
    let tab = match args.flag("tab") {
        Some(value) => {
            Tab::parse(value).ok_or_else(|| "--tab must be crons or ingress".to_owned())?
        }
        None => Tab::default(),
    };
    let output = if args.has("json") {
        let mut text = serde_json::to_string_pretty(&store.snapshot(
            now,
            tab,
            args.flag("select"),
            args.flag("hook"),
        ))
        .map_err(|error| format!("encode snapshot: {error}"))?;
        text.push('\n');
        text
    } else {
        store.snapshot_lua(now, tab, args.flag("select"), args.flag("hook"))
    };
    match args.flag("out") {
        Some(path) => {
            std::fs::write(path, output).map_err(|error| format!("write {path}: {error}"))?;
            println!("wrote {path}");
        }
        None => print!("{output}"),
    }
    Ok(())
}

fn apply(args: &Args) -> Result<(), String> {
    let path = args.records()?;
    let now = args.now_ms()?;
    let intent: Intent = serde_json::from_str(args.required("intent")?)
        .map_err(|error| format!("--intent: {error}"))?;
    let mut store = Store::load(path)?;
    let result = store.apply(&intent, now);
    report(path, &result)?;
    if !result.message.is_empty() {
        println!("{}", result.message);
    }
    Ok(())
}

fn set(args: &Args) -> Result<(), String> {
    let path = args.records()?;
    let now = args.now_ms()?;
    let id = args
        .positionals
        .get(1)
        .ok_or_else(|| "usage: pulse --records DIR set ID --schedule S".to_owned())?;
    let intent = Intent::Save {
        id: id.clone(),
        name: args.flag("name").map(str::to_owned),
        schedule: args.required("schedule")?.to_owned(),
        target: args.flag("target").map(str::to_owned),
        enabled: if args.has("disable") {
            Some(false)
        } else if args.has("enable") {
            Some(true)
        } else {
            None
        },
    };
    let mut store = Store::load(path)?;
    let result = store.apply(&intent, now);
    report(path, &result)?;
    println!("{}", result.message);
    Ok(())
}

fn toggle(args: &Args) -> Result<(), String> {
    let path = args.records()?;
    let now = args.now_ms()?;
    let id = args
        .positionals
        .get(1)
        .ok_or_else(|| "usage: pulse --records DIR toggle ID".to_owned())?;
    let mut store = Store::load(path)?;
    let result = store.apply(&Intent::Toggle { id: id.clone() }, now);
    report(path, &result)?;
    println!("{}", result.message);
    Ok(())
}

fn delete(args: &Args) -> Result<(), String> {
    let path = args.records()?;
    let now = args.now_ms()?;
    let id = args
        .positionals
        .get(1)
        .ok_or_else(|| "usage: pulse --records DIR delete ID".to_owned())?;
    let mut store = Store::load(path)?;
    let result = store.apply(&Intent::Delete { id: id.clone() }, now);
    report(path, &result)?;
    println!("{}", result.message);
    Ok(())
}

fn validate(args: &Args) -> Result<(), String> {
    let text = args
        .positionals
        .get(1)
        .ok_or_else(|| "usage: pulse validate 'SCHEDULE'".to_owned())?;
    let now = args.now_ms()?;
    let schedule = Schedule::parse(text).map_err(problems)?;
    println!("{}", schedule.text());
    println!("{}", schedule.describe());
    let mut at = now;
    for _ in 0..3 {
        let Some(next) = schedule.next_after(at) else {
            println!("never fires");
            return Ok(());
        };
        println!("  {}  ({})", iso(next), future_text(now, next));
        at = next;
    }
    Ok(())
}

fn report(path: &Path, result: &Applied) -> Result<(), String> {
    if !result.ok() {
        return Err(problems(result.problems.clone()));
    }
    if result.entries.is_empty() {
        return Ok(());
    }
    Store::append(path, &result.entries)
}

fn problems(problems: Vec<pulse::Problem>) -> String {
    problems
        .iter()
        .map(|problem| format!("{}: {}", problem.field, problem.text))
        .collect::<Vec<_>>()
        .join("\n")
}
