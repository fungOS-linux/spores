//! A view host: the Pulse console in a browser.
//!
//! The Spore's Lua view is the renderer. This server runs it through
//! `tools/shot.lua`, serves the scene document it produced, and routes the
//! presses back through `pulse::Store::apply` — the same reducer the CLI uses.
//! It owns the records directory because a browser cannot, which is the same
//! split the worklog receiver has.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use pulse::{Intent, Store, Tab};
use serde::Deserialize;

const PAGE: &str = include_str!("../../web/index.html");
const MAX_LINE: u64 = 16 << 10;
const MAX_SIDE: u32 = 4096;
const MAX_BODY: usize = 64 << 10;
const DEFAULT_BIND: &str = "127.0.0.1:8799";

const USAGE: &str = "\
usage: pulse-serve --records DIR [options]

  --bind <addr:port>   listen address (default 127.0.0.1:8799)
  --records <dir>      the records directory (default <spore>/examples/records)
  --root <dir>         the Spore directory holding ui/ (default the manifest dir)
  --renderer <file>    the scene renderer (default <root>/../../tools/shot.lua)
  --sdk <dir>          fungos-sdk checkout holding lua/fungos/ui.lua (or $FUNGOS_SDK)
  --theme <file>       preview palette (default <root>/../themes/graphite.lua)
  --now <ms>           render at a fixed instant instead of the wall clock";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("pulse-serve: {error}");
            ExitCode::FAILURE
        }
    }
}

struct Config {
    bind: String,
    records: PathBuf,
    root: PathBuf,
    renderer: PathBuf,
    sdk: PathBuf,
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
    let config = Config {
        bind: option(args, "--bind").unwrap_or_else(|| DEFAULT_BIND.to_owned()),
        records: option(args, "--records")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("examples/records")),
        renderer: option(args, "--renderer")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("../../tools/shot.lua")),
        sdk: option(args, "--sdk")
            .or_else(|| std::env::var("FUNGOS_SDK").ok())
            .map(PathBuf::from)
            .ok_or("no SDK: pass --sdk or set FUNGOS_SDK (the checkout with lua/fungos/ui.lua)")?,
        theme: option(args, "--theme")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("../themes/graphite.lua")),
        now: option(args, "--now")
            .map(|value| {
                value
                    .parse::<i64>()
                    .map_err(|_| "--now must be milliseconds")
            })
            .transpose()?,
        root,
    };
    if !config.sdk.join("lua/fungos/ui.lua").exists() {
        return Err(format!(
            "{} has no lua/fungos/ui.lua — not an SDK checkout",
            config.sdk.display()
        ));
    }
    for path in [&config.renderer, &config.theme] {
        if !path.exists() {
            return Err(format!("{} does not exist", path.display()));
        }
    }
    if !config.records.exists() {
        return Err(format!(
            "{} does not exist (pass --records)",
            config.records.display()
        ));
    }

    let listener = TcpListener::bind(&config.bind)
        .map_err(|error| format!("bind {}: {error}", config.bind))?;
    println!(
        "pulse-serve: {} on http://{} (records {})",
        config.root.display(),
        config.bind,
        config.records.display()
    );

    let config = Arc::new(config);
    // One scene is the whole working set. A browser polls; a page that has not
    // moved should cost a hash, not a `lua` process.
    let cache: Arc<Mutex<Option<(u64, String)>>> = Arc::new(Mutex::new(None));
    for connection in listener.incoming() {
        match connection {
            Ok(stream) => {
                let config = Arc::clone(&config);
                let cache = Arc::clone(&cache);
                std::thread::spawn(move || {
                    if let Err(error) = handle(stream, &config, &cache) {
                        eprintln!("pulse-serve: {error}");
                    }
                });
            }
            Err(error) => eprintln!("pulse-serve: accept: {error}"),
        }
    }
    Ok(())
}

fn handle(
    mut stream: TcpStream,
    config: &Config,
    cache: &Mutex<Option<(u64, String)>>,
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
    let method = parts.next().unwrap_or_default().to_owned();
    let target = parts.next().unwrap_or_default().to_owned();

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

    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path, query),
        None => (target.as_str(), ""),
    };

    let outcome = match (method.as_str(), path) {
        ("GET", "/") | ("GET", "/index.html") => respond(
            &mut stream,
            200,
            "text/html; charset=utf-8",
            PAGE,
            &[("cache-control", "no-store")],
        ),
        ("GET", "/scene.json") => scene(&mut stream, config, cache, query),
        ("POST", "/apply") => apply(&mut stream, config, cache, &headers, &mut reader),
        ("GET", _) | ("HEAD", _) => respond(&mut stream, 404, "text/plain", "not found", &[]),
        _ => respond(&mut stream, 405, "text/plain", "GET and POST only", &[]),
    };
    // A store that cannot be read, a renderer that exits non-zero and a missing
    // snapshot all produce the same page symptom: nothing. Say which one it was
    // instead of closing the connection under the page.
    if let Err(error) = outcome {
        eprintln!("pulse-serve: {error}");
        let _ = respond(&mut stream, 500, "text/plain; charset=utf-8", &error, &[]);
    }
    Ok(())
}

