//! A read-only view host: the fleet Spore in a browser.
//!
//! The daemon owns peer facts and topology; this owns neither. It answers two
//! GETs, and for the scene it runs the same Lua the wall runs. What the browser
//! draws is what Canvas draws, at whatever size the browser asks for.
//!
//! Usage: fleet-serve [--bind 127.0.0.1:8796] [--root <spore dir>]
//!                     [--snapshot <snapshot.lua>]
//!                     [--status <fleet-status.json>] [--topology <topology.json>]
//!                     [--theme <theme.lua>] [--sdk <repo root>] [--print]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::{Arc, Mutex};
use std::time::{Duration, UNIX_EPOCH};

use fleet::{StatusRow, build, to_lua};

const PAGE: &str = include_str!("../../web/index.html");
const MAX_LINE: u64 = 8 << 10;
const MAX_SIDE: u32 = 4096;
/// The desktop's loopback range is spoken for one surface at a time — 8795 is
/// the worklog — so this takes the next shelf above it.
const DEFAULT_BIND: &str = "127.0.0.1:8796";

const USAGE: &str = "\
fleet-serve — serve the fleet view to a browser

  --bind <addr:port>   listen address (default 127.0.0.1:8796)
  --root <dir>         the Spore directory holding ui/ and examples/
  --renderer <file>    the scene renderer (default <root>/../../tools/shot.lua)
  --sdk <dir>          checkout holding sdk/ui/ui.lua (default <root>/../..)
  --snapshot <file>    a prebuilt snapshot Lua (default examples/fleet_snapshot.lua)
  --status <file>      `mycelium fleet status --json` output; overrides --snapshot
  --topology <file>    `mycelium topology --json` output (optional)
  --theme <file>       preview palette and type scale (default ../themes/graphite.lua)
  --now <epoch>        pin the classification instant (default: newest last_seen)
  --print              write the normalized snapshot Lua to stdout and exit
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("fleet-serve: {error}");
            ExitCode::FAILURE
        }
    }
}

struct Config {
    bind: String,
    root: PathBuf,
    renderer: PathBuf,
    sdk: PathBuf,
    snapshot: PathBuf,
    status: Option<PathBuf>,
    topology: Option<PathBuf>,
    theme: PathBuf,
    now: Option<i64>,
}

fn option(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn run(args: &[String]) -> Result<(), String> {
    let root = option(args, "--root")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    let sdk = option(args, "--sdk")
        .or_else(|| std::env::var("FUNGOS_SDK").ok())
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("../.."));
    let config = Config {
        bind: option(args, "--bind").unwrap_or_else(|| DEFAULT_BIND.to_string()),
        renderer: option(args, "--renderer")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("../../tools/shot.lua")),
        snapshot: option(args, "--snapshot")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("examples/fleet_snapshot.lua")),
        status: option(args, "--status").map(PathBuf::from),
        topology: option(args, "--topology").map(PathBuf::from),
        theme: option(args, "--theme")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("../themes/graphite.lua")),
        now: option(args, "--now")
            .map(|value| value.parse().map_err(|_| "--now expects epoch seconds"))
            .transpose()?,
        sdk,
        root,
    };
    let ui = config.sdk.join("sdk/ui/ui.lua");
    if !ui.exists() {
        return Err(format!(
            "{} has no sdk/ui/ui.lua — pass --sdk with the Spore repository root",
            config.sdk.display()
        ));
    }
    for path in [&config.theme, &config.renderer] {
        if !path.exists() {
            return Err(format!("{} does not exist", path.display()));
        }
    }

    if args.iter().any(|arg| arg == "--print") {
        let status = config
            .status
            .as_deref()
            .ok_or("--print needs --status <fleet-status.json>")?;
        print!(
            "{}",
            normalize(&config, status, config.topology.as_deref())?
        );
        return Ok(());
    }

    let listener = TcpListener::bind(&config.bind)
        .map_err(|error| format!("bind {}: {error}", config.bind))?;
    println!(
        "fleet-serve: {} on http://{} ({})",
        config.root.display(),
        config.bind,
        match &config.status {
            Some(status) => format!("status {}", status.display()),
            None => format!("snapshot {}", config.snapshot.display()),
        }
    );

    let config = Arc::new(config);
    let cached: Arc<Mutex<Option<(CacheKey, String)>>> = Arc::new(Mutex::new(None));
    for connection in listener.incoming() {
        match connection {
            Ok(stream) => {
                let config = Arc::clone(&config);
                let cached = Arc::clone(&cached);
                std::thread::spawn(move || {
                    if let Err(error) = handle(stream, &config, &cached) {
                        eprintln!("fleet-serve: {error}");
                    }
                });
            }
            Err(error) => eprintln!("fleet-serve: accept: {error}"),
        }
    }
    Ok(())
}

