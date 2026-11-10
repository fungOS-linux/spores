//! `spore-serve`: one Spore view, one page.
//!
//! A Spore view is a Lua file that paints a scene; a host supplies the snapshot
//! and records the primitives. This host supplies the snapshot from a producer
//! (a file or a command) and records the primitives by running the view through
//! `tools/shot.lua` — the same renderer `tend` and the Spore tests use, so what
//! a browser sees is what the Spore draws, not a second implementation of it.
//!
//! ```text
//! spore-serve --root spores/terminal --view ui/grid.lua \
//!   --snapshot-cmd 'curl -fsS http://127.0.0.1:8792/snapshot.lua?session=1' \
//!   --proxy http://127.0.0.1:8792 --bind 127.0.0.1:8793
//! ```
//!
//! `--proxy` forwards everything this host does not serve — input, resize,
//! scroll, sessions, events — to the process that owns the state. The renderer
//! is stateless: it holds a socket and a view, and nothing else.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::Arc;
use std::time::Duration;

const INDEX_HTML: &str = include_str!("page.html");

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("spore-serve: {error}");
            ExitCode::FAILURE
        }
    }
}

struct Config {
    view: PathBuf,
    snapshot: SnapshotSource,
    theme: PathBuf,
    sdk: PathBuf,
    lua: String,
    renderer: PathBuf,
    width: u32,
    height: u32,
    proxy: Option<String>,
    title: String,
}

enum SnapshotSource {
    File(PathBuf),
    Command(String),
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut root = None;
    let mut view = None;
    let mut snapshot_file = None;
    let mut snapshot_cmd = None;
    let mut theme = None;
    let mut sdk = std::env::var("FUNGOS_SDK").ok().map(PathBuf::from);
    let mut lua = std::env::var("TEND_LUA").unwrap_or_else(|_| "lua".to_string());
    let mut renderer = None;
    let mut width = 1200;
    let mut height = 900;
    let mut proxy = None;
    let mut bind = "127.0.0.1:8793".to_string();
    let mut title: Option<String> = None;

    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let mut value = || -> Result<String, String> {
            index += 1;
            args.get(index)
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match flag {
            "--root" => root = Some(PathBuf::from(value()?)),
            // A Spore already names its view in `spore.toml`. Repeating the path
            // on the command line is how a host and a manifest come to disagree.
            "--spore" => {
                let dir = PathBuf::from(value()?);
                root = Some(dir.clone());
                view = Some(spore_view(&dir)?);
            }
            "--view" => view = Some(PathBuf::from(value()?)),
            "--snapshot" => snapshot_file = Some(PathBuf::from(value()?)),
            "--snapshot-cmd" => snapshot_cmd = Some(value()?),
            "--theme" => theme = Some(PathBuf::from(value()?)),
            "--sdk" => sdk = Some(PathBuf::from(value()?)),
            "--lua" => lua = value()?,
            "--renderer" => renderer = Some(PathBuf::from(value()?)),
            "--width" => width = value()?.parse().map_err(|_| "bad --width")?,
            "--height" => height = value()?.parse().map_err(|_| "bad --height")?,
            "--proxy" => proxy = Some(value()?),
            "--bind" => bind = value()?,
            "--title" => title = Some(value()?),
            "--help" | "-h" => {
                println!(
                    "spore-serve --root <spore> [--view ui/x.lua] \
                     (--snapshot file.lua | --snapshot-cmd 'cmd') \
                     [--theme f.lua] [--sdk dir] [--lua lua] [--renderer shot.lua] \
                     [--width 1200] [--height 900] [--proxy url] [--bind host:port]"
                );
                return Ok(());
            }
            other => return Err(format!("unknown flag {other}")),
        }
        index += 1;
    }

    let root = root.ok_or("--root <spore dir> is required")?;
    let view_file = view.clone().unwrap_or_else(|| PathBuf::from("ui/view.lua"));
    let view = view_file.clone();
    let theme = theme.unwrap_or_else(|| root.join("../../spores/themes/graphite.lua"));
    let renderer = renderer.unwrap_or_else(|| root.join("../../tools/shot.lua"));
    let sdk = sdk.ok_or("no SDK: pass --sdk or set FUNGOS_SDK")?;
    let snapshot = match (snapshot_cmd, snapshot_file) {
        (Some(command), _) => SnapshotSource::Command(command),
        (None, Some(path)) => SnapshotSource::File(path),
        (None, None) => return Err("one of --snapshot or --snapshot-cmd is required".into()),
    };

