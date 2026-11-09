//! A read-only view host: the worklog Spore in a browser.
//!
//! The receiver in `main.rs` owns a signed socket and the log itself; this owns
//! neither. It answers two GETs, and for the scene it runs the same Lua the wall
//! runs — the renderer is the Spore's view, not a second implementation of it in
//! JavaScript. What the browser draws is what Canvas draws, at whatever size the
//! browser asks for.
//!
//! Usage: worklog-serve [--bind 127.0.0.1:8798] [--root <spore dir>]
//!                      [--snapshot <snapshot.lua>] [--theme <theme.lua>]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::{Arc, Mutex};
use std::time::{Duration, UNIX_EPOCH};

/// The page is part of the binary: a view host with an install-dependent file
/// path would be a host that works only where it was built.
const PAGE: &str = include_str!("../../web/index.html");
const MAX_LINE: u64 = 8 << 10;
/// A browser asking for a scene is asking for a window, not for a poster.
const MAX_SIDE: u32 = 4096;
/// The desktop's loopback range is spoken for one surface at a time — 8794 is
/// the wiki, 8797 the SFX board — so this takes the next shelf above them.
const DEFAULT_BIND: &str = "127.0.0.1:8795";

const USAGE: &str = "\
worklog-serve — serve the worklog view to a browser

  --bind <addr:port>   listen address (default 127.0.0.1:8795)
  --root <dir>         the Spore directory holding ui/ and examples/
  --renderer <file>    the scene renderer (default <root>/../../tools/shot.lua)
  --sdk <dir>          fungos-sdk checkout holding lua/fungos/ui.lua (default $FUNGOS_SDK)
  --snapshot <file>    the snapshot Lua to render (default examples/worklog_mock.lua)
  --theme <file>       preview palette and type scale (default ../themes/graphite.lua)
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
            eprintln!("worklog-serve: {error}");
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
    theme: PathBuf,
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
        bind: option(args, "--bind").unwrap_or_else(|| DEFAULT_BIND.to_string()),
        // The view belongs to the Spore; the scene renderer is shared tooling two
        // levels up. Both are named so neither has to guess the other's layout.
        renderer: option(args, "--renderer")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("../../tools/shot.lua")),
        // The renderer resolves `fungos.ui` through `FUNGOS_SDK`, and a service
        // that inherits whatever the shell happened to export breaks the first
        // time it is started from somewhere else. Name it once, here.
        sdk: option(args, "--sdk")
            .or_else(|| std::env::var("FUNGOS_SDK").ok())
            .map(PathBuf::from)
            .ok_or("no SDK: pass --sdk or set FUNGOS_SDK (the checkout with lua/fungos/ui.lua)")?,
        snapshot: option(args, "--snapshot")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("examples/worklog_mock.lua")),
        theme: option(args, "--theme")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("../themes/graphite.lua")),
        root,
    };
    if !config.sdk.join("lua/fungos/ui.lua").exists() {
        return Err(format!(
            "{} has no lua/fungos/ui.lua — not an SDK checkout",
            config.sdk.display()
        ));
    }
    for path in [&config.snapshot, &config.theme, &config.renderer] {
        if !path.exists() {
            return Err(format!("{} does not exist", path.display()));
        }
    }

    let listener = TcpListener::bind(&config.bind)
        .map_err(|error| format!("bind {}: {error}", config.bind))?;
    println!(
        "worklog-serve: {} on http://{} (snapshot {})",
        config.root.display(),
        config.bind,
        config.snapshot.display()
    );

    let config = Arc::new(config);
    // One scene is the whole working set, and re-running `lua` for a browser that
    // polls every five seconds is a process per request for a page that mostly has
    // not changed.
    let cached: Arc<Mutex<Option<(CacheKey, String)>>> = Arc::new(Mutex::new(None));
    for connection in listener.incoming() {
        match connection {
            Ok(stream) => {
                let config = Arc::clone(&config);
                let cached = Arc::clone(&cached);
                // A browser opens several connections at once, and a client that
                // connects and then stalls must cost its own thread and nothing
                // else. Serving them in turn made the page miss its own poll.
                std::thread::spawn(move || {
                    if let Err(error) = handle(stream, &config, &cached) {
                        eprintln!("worklog-serve: {error}");
                    }
                });
            }
            Err(error) => eprintln!("worklog-serve: accept: {error}"),
        }
    }
    Ok(())
}