#[derive(Clone, PartialEq, Eq)]
struct CacheKey {
    /// Everything that can change the picture: the inputs' stamps and the window.
    stamp: u64,
    width: u32,
    height: u32,
}

fn handle(
    mut stream: TcpStream,
    config: &Config,
    cached: &Mutex<Option<(CacheKey, String)>>,
) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|error| format!("set_read_timeout: {error}"))?;
    let _ = stream.set_nodelay(true);
    let mut reader = BufReader::new(stream.try_clone().map_err(|error| error.to_string())?);
    let mut line = String::new();
    reader
        .by_ref()
        .take(MAX_LINE)
        .read_line(&mut line)
        .map_err(|error| format!("read request line: {error}"))?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();
    let mut headers = String::new();
    loop {
        let mut header = String::new();
        let read = reader
            .by_ref()
            .take(MAX_LINE)
            .read_line(&mut header)
            .map_err(|error| format!("read header: {error}"))?;
        if read == 0 || header.trim().is_empty() {
            break;
        }
        headers.push_str(&header);
        if headers.len() > MAX_LINE as usize {
            break;
        }
    }

    if method != "GET" && method != "HEAD" {
        return respond(
            &mut stream,
            405,
            "text/plain",
            "this host answers GET only",
            &[],
        );
    }
    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path, query),
        None => (target, ""),
    };

    match path {
        "/" | "/index.html" => respond(
            &mut stream,
            200,
            "text/html; charset=utf-8",
            PAGE,
            &[("cache-control", "no-store")],
        ),
        "/scene.json" => {
            let width = query_value(query, "w").unwrap_or(1600).clamp(1, MAX_SIDE);
            let height = query_value(query, "h").unwrap_or(1000).clamp(1, MAX_SIDE);
            let key = CacheKey {
                stamp: input_stamp(config)?,
                width,
                height,
            };
            let body = {
                let mut guard = cached
                    .lock()
                    .map_err(|error| format!("cache lock: {error}"))?;
                let hit = match guard.as_ref() {
                    Some((cached_key, body)) if *cached_key == key => Some(body.clone()),
                    _ => None,
                };
                match hit {
                    Some(body) => body,
                    None => match render_scene(config, width, height) {
                        Ok(body) => {
                            *guard = Some((key.clone(), body.clone()));
                            body
                        }
                        Err(error) => {
                            return respond(
                                &mut stream,
                                500,
                                "text/plain; charset=utf-8",
                                &error,
                                &[],
                            );
                        }
                    },
                }
            };
            let etag = format!("\"{}-{width}-{height}-{}\"", key.stamp, body.len());
            if header(&headers, "if-none-match")
                .is_some_and(|value| value.split(',').any(|tag| tag.trim() == etag))
            {
                eprintln!("fleet-serve: GET {path}?{query} 304 unchanged");
                return respond(&mut stream, 304, "application/json", "", &[("etag", &etag)]);
            }
            eprintln!("fleet-serve: GET {path}?{query} 200 sent");
            respond(
                &mut stream,
                200,
                "application/json",
                &body,
                &[("etag", &etag), ("cache-control", "no-cache")],
            )
        }
        _ => respond(&mut stream, 404, "text/plain", "not found", &[]),
    }
}

fn header<'a>(headers: &'a str, name: &str) -> Option<&'a str> {
    headers.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

fn query_value(query: &str, name: &str) -> Option<u32> {
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then(|| value.parse().ok()).flatten()
    })
}