    let config = Arc::new(Config {
        view: if view.is_absolute() {
            view
        } else {
            root.join(view)
        },
        snapshot,
        theme,
        sdk,
        lua,
        renderer,
        width,
        height,
        proxy,
        title: title.unwrap_or_else(|| {
            let stem = view_file
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_string());
            stem.unwrap_or_else(|| "spore".to_string())
        }),
    });

    let listener = TcpListener::bind(&bind).map_err(|error| format!("bind {bind}: {error}"))?;
    println!("spore-serve: {} at http://{bind}/", config.view.display());
    if let Some(proxy) = &config.proxy {
        println!("spore-serve: proxying state to {proxy}");
    }
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let config = Arc::clone(&config);
        std::thread::spawn(move || {
            let _ = handle(stream, &config);
        });
    }
    Ok(())
}

struct Request {
    method: String,
    target: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

fn handle(mut stream: TcpStream, config: &Config) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|error| error.to_string())?;
    let Some(request) = read_request(&mut stream)? else {
        return Ok(());
    };

    let path = request.target.split('?').next().unwrap_or("/");
    match (request.method.as_str(), path) {
        ("GET", "/") => {
            let page = INDEX_HTML.replace("__TITLE__", &config.title);
            respond(
                &mut stream,
                "200 OK",
                "text/html; charset=utf-8",
                page.as_bytes(),
            )
        }
        ("GET", "/health") => respond(&mut stream, "200 OK", "text/plain; charset=utf-8", b"ok\n"),
        ("GET", "/scene.json") => {
            // A page asks for the surface it is drawing on: a terminal sized for
            // a 1200x900 authoring window and then stretched to a laptop screen
            // has cells in the wrong place and glyphs at the wrong size.
            let width = query_number(&request.target, "w").unwrap_or(config.width);
            let height = query_number(&request.target, "h").unwrap_or(config.height);
            // Everything but the surface size belongs to the producer: a session
            // id, a path, a query. `{query}` in `--snapshot-cmd` is how the
            // command sees it, so one host serves every session the producer has.
            let passthrough = query_without(&request.target, &["w", "h"]);
            // The page's own viewport is the one number a blank screen hides.
            eprintln!("scene {width}x{height} {passthrough}");
            match scene(
                config,
                width.clamp(200, 4000),
                height.clamp(200, 4000),
                &passthrough,
            ) {
                Ok(body) => respond(
                    &mut stream,
                    "200 OK",
                    "application/json; charset=utf-8",
                    body.as_bytes(),
                ),
                Err(error) => respond(
                    &mut stream,
                    "500 Internal Server Error",
                    "text/plain; charset=utf-8",
                    format!("{error}\n").as_bytes(),
                ),
            }
        }
        ("GET", "/events") => events(&mut stream, config, &request.target),
        _ => match &config.proxy {
            Some(upstream) => forward(&mut stream, upstream, &request),
            None => respond(
                &mut stream,
                "404 Not Found",
                "text/plain; charset=utf-8",
                b"not found\n",
            ),
        },
    }
}