fn scene(
    stream: &mut TcpStream,
    config: &Config,
    cache: &Mutex<Option<(u64, String)>>,
    query: &str,
) -> Result<(), String> {
    let width = query_value(query, "w").unwrap_or(1182).clamp(1, MAX_SIDE);
    let height = query_value(query, "h").unwrap_or(1569).clamp(1, MAX_SIDE);
    let tab = Tab::parse(&query_text(query, "tab")).unwrap_or_default();
    let select = query_text(query, "select");
    let hook = query_text(query, "hook");
    let now = config.now.unwrap_or_else(now_ms);

    let store = Store::load(&config.records)?;
    let snapshot = store.snapshot_lua(
        now,
        tab,
        if select.is_empty() {
            None
        } else {
            Some(select.as_str())
        },
        if hook.is_empty() {
            None
        } else {
            Some(hook.as_str())
        },
    );

    let mut hasher = DefaultHasher::new();
    snapshot.hash(&mut hasher);
    width.hash(&mut hasher);
    height.hash(&mut hasher);
    let key = hasher.finish();

    let body = {
        let mut guard = cache
            .lock()
            .map_err(|error| format!("cache lock: {error}"))?;
        match guard.as_ref() {
            Some((cached, body)) if *cached == key => body.clone(),
            _ => {
                let body = render_scene(config, &snapshot, width, height)?;
                *guard = Some((key, body.clone()));
                body
            }
        }
    };

    respond(
        stream,
        200,
        "application/json",
        &body,
        &[("cache-control", "no-store")],
    )
}

#[derive(Deserialize)]
struct ApplyRequest {
    intent: Intent,
}

fn apply(
    stream: &mut TcpStream,
    config: &Config,
    cache: &Mutex<Option<(u64, String)>>,
    headers: &str,
    reader: &mut BufReader<TcpStream>,
) -> Result<(), String> {
    let length: usize = header_value(headers, "content-length")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    if length > MAX_BODY {
        return respond(stream, 413, "text/plain", "body too large", &[]);
    }
    let mut body = vec![0u8; length];
    if length > 0 {
        reader
            .read_exact(&mut body)
            .map_err(|error| format!("read body: {error}"))?;
    }
    let request: ApplyRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(error) => {
            let payload = serde_json::json!({
                "ok": false,
                "problems": [{ "field": "intent", "text": error.to_string() }],
            });
            return respond(
                stream,
                400,
                "application/json",
                &payload.to_string(),
                &[("cache-control", "no-store")],
            );
        }
    };

    let now = config.now.unwrap_or_else(now_ms);
    let mut store = Store::load(&config.records)?;
    let applied = store.apply(&request.intent, now);
    if applied.ok() {
        Store::append(&config.records, &applied.entries)?;
    }

    if let Ok(mut guard) = cache.lock() {
        *guard = None;
    }
    let payload = if applied.ok() {
        serde_json::json!({"ok": true, "message": applied.message})
    } else {
        serde_json::json!({"ok": false, "problems": applied.problems})
    };
    let text = payload.to_string();
    respond(
        stream,
        if applied.ok() { 200 } else { 400 },
        "application/json",
        &text,
        &[("cache-control", "no-store")],
    )
}

static RENDER_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Render through the view the wall runs. `tools/shot.lua` supplies the canvas
/// ABI, resolves `fungos.ui` and writes the scene document; anything this file
/// drew itself would be a second renderer.
fn render_scene(
    config: &Config,
    snapshot: &str,
    width: u32,
    height: u32,
) -> Result<String, String> {
    let id = RENDER_COUNTER.fetch_add(1, Ordering::Relaxed);
    let snapshot_path =
        std::env::temp_dir().join(format!("pulse-snapshot-{}-{id}.lua", std::process::id()));
    let out_path =
        std::env::temp_dir().join(format!("pulse-scene-{}-{id}.json", std::process::id()));
    std::fs::write(&snapshot_path, snapshot)
        .map_err(|error| format!("write {}: {error}", snapshot_path.display()))?;
    let result = Command::new("lua")
        .env("FUNGOS_SDK", &config.sdk)
        .arg(&config.renderer)
        .arg(config.root.join("ui/console.lua"))
        .arg(&snapshot_path)
        .arg(&config.theme)
        .arg(&out_path)
        .arg(width.to_string())
        .arg(height.to_string())
        .output();
    let rendered = match &result {
        Ok(result) if result.status.success() => std::fs::read_to_string(&out_path).ok(),
        _ => None,
    };
    let _ = std::fs::remove_file(&snapshot_path);
    let _ = std::fs::remove_file(&out_path);
    match (result, rendered) {
        (Ok(_), Some(body)) => Ok(body),
        (Ok(result), None) => Err(format!(
            "render failed: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        )),
        (Err(error), _) => Err(format!("run lua: {error}")),
    }
}

fn header_value<'a>(headers: &'a str, name: &str) -> Option<&'a str> {
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

fn query_text(query: &str, name: &str) -> String {
    query
        .split('&')
        .find_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            (key == name).then(|| decode(value))
        })
        .unwrap_or_default()
}

fn decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                match u8::from_str_radix(&value[index + 1..index + 3], 16) {
                    Ok(byte) => {
                        out.push(byte);
                        index += 3;
                    }
                    Err(_) => {
                        out.push(bytes[index]);
                        index += 1;
                    }
                }
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
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
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        _ => "Error",
    };
    let mut head = format!("HTTP/1.1 {status} {reason}\r\ncontent-type: {kind}\r\n");
    head.push_str(&format!("content-length: {}\r\n", body.len()));
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
