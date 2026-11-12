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
/// The rasterizer's bench, not the worklog. See `web/probe.html`.
const PROBE: &str = include_str!("../../web/probe.html");
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
    /// The wasm scene renderer and its glue, built from `soil-scene-gpu`. The
    /// page imports them; this host only carries them.
    wasm: PathBuf,
    /// The face the renderer rasterizes text with. A host input like the theme:
    /// nothing licensed is compiled into the module, and the page caches what it
    /// is given by content id.
    font: Option<(PathBuf, String)>,
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
        wasm: option(args, "--wasm-dir")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("web/pkg")),
        font: match option(args, "--font")
            .map(PathBuf::from)
            .or_else(default_font)
        {
            // The id is computed once, at startup: it is a property of the file,
            // and hashing a megabyte on every page load to learn something that
            // cannot have changed is the kind of work a server should do once.
            Some(path) => {
                let id = font_id(&path)?;
                Some((path, id))
            }
            None => None,
        },
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
    /// The host state the view reads. Two queries are two pictures, so they are
    /// two cache entries — a page that searches must not be handed the last
    /// unfiltered scene.
    query: String,
    offset: u32,
    hover: String,
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
    // response, not sending anything. The headers are worth reading though: the
    // page asks whether the scene changed, and answering that is what keeps it
    // from repainting a page that has not moved.
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
        // The bench reports to the host it came from, so the numbers a browser
        // computes are readable without anyone copying them out of a page.
        path if path.starts_with("/probe-report/") => {
            let said = &path["/probe-report/".len()..];
            eprintln!("probe-report: {said}");
            respond(&mut stream, 204, "text/plain", "", &[])
        }
        "/probe" => respond(
            &mut stream,
            200,
            "text/html; charset=utf-8",
            PROBE,
            &[("cache-control", "no-store")],
        ),
        // The page renders with the same rasterizer the wall does: this host
        // ships the module, the page runs it.
        "/pkg/scene.js" => serve_asset(
            &mut stream,
            &config.wasm.join("scene.js"),
            "text/javascript; charset=utf-8",
            &headers,
        ),
        "/pkg/scene_bg.wasm" => serve_asset(
            &mut stream,
            &config.wasm.join("scene_bg.wasm"),
            "application/wasm",
            &headers,
        ),
        // Content-addressed, because a font never changes for a given id: the
        // page caches it under that URL and a reload costs nothing.
        "/fonts.json" => match &config.font {
            Some((path, id)) => {
                let kind = if path.extension().and_then(|value| value.to_str()) == Some("otf") {
                    "font/otf"
                } else {
                    "font/ttf"
                };
                let body =
                    format!("{{\"id\":\"{id}\",\"url\":\"/font/{id}\",\"kind\":\"{kind}\"}}");
                respond(
                    &mut stream,
                    200,
                    "application/json",
                    &body,
                    &[("cache-control", "no-store")],
                )
            }
            None => respond(
                &mut stream,
                404,
                "text/plain; charset=utf-8",
                "no font: pass --font <single-face .ttf or .otf>",
                &[],
            ),
        },
        path if path.starts_with("/font/") => match &config.font {
            Some((file, id)) if path == format!("/font/{id}") => serve_font(&mut stream, file, id),
            // A stale id is a font the browser kept from a previous host. Saying
            // so is how a cache gets invalidated without a human.
            _ => respond(
                &mut stream,
                404,
                "text/plain; charset=utf-8",
                "no font under that content id",
                &[],
            ),
        },
        "/scene.json" => {
            let width = query_value(query, "w").unwrap_or(1182).clamp(1, MAX_SIDE);
            let height = query_value(query, "h").unwrap_or(1569).clamp(1, MAX_SIDE);
            let key = CacheKey {
                stamp: modified(&config.snapshot)?,
                width,
                height,
                query: query_text(query, "q"),
                offset: query_value(query, "offset").unwrap_or(0),
                hover: query_text(query, "hover"),
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
                    None => match render_scene(config, &key) {
                        Ok(body) => {
                            *guard = Some((key.clone(), body.clone()));
                            body
                        }
                        // A dropped connection reaches the page as "Failed to
                        // fetch", which says nothing about the view. The status
                        // line can, so the render's own error is the response.
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
            // The page polls, and repainting a scene that has not moved is what
            // flicker is made of. The tag names everything that can change the
            // picture, so an unchanged scene costs a 304 and no redraw.
            let etag = format!(
                "\"{}-{width}-{height}-{}-{}-{}-{}\"",
                key.stamp,
                key.query,
                key.offset,
                key.hover,
                body.len()
            );
            if header(&headers, "if-none-match")
                .is_some_and(|value| value.split(',').any(|tag| tag.trim() == etag))
            {
                // Logged because "is it still flickering" is answered here: a poll
                // that revalidates says 304, and a poll that repaints says 200.
                eprintln!("worklog-serve: GET {path}?{query} 304 unchanged");
                return respond(&mut stream, 304, "application/json", "", &[("etag", &etag)]);
            }
            let asked = header(&headers, "if-none-match").unwrap_or("nothing");
            eprintln!("worklog-serve: GET {path}?{query} 200 sent (asked for {asked})");
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

/// A request header, case-insensitively, without its trailing CRLF.
fn header<'a>(headers: &'a str, name: &str) -> Option<&'a str> {
    headers.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

/// A build artifact the page imports. Its tag is its own stamp, so a rebuilt
/// module is fetched once and an unchanged one costs a 304 — the same bargain the
/// scene makes, on a file the host did not generate.
fn serve_asset(
    stream: &mut TcpStream,
    path: &Path,
    kind: &str,
    headers: &str,
) -> Result<(), String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            let message = format!(
                "{} is not built ({error})\n\n  cargo build -p soil-scene-gpu --lib \
                 --no-default-features --target wasm32-unknown-unknown\n  wasm-bindgen \
                 --target web --out-name scene --out-dir <this host's web/pkg> \
                 <the .wasm above>\n",
                path.display()
            );
            return respond(stream, 404, "text/plain; charset=utf-8", &message, &[]);
        }
    };
    let etag = format!("\"{}-{}\"", modified(path)?, bytes.len());
    let tags = [("etag", etag.as_str()), ("cache-control", "no-cache")];
    if header(headers, "if-none-match")
        .is_some_and(|value| value.split(',').any(|tag| tag.trim() == etag.as_str()))
    {
        return respond_bytes(stream, 304, kind, &[], &tags);
    }
    respond_bytes(stream, 200, kind, &bytes, &tags)
}