/// Ask the producer for one snapshot. This is the cheap half: a command that
/// prints a few kilobytes, not a render.
fn snapshot(config: &Config, query: &str) -> Result<String, String> {
    match &config.snapshot {
        SnapshotSource::File(path) => std::fs::read_to_string(path)
            .map_err(|error| format!("read {}: {error}", path.display())),
        SnapshotSource::Command(command) => {
            let command = command.replace("{query}", query);
            let output = Command::new("sh")
                .arg("-c")
                .arg(&command)
                .output()
                .map_err(|error| format!("run snapshot command: {error}"))?;
            if !output.status.success() {
                return Err(format!(
                    "snapshot command failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
            }
            String::from_utf8(output.stdout).map_err(|error| format!("snapshot is not utf-8: {error}"))
        }
    }
}

/// Render one snapshot through the Spore's view.
fn render(config: &Config, snapshot: &str, width: u32, height: u32) -> Result<String, String> {
    let stamp = format!("{}-{}", std::process::id(), now_ms());
    let dir = std::env::temp_dir();
    let snapshot_path = dir.join(format!("spore-serve-snapshot-{stamp}.lua"));
    std::fs::write(&snapshot_path, snapshot)
        .map_err(|error| format!("write {}: {error}", snapshot_path.display()))?;

    let out = dir.join(format!("spore-serve-scene-{stamp}.json"));
    let result = Command::new(&config.lua)
        .env("FUNGOS_SDK", &config.sdk)
        .arg(&config.renderer)
        .arg(&config.view)
        .arg(&snapshot_path)
        .arg(&config.theme)
        .arg(&out)
        .arg(width.to_string())
        .arg(height.to_string())
        .output()
        .map_err(|error| format!("run {}: {error}", config.lua))?;

    let _ = std::fs::remove_file(&snapshot_path);
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
    Ok(body)
}

fn scene(config: &Config, width: u32, height: u32, query: &str) -> Result<String, String> {
    let snapshot = snapshot(config, query)?;
    render(config, &snapshot, width, height)
}

/// Push a new scene whenever the producer's snapshot changes.
///
/// The page used to poll, which made every keystroke wait for the next tick and
/// every tick pay for a render whether or not anything had happened. This is the
/// same work, driven by the producer instead of a clock: read the snapshot (a
/// few kilobytes), compare, render only on change, push.
fn events(stream: &mut TcpStream, config: &Config, target: &str) -> Result<(), String> {
    let width = query_number(target, "w").unwrap_or(config.width).clamp(200, 4000);
    let height = query_number(target, "h").unwrap_or(config.height).clamp(200, 4000);
    let passthrough = query_without(target, &["w", "h"]);

    stream
        .write_all(
            b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream; charset=utf-8\r\ncache-control: no-store\r\nconnection: keep-alive\r\nx-accel-buffering: no\r\n\r\n",
        )
        .map_err(|error| error.to_string())?;
    stream.flush().map_err(|error| error.to_string())?;

    let mut last: Option<String> = None;
    let mut last_write = std::time::Instant::now();
    loop {
        match snapshot(config, &passthrough) {
            Ok(current) => {
                if last.as_deref() != Some(current.as_str()) {
                    match render(config, &current, width, height) {
                        Ok(body) => {
                            let message = format!("data: {body}\n\n");
                            if stream.write_all(message.as_bytes()).is_err() {
                                return Ok(());
                            }
                            if stream.flush().is_err() {
                                return Ok(());
                            }
                            last = Some(current);
                            last_write = std::time::Instant::now();
                        }
                        Err(error) => {
                            let message = format!("event: error\ndata: {}\n\n", error.replace('\n', " "));
                            if stream.write_all(message.as_bytes()).is_err() {
                                return Ok(());
                            }
                            let _ = stream.flush();
                            last = Some(current);
                            last_write = std::time::Instant::now();
                        }
                    }
                }
            }
            Err(error) => {
                let message = format!("event: error\ndata: {}\n\n", error.replace('\n', " "));
                if stream.write_all(message.as_bytes()).is_err() {
                    return Ok(());
                }
                let _ = stream.flush();
                last_write = std::time::Instant::now();
            }
        }
        if last_write.elapsed() > std::time::Duration::from_secs(15) {
            if stream.write_all(b": keepalive\n\n").is_err() {
                return Ok(());
            }
            let _ = stream.flush();
            last_write = std::time::Instant::now();
        }
        // The snapshot read is a few kilobytes; the render is the expensive part
        // and only runs on change. Polling the producer this often is what makes
        // a keystroke reach the screen in one frame.
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
}

/// Hand an unserved request to the process that owns the state, byte for byte.
/// The response is copied through, so SSE stays streaming and a 204 stays a 204.
fn forward(stream: &mut TcpStream, upstream: &str, request: &Request) -> Result<(), String> {
    let address = upstream.trim_start_matches("http://").trim_end_matches('/');
    let mut peer =
        TcpStream::connect(address).map_err(|error| format!("connect {address}: {error}"))?;

    let mut head = format!("{} {} HTTP/1.1\r\n", request.method, request.target);
    head.push_str(&format!("host: {address}\r\n"));
    head.push_str("connection: close\r\n");
    for (name, value) in &request.headers {
        match name.as_str() {
            "host" | "connection" | "content-length" => {}
            _ => head.push_str(&format!("{name}: {value}\r\n")),
        }
    }
    if !request.body.is_empty() {
        head.push_str(&format!("content-length: {}\r\n", request.body.len()));
    }
    head.push_str("\r\n");
    peer.write_all(head.as_bytes()).map_err(|e| e.to_string())?;
    if !request.body.is_empty() {
        peer.write_all(&request.body).map_err(|e| e.to_string())?;
    }

    let mut buffer = [0u8; 16 * 1024];
    loop {
        match peer.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => stream
                .write_all(&buffer[..read])
                .map_err(|e| e.to_string())?,
            Err(error) => return Err(format!("proxy read: {error}")),
        }
    }
    Ok(())
}

fn read_request(stream: &mut TcpStream) -> Result<Option<Request>, String> {
    let mut raw = Vec::new();
    let mut buffer = [0u8; 8 * 1024];
    let head_end;
    loop {
        let read = stream.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            return Ok(None);
        }
        raw.extend_from_slice(&buffer[..read]);
        if let Some(position) = find(&raw, b"\r\n\r\n") {
            head_end = position + 4;
            break;
        }
        if raw.len() > 64 * 1024 {
            return Err("request head too large".into());
        }
    }

    let head = String::from_utf8_lossy(&raw[..head_end]).to_string();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or("/").to_string();
    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }

    let length: usize = headers
        .iter()
        .find(|(name, _)| name == "content-length")
        .and_then(|(_, value)| value.parse().ok())
        .unwrap_or(0);
    let mut body = raw[head_end..].to_vec();
    while body.len() < length {
        let read = stream.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&buffer[..read]);
    }
    body.truncate(length);

    Ok(Some(Request {
        method,
        target,
        headers,
        body,
    }))
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn respond(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> Result<(), String> {
    let head = format!(
        "HTTP/1.1 {status}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\ncache-control: no-store\r\nconnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(head.as_bytes())
        .map_err(|e| e.to_string())?;
    stream.write_all(body).map_err(|e| e.to_string())
}

/// The request's query with the surface size removed, ready to hand to the
/// producer: `?session=1&path=src`, or an empty string.
fn query_without(target: &str, drop: &[&str]) -> String {
    let Some((_, query)) = target.split_once('?') else {
        return String::new();
    };
    let kept: Vec<&str> = query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .filter(|pair| {
            let key = pair.split_once('=').map_or(*pair, |(key, _)| key);
            !drop.contains(&key)
        })
        .collect();
    if kept.is_empty() {
        String::new()
    } else {
        format!("?{}", kept.join("&"))
    }
}

/// The view a Spore declares in its manifest: `[ui] entry = "ui/x.lua"`.
///
/// A manifest is TOML, but this reads one string from one table. A full parser
/// would be a dependency for a key that is always written the same way, and a
/// missing entry is a clear error rather than a silent default.
fn spore_view(dir: &Path) -> Result<PathBuf, String> {
    let manifest = dir.join("spore.toml");
    let text = std::fs::read_to_string(&manifest)
        .map_err(|error| format!("read {}: {error}", manifest.display()))?;
    let mut in_ui = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_ui = line == "[ui]";
            continue;
        }
        if in_ui
            && let Some(rest) = line.strip_prefix("entry")
            && let Some((_, value)) = rest.split_once('=')
        {
            let value = value.trim().trim_matches('"').trim_matches('\'');
            if !value.is_empty() {
                return Ok(PathBuf::from(value));
            }
        }
    }
    Err(format!("{} has no [ui] entry", manifest.display()))
}

fn query_number(target: &str, name: &str) -> Option<u32> {
    let query = target.split_once('?')?.1;
    for pair in query.split('&') {
        if let Some((key, value)) = pair.split_once('=')
            && key == name
        {
            return value.parse().ok();
        }
    }
    None
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[allow(dead_code)]
fn exists(path: &Path) -> bool {
    path.is_file()
}