#[derive(Clone, PartialEq, Eq)]
struct CacheKey {
    /// The snapshot's modification time, so a fresh snapshot re-renders and an
    /// untouched one costs nothing.
    stamp: u64,
    width: u32,
    height: u32,
}

fn handle(
    mut stream: TcpStream,
    config: &Config,
    cached: &Mutex<Option<(CacheKey, String)>>,
) -> Result<(), String> {
    // A client that connects and stalls costs its own thread and nothing else.
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
    // Nothing reads the body, and a GET has none — draining the stream here waits
    // out the read timeout on every request, because the client is waiting for the
    // response, not sending anything.

    if method != "GET" && method != "HEAD" {
        return respond(&mut stream, 405, "text/plain", "this host answers GET only");
    }
    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path, query),
        None => (target, ""),
    };

    match path {
        "/" | "/index.html" => respond(&mut stream, 200, "text/html; charset=utf-8", PAGE),
        "/scene.json" => {
            let width = query_value(query, "w").unwrap_or(1182).clamp(1, MAX_SIDE);
            let height = query_value(query, "h").unwrap_or(1569).clamp(1, MAX_SIDE);
            let key = CacheKey {
                stamp: modified(&config.snapshot)?,
                width,
                height,
            };
            let mut guard = cached
                .lock()
                .map_err(|error| format!("cache lock: {error}"))?;
            let hit = match guard.as_ref() {
                Some((cached_key, body)) if *cached_key == key => Some(body.clone()),
                _ => None,
            };
            let body = match hit {
                Some(body) => body,
                None => match render_scene(config, width, height) {
                    Ok(body) => {
                        *guard = Some((key, body.clone()));
                        body
                    }
                    // A dropped connection reaches the page as "Failed to
                    // fetch", which says nothing about the view. The status line
                    // can, so the render's own error is the response.
                    Err(error) => {
                        drop(guard);
                        return respond(&mut stream, 500, "text/plain; charset=utf-8", &error);
                    }
                },
            };
            drop(guard);
            respond(&mut stream, 200, "application/json", &body)
        }
        _ => respond(&mut stream, 404, "text/plain", "not found"),
    }
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

/// Render through the view the wall runs. `tools/shot.lua` is the renderer: it
/// supplies the `canvas` ABI, resolves the SDK library and writes the scene
/// document. Anything this file drew itself would be a second renderer.
fn render_scene(config: &Config, width: u32, height: u32) -> Result<String, String> {
    let out = std::env::temp_dir().join(format!("worklog-scene-{}.json", std::process::id()));
    let result = Command::new("lua")
        .env("FUNGOS_SDK", &config.sdk)
        .arg(&config.renderer)
        .arg(config.root.join("ui/worklog.lua"))
        .arg(&config.snapshot)
        .arg(&config.theme)
        .arg(&out)
        .arg(width.to_string())
        .arg(height.to_string())
        .output()
        .map_err(|error| format!("run lua: {error}"))?;
    if !result.status.success() {
        let _ = std::fs::remove_file(&out);
        // The view's own words, not a status code: a snapshot with a shape the
        // view did not expect says so on stderr and the page shows it.
        return Err(format!(
            "render failed: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        ));
    }
    let body = std::fs::read_to_string(&out)
        .map_err(|error| format!("read {}: {error}", out.display()))?;
    let _ = std::fs::remove_file(&out);
    Ok(body)
}

fn respond(stream: &mut TcpStream, status: u16, kind: &str, body: &str) -> Result<(), String> {
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        _ => "Error",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\ncontent-type: {kind}\r\ncontent-length: {}\r\ncache-control: no-store\r\nconnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.write_all(body.as_bytes()))
        .and_then(|()| stream.flush())
        .map_err(|error| format!("respond: {error}"))
}