fn modified(path: &Path) -> Result<u64, String> {
    let meta =
        std::fs::metadata(path).map_err(|error| format!("stat {}: {error}", path.display()))?;
    let stamp = meta
        .modified()
        .map_err(|error| format!("mtime {}: {error}", path.display()))?
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("mtime {}: {error}", path.display()))?;
    Ok(stamp.as_secs())
}

fn input_stamp(config: &Config) -> Result<u64, String> {
    match &config.status {
        Some(status) => {
            let topology = match &config.topology {
                Some(path) => modified(path)?,
                None => 0,
            };
            Ok(modified(status)?.wrapping_mul(31).wrapping_add(topology))
        }
        None => modified(&config.snapshot),
    }
}

/// The instant a snapshot is classified against. The newest `last_seen` is the
/// daemon's own idea of "now" and needs no clock; an explicit `--now` pins it
/// for a fixture that has to stay still.
fn classification_now(config: &Config, status: &[StatusRow]) -> Result<i64, String> {
    if let Some(now) = config.now {
        return Ok(now);
    }
    Ok(status
        .iter()
        .filter_map(|row| row.last_seen)
        .max()
        .unwrap_or(0))
}

fn normalize(
    config: &Config,
    status_path: &Path,
    topology_path: Option<&Path>,
) -> Result<String, String> {
    let status: Vec<StatusRow> = serde_json::from_str(
        &std::fs::read_to_string(status_path)
            .map_err(|error| format!("read {}: {error}", status_path.display()))?,
    )
    .map_err(|error| {
        format!(
            "{} is not `fleet status --json`: {error}",
            status_path.display()
        )
    })?;
    let topology = match topology_path {
        Some(path) => Some(
            serde_json::from_str::<serde_json::Value>(
                &std::fs::read_to_string(path)
                    .map_err(|error| format!("read {}: {error}", path.display()))?,
            )
            .map_err(|error| format!("{} is not `topology --json`: {error}", path.display()))?,
        ),
        None => None,
    };
    let now = classification_now(config, &status)?;
    to_lua(&build(&status, topology.as_ref(), now)).map_err(|error| error.to_string())
}

fn render_scene(config: &Config, width: u32, height: u32) -> Result<String, String> {
    let snapshot = match &config.status {
        Some(status) => {
            let path =
                std::env::temp_dir().join(format!("fleet-snapshot-{}.lua", std::process::id()));
            let body = normalize(config, status, config.topology.as_deref())?;
            std::fs::write(&path, body)
                .map_err(|error| format!("write {}: {error}", path.display()))?;
            path
        }
        None => config.snapshot.clone(),
    };
    let out = std::env::temp_dir().join(format!("fleet-scene-{}.json", std::process::id()));
    let result = Command::new("lua")
        .env("FUNGOS_SDK", &config.sdk)
        .arg(&config.renderer)
        .arg(config.root.join("ui/fleet.lua"))
        .arg(&snapshot)
        .arg(&config.theme)
        .arg(&out)
        .arg(width.to_string())
        .arg(height.to_string())
        .output()
        .map_err(|error| format!("run lua: {error}"))?;
    if !result.status.success() {
        let _ = std::fs::remove_file(&out);
        return Err(format!(
            "render failed: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        ));
    }
    let body = std::fs::read_to_string(&out)
        .map_err(|error| format!("read {}: {error}", out.display()))?;
    let _ = std::fs::remove_file(&out);
    if config.status.is_some() {
        let _ = std::fs::remove_file(&snapshot);
    }
    Ok(body)
}

fn respond(
    stream: &mut TcpStream,
    status: u16,
    kind: &str,
    body: &str,
    extra: &[(&str, &str)],
) -> Result<(), String> {
    let reason = match status {
        200 => "OK",
        304 => "Not Modified",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        _ => "Error",
    };
    let mut head = format!("HTTP/1.1 {status} {reason}\r\ncontent-type: {kind}\r\n");
    if status != 304 {
        head.push_str(&format!("content-length: {}\r\n", body.len()));
    }
    for (name, value) in extra {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("connection: close\r\n\r\n");
    stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.write_all(body.as_bytes()))
        .and_then(|()| stream.flush())
        .map_err(|error| format!("respond: {error}"))
}