/// The font, under the id its bytes hash to.
///
/// Immutable for a given id: the page can cache it forever because a different
/// font is a different URL, which is what makes caching it a decision the host
/// makes rather than a bet the browser takes.
fn serve_font(stream: &mut TcpStream, path: &Path, id: &str) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let kind = match path.extension().and_then(|value| value.to_str()) {
        Some("otf") => "font/otf",
        _ => "font/ttf",
    };
    respond_bytes(
        stream,
        200,
        kind,
        &bytes,
        &[
            ("x-content-id", id),
            ("cache-control", "public, max-age=31536000, immutable"),
        ],
    )
}

/// The content id of a font's bytes.
fn font_id(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    Ok(Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// The face to render with when the operator names none.
///
/// Single-face files only. A `.ttc` is a collection, and `fontdue` reads one sfnt
/// — naming a collection here would render whichever weight the parser happened
/// to land on, which is a bug nobody would look for in a font list.
fn default_font() -> Option<PathBuf> {
    [
        // Small single faces first: a browser downloads this once per host, and
        // the 8 MB system variable face costs more than the page it draws.
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/System/Library/Fonts/Supplemental/Verdana.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        "/System/Library/Fonts/SFNS.ttf",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|path| path.exists())
}

fn query_value(query: &str, name: &str) -> Option<u32> {
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then(|| value.parse().ok()).flatten()
    })
}

/// A query parameter as text, percent-decoded enough for a search box: `%20` and
/// `+` are the two a browser actually sends, and a query containing `&` arrives
/// encoded, so decoding after splitting is correct.
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
fn render_scene(config: &Config, key: &CacheKey) -> Result<String, String> {
    // The renderer's output goes beside the process, not into whatever TMPDIR it
    // inherited: a service started from an agent's terminal inherits that
    // terminal's per-turn scratch directory, and reaping it mid-run turned every
    // later request into `No such file or directory`. Creating the directory each
    // time costs one syscall and makes the path self-healing.
    let scratch = std::env::temp_dir().join("worklog-serve");
    std::fs::create_dir_all(&scratch)
        .map_err(|error| format!("create {}: {error}", scratch.display()))?;
    let out = scratch.join(format!("scene-{}.json", std::process::id()));
    let mut command = Command::new("lua");
    command
        .env("FUNGOS_SDK", &config.sdk)
        .arg(&config.renderer)
        .arg(config.root.join("ui/worklog.lua"))
        .arg(&config.snapshot)
        .arg(&config.theme)
        .arg(&out)
        .arg(key.width.to_string())
        .arg(key.height.to_string());
    // The host state a view is allowed to read travels as flags, so the page and
    // the wall ask for a picture the same way.
    if !key.query.is_empty() {
        command.arg(format!("--query={}", key.query));
    }
    if key.offset > 0 {
        command.arg(format!("--scroll={}", key.offset));
    }
    if !key.hover.is_empty() {
        command.arg(format!("--hover={}", key.hover));
    }
    let result = command
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

fn respond(
    stream: &mut TcpStream,
    status: u16,
    kind: &str,
    body: &str,
    extra: &[(&str, &str)],
) -> Result<(), String> {
    respond_bytes(stream, status, kind, body.as_bytes(), extra)
}

fn respond_bytes(
    stream: &mut TcpStream,
    status: u16,
    kind: &str,
    body: &[u8],
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
    // A 304 carries no body and must not claim a length for one.
    if status != 304 {
        head.push_str(&format!("content-length: {}\r\n", body.len()));
    }
    for (name, value) in extra {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("connection: close\r\n\r\n");
    stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.write_all(body))
        .and_then(|()| stream.flush())
        .map_err(|error| format!("respond: {error}"))
}
