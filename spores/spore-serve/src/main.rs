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

use std::collections::HashMap;
use std::io::{BufRead, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const INDEX_HTML: &str = include_str!("page.html");

/// How long a channel with no viewers keeps its last frame. Switching back to a
/// session you were just in should cost a socket, not a render.
const IDLE_LIMIT: Duration = Duration::from_secs(120);

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
    hub: Arc<Hub>,
    pool: Arc<Pool>,
}

enum SnapshotSource {
    File(PathBuf),
    Command(String),
    /// `http://` only, and only because this is a loopback producer: a URL here
    /// saves a `sh -c curl` per frame, which is most of the cost of a small
    /// snapshot.
    Url(String),
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut root = None;
    let mut view = None;
    let mut snapshot_file = None;
    let mut snapshot_cmd = None;
    let mut snapshot_url = None;
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
            // `http://host:port/path?{query}`: the same producer, without a
            // shell process per frame.
            "--snapshot-url" => snapshot_url = Some(value()?),
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
                     (--snapshot file.lua | --snapshot-cmd 'cmd' | --snapshot-url url) \
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
    let snapshot = match (snapshot_url, snapshot_cmd, snapshot_file) {
        (Some(url), _, _) => SnapshotSource::Url(url),
        (None, Some(command), _) => SnapshotSource::Command(command),
        (None, None, Some(path)) => SnapshotSource::File(path),
        (None, None, None) => {
            return Err("one of --snapshot, --snapshot-cmd or --snapshot-url is required".into());
        }
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
        hub: Arc::new(Hub::new()),
        pool: Arc::new(Pool::new()),
        title: title.unwrap_or_else(|| {
            let stem = view_file
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_string());
            stem.unwrap_or_else(|| "spore".to_string())
        }),
    });

    // Load the view into the renderers before anyone is waiting on it. A cold
    // pool would put the parse cost back on the first click, which is the thing
    // this is for.
    config.pool.warm(&config);

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

fn handle(mut stream: TcpStream, config: &Arc<Config>) -> Result<(), String> {
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
            // The shell is 22 KB of HTML and JS that changes only when this
            // binary does. `no-store` made every reload fetch it again; an ETag
            // makes a reload a 304, which is one round trip and no body.
            let etag = etag_for(page.as_bytes());
            let fresh = request.headers.iter().any(|(name, value)| {
                name.eq_ignore_ascii_case("if-none-match") && value.trim() == etag
            });
            if fresh {
                let head = format!(
                    "HTTP/1.1 304 Not Modified\r\netag: {etag}\r\ncache-control: no-cache\r\nconnection: close\r\n\r\n"
                );
                let _ = stream.write_all(head.as_bytes());
                return Ok(());
            }
            respond_page(&mut stream, page.as_bytes(), &etag)
        }
        ("GET", "/health") => respond(&mut stream, "200 OK", "text/plain; charset=utf-8", b"ok\n"),
        // Warm a channel without watching it.
        //
        // A click is preceded by a hover, and a hover is the only warning a host
        // gets. Rendering the session under the pointer means the click finds a
        // channel whose frame is already in memory instead of starting one.
        ("GET", "/warm") => {
            let width = query_number(&request.target, "w")
                .unwrap_or(config.width)
                .clamp(200, 4000);
            let height = query_number(&request.target, "h")
                .unwrap_or(config.height)
                .clamp(200, 4000);
            let passthrough = query_without(&request.target, &["w", "h", "sidebar"]);
            let sidebar = query_number(&request.target, "sidebar").unwrap_or(0);
            let key = channel_key(config, width, height, sidebar, &passthrough);
            let channel = config.hub.subscribe(key.clone());
            start_pump(config, &channel, width, height, sidebar, passthrough);
            warm(&channel);
            config.hub.unsubscribe(&key, &channel);
            respond(
                &mut stream,
                "204 No Content",
                "text/plain; charset=utf-8",
                b"",
            )
        }
        ("GET", "/scene.json") => {
            // A page asks for the surface it is drawing on: a terminal sized for
            // a 1200x900 authoring window and then stretched to a laptop screen
            // has cells in the wrong place and glyphs at the wrong size.
            let width = query_number(&request.target, "w").unwrap_or(config.width);
            let height = query_number(&request.target, "h").unwrap_or(config.height);
            // Everything but the surface size belongs to the producer: a session
            // id, a path, a query. `{query}` in `--snapshot-cmd` is how the
            // command sees it, so one host serves every session the producer has.
            let passthrough = query_without(&request.target, &["w", "h", "sidebar"]);
            // The sidebar offset is the page's scroll position. It belongs to the
            // renderer, not to the producer: the PTY has no sidebar.
            let sidebar = query_number(&request.target, "sidebar").unwrap_or(0);
            // The page's own viewport is the one number a blank screen hides.
            eprintln!("scene {width}x{height} sidebar={sidebar} {passthrough}");
            match scene(
                config,
                width.clamp(200, 4000),
                height.clamp(200, 4000),
                &passthrough,
                sidebar,
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
            String::from_utf8(output.stdout)
                .map_err(|error| format!("snapshot is not utf-8: {error}"))
        }
        SnapshotSource::Url(template) => {
            let url = template.replace("{query}", query);
            http_get(&url)
        }
    }
}

/// A loopback HTTP GET.
///
/// Deliberately not a general client: `http://` only, no redirects, no
/// chunked encoding — it exists so a producer on this machine can be asked for
/// a snapshot without a `sh` and a `curl` process in the way of every frame.
fn http_get(url: &str) -> Result<String, String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| format!("snapshot url must start with http://: {url}"))?;
    let (authority, path) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, "/"),
    };
    let address = if authority.contains(':') {
        authority.to_string()
    } else {
        format!("{authority}:80")
    };
    let mut stream = TcpStream::connect(&address)
        .map_err(|error| format!("snapshot url: connect {address}: {error}"))?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {authority}\r\nAccept: */*\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("snapshot url: send: {error}"))?;
    let mut raw = Vec::new();
    stream
        .read_to_end(&mut raw)
        .map_err(|error| format!("snapshot url: read: {error}"))?;
    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| "snapshot url: response has no headers".to_string())?;
    let head = String::from_utf8_lossy(&raw[..split]);
    let status = head.lines().next().unwrap_or("");
    if !status.contains(" 200") {
        return Err(format!("snapshot url: {status}"));
    }
    String::from_utf8(raw[split + 4..].to_vec())
        .map_err(|error| format!("snapshot is not utf-8: {error}"))
}

/// A `shot.lua --serve` process: the view and the SDK are already loaded, and it
/// waits on stdin for one frame at a time.
struct Renderer {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: std::io::BufReader<std::process::ChildStdout>,
}

impl Renderer {
    fn spawn(config: &Config) -> Result<Self, String> {
        let mut child = Command::new(&config.lua)
            .env("FUNGOS_SDK", &config.sdk)
            .arg(&config.renderer)
            .arg("--serve")
            .arg(&config.view)
            .arg(&config.theme)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // A view that fails to load should say so in this host's log rather
            // than into a pipe nobody reads.
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| format!("start renderer {}: {error}", config.lua))?;
        let stdin = child.stdin.take().ok_or("renderer has no stdin")?;
        let stdout = child.stdout.take().ok_or("renderer has no stdout")?;
        Ok(Self {
            child,
            stdin,
            stdout: std::io::BufReader::new(stdout),
        })
    }

    fn render(
        &mut self,
        snapshot: &str,
        width: u32,
        height: u32,
        sidebar: u32,
    ) -> Result<String, String> {
        writeln!(self.stdin, "{width} {height} {sidebar} {}", snapshot.len())
            .map_err(|error| format!("renderer: send header: {error}"))?;
        self.stdin
            .write_all(snapshot.as_bytes())
            .and_then(|_| self.stdin.flush())
            .map_err(|error| format!("renderer: send snapshot: {error}"))?;
        let mut header = String::new();
        self.stdout
            .read_line(&mut header)
            .map_err(|error| format!("renderer: read header: {error}"))?;
        let length: usize = header
            .trim()
            .parse()
            .map_err(|_| format!("renderer: bad reply {header:?}"))?;
        if length == 0 {
            let mut message = String::new();
            let _ = self.stdout.read_line(&mut message);
            return Err(format!("render failed: {}", message.trim()));
        }
        let mut body = vec![0u8; length];
        self.stdout
            .read_exact(&mut body)
            .map_err(|error| format!("renderer: read scene: {error}"))?;
        String::from_utf8(body).map_err(|error| format!("scene is not utf-8: {error}"))
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Idle renderers, shared by every channel.
///
/// Rendering used to be a process per frame, which is why a first visit felt
/// slow however fast the render itself was. These are started at boot and kept
/// warm; a frame is a write and a read.
struct Pool {
    idle: Mutex<Vec<Renderer>>,
    live: AtomicUsize,
}

/// Enough to render several channels at once without hoarding Lua images.
const MAX_RENDERERS: usize = 4;

impl Pool {
    fn new() -> Self {
        Self {
            idle: Mutex::new(Vec::new()),
            live: AtomicUsize::new(0),
        }
    }

    fn take(&self, config: &Config) -> Option<Renderer> {
        if let Ok(mut idle) = self.idle.lock() {
            if let Some(renderer) = idle.pop() {
                return Some(renderer);
            }
        }
        if self.live.load(Ordering::Acquire) >= MAX_RENDERERS {
            return None;
        }
        self.live.fetch_add(1, Ordering::AcqRel);
        match Renderer::spawn(config) {
            Ok(renderer) => Some(renderer),
            Err(error) => {
                self.live.fetch_sub(1, Ordering::AcqRel);
                eprintln!("spore-serve: {error}");
                None
            }
        }
    }

    fn put(&self, renderer: Renderer) {
        if let Ok(mut idle) = self.idle.lock() {
            idle.push(renderer);
        }
    }

    fn discard(&self, renderer: Renderer) {
        drop(renderer);
        self.live.fetch_sub(1, Ordering::AcqRel);
    }

    /// Start the pool before the first viewer asks for a frame. A cold pool is
    /// the exact cost this exists to remove.
    fn warm(self: &Arc<Self>, config: &Arc<Config>) {
        for _ in 0..MAX_RENDERERS {
            match self.take(config) {
                Some(renderer) => self.put(renderer),
                None => break,
            }
        }
    }
}

/// Render one snapshot through the Spore's view.
///
/// Render one snapshot, through whichever renderer is free.
///
/// A warm renderer is a write and a read; a cold one is a `lua` process plus
/// parsing the SDK and the view. The pool exists so that the first visit to a
/// session is a write and a read too.
fn render(
    config: &Config,
    snapshot: &str,
    width: u32,
    height: u32,
    sidebar: u32,
) -> Result<String, String> {
    if let Some(mut renderer) = config.pool.take(config) {
        match renderer.render(snapshot, width, height, sidebar) {
            Ok(body) => {
                config.pool.put(renderer);
                return Ok(body);
            }
            Err(error) => {
                // A renderer that cannot answer is not reusable, and keeping it
                // would fail every frame after it.
                config.pool.discard(renderer);
                return Err(error);
            }
        }
    }
    render_once(config, snapshot, width, height, sidebar)
}

/// The one-shot path: a process per frame, used when the pool is at its cap.
///
/// The snapshot goes in on stdin and the scene comes back on stdout. It used to
/// go through two files in `std::env::temp_dir()`, which is the wrong place on a
/// machine where `TMPDIR` is a scratch directory someone else deletes: a frame
/// would fail mid-render with "cannot open .../spore-serve-snapshot-*.lua" and
/// the page would sit on its last scene. Pipes cannot be swept out from under a
/// process, and they save four syscalls a frame.
fn render_once(
    config: &Config,
    snapshot: &str,
    width: u32,
    height: u32,
    sidebar: u32,
) -> Result<String, String> {
    let mut child = Command::new(&config.lua)
        .env("FUNGOS_SDK", &config.sdk)
        .arg(&config.renderer)
        .arg(&config.view)
        .arg("-")
        .arg(&config.theme)
        .arg("-")
        .arg(width.to_string())
        .arg(height.to_string())
        // Host state the view is allowed to read. A scroll offset is an input to
        // the picture, not a repaint of it: the same snapshot at two offsets is
        // two scenes, which is why it is part of the channel key too.
        .arg(format!("--sidebar={sidebar}"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("run {}: {error}", config.lua))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(snapshot.as_bytes())
            .map_err(|error| format!("send snapshot: {error}"))?;
    }
    let result = child
        .wait_with_output()
        .map_err(|error| format!("run {}: {error}", config.lua))?;

    if !result.status.success() {
        return Err(format!(
            "render failed: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        ));
    }
    String::from_utf8(result.stdout).map_err(|error| format!("scene is not utf-8: {error}"))
}

fn scene(
    config: &Config,
    width: u32,
    height: u32,
    query: &str,
    sidebar: u32,
) -> Result<String, String> {
    let snapshot = snapshot(config, query)?;
    render(config, &snapshot, width, height, sidebar)
}

/// Push a new scene whenever the producer's snapshot changes.
///
/// The page used to poll, which made every keystroke wait for the next tick and
/// every tick pay for a render whether or not anything had happened. This is the
/// same work, driven by the producer instead of a clock: read the snapshot (a
/// few kilobytes), compare, render only on change, push.
/// One renderer per (view, surface, producer query), shared by every viewer.
///
/// A render is the expensive part of a frame. Without this, two browsers on one
/// session mean two `lua` processes per change, and a third means three; with it
/// the second viewer costs a socket. The key is what the scene depends on: the
/// view, the surface size, and the producer query (a session id). Anything else
/// would serve a frame drawn for one surface to another.
struct Hub {
    channels: Mutex<HashMap<String, Arc<Channel>>>,
}

struct Channel {
    generation: Mutex<u64>,
    scene: Mutex<String>,
    /// The producer snapshot `scene` was rendered from. A viewer that comes
    /// back to this channel after everyone left gets the frame immediately and
    /// does not pay for a render that would produce the same bytes.
    snapshot: Mutex<String>,
    wake: Condvar,
    subscribers: AtomicUsize,
    stopped: AtomicBool,
    /// When the last subscriber left. A channel with nobody watching is kept
    /// for a while instead of dropped: switching back to a session you were
    /// just in is the common case, and a cold channel means a full render
    /// before the first pixel.
    idle: Mutex<Option<Instant>>,
}

impl Hub {
    fn new() -> Self {
        Self {
            channels: Mutex::new(HashMap::new()),
        }
    }

    fn subscribe(&self, key: String) -> Arc<Channel> {
        let mut channels = self.channels.lock().unwrap();
        channels.retain(|_, channel| {
            if channel.subscribers.load(Ordering::Acquire) > 0 {
                return true;
            }
            match *channel.idle.lock().unwrap() {
                Some(at) => at.elapsed() < IDLE_LIMIT,
                None => true,
            }
        });
        if let Some(channel) = channels.get(&key) {
            channel.subscribers.fetch_add(1, Ordering::AcqRel);
            channel.stopped.store(false, Ordering::Release);
            *channel.idle.lock().unwrap() = None;
            return Arc::clone(channel);
        }
        let channel = Arc::new(Channel {
            generation: Mutex::new(0),
            scene: Mutex::new(String::new()),
            snapshot: Mutex::new(String::new()),
            wake: Condvar::new(),
            subscribers: AtomicUsize::new(1),
            stopped: AtomicBool::new(false),
            idle: Mutex::new(None),
        });
        channels.insert(key, Arc::clone(&channel));
        channel
    }

    fn unsubscribe(&self, _key: &str, channel: &Arc<Channel>) {
        let left = channel
            .subscribers
            .fetch_sub(1, Ordering::AcqRel)
            .saturating_sub(1);
        if left == 0 {
            channel.stopped.store(true, Ordering::Release);
            channel.wake.notify_all();
            // Kept, not removed: the scene stays warm for a switch back. The
            // next `subscribe` sweeps it if nobody came.
            *channel.idle.lock().unwrap() = Some(Instant::now());
            if let Ok(mut channels) = self.channels.lock() {
                channels.retain(|_, other| {
                    if Arc::ptr_eq(other, channel) {
                        return true;
                    }
                    if other.subscribers.load(Ordering::Acquire) > 0 {
                        return true;
                    }
                    match *other.idle.lock().unwrap() {
                        Some(at) => at.elapsed() < IDLE_LIMIT,
                        None => true,
                    }
                });
            }
        }
    }
}

/// What a scene depends on, and therefore what one render can be shared by.
fn channel_key(config: &Config, width: u32, height: u32, sidebar: u32, query: &str) -> String {
    format!(
        "{}|{width}x{height}|{sidebar}|{query}",
        config.view.display()
    )
}

/// Start the render loop for a channel that has no pump yet.
fn start_pump(
    config: &Arc<Config>,
    channel: &Arc<Channel>,
    width: u32,
    height: u32,
    sidebar: u32,
    query: String,
) {
    if channel.subscribers.load(Ordering::Acquire) != 1 {
        return;
    }
    let cached = channel
        .snapshot
        .lock()
        .map(|snapshot| snapshot.clone())
        .unwrap_or_default();
    let last = if cached.is_empty() {
        None
    } else {
        Some(cached)
    };
    let config = Arc::clone(config);
    let pump_channel = Arc::clone(channel);
    std::thread::spawn(move || pump(config, pump_channel, width, height, sidebar, query, last));
}

/// Wait for a channel's first frame, but not forever: a hover that never becomes
/// a click should not hold the thread that served it.
fn warm(channel: &Arc<Channel>) {
    let deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < deadline {
        let ready = channel
            .scene
            .lock()
            .map(|scene| !scene.is_empty())
            .unwrap_or(false);
        if ready {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// The loop that owns this channel's renders.
fn pump(
    config: Arc<Config>,
    channel: Arc<Channel>,
    width: u32,
    height: u32,
    sidebar: u32,
    query: String,
    last: Option<String>,
) {
    let mut last = last;
    while !channel.stopped.load(Ordering::Acquire) {
        if let Ok(current) = snapshot(&config, &query)
            && last.as_deref() != Some(current.as_str())
        {
            match render(&config, &current, width, height, sidebar) {
                Ok(body) => {
                    if let Ok(mut scene) = channel.scene.lock() {
                        *scene = body;
                    }
                    // Remember what the frame was drawn from, so a viewer that
                    // comes back to an unchanged session is served the frame it
                    // already had instead of paying for an identical render.
                    if let Ok(mut kept) = channel.snapshot.lock() {
                        *kept = current.clone();
                    }
                    if let Ok(mut generation) = channel.generation.lock() {
                        *generation += 1;
                    }
                    channel.wake.notify_all();
                    last = Some(current);
                }
                Err(error) => eprintln!("spore-serve: render failed: {error}"),
            }
        }
        std::thread::sleep(Duration::from_millis(15));
    }
}

fn events(stream: &mut TcpStream, config: &Arc<Config>, target: &str) -> Result<(), String> {
    let width = query_number(target, "w")
        .unwrap_or(config.width)
        .clamp(200, 4000);
    let height = query_number(target, "h")
        .unwrap_or(config.height)
        .clamp(200, 4000);
    let passthrough = query_without(target, &["w", "h", "sidebar"]);
    let sidebar = query_number(target, "sidebar").unwrap_or(0);
    let key = channel_key(config, width, height, sidebar, &passthrough);

    stream
        .write_all(
            b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream; charset=utf-8\r\ncache-control: no-store\r\nconnection: keep-alive\r\nx-accel-buffering: no\r\n\r\n",
        )
        .map_err(|error| error.to_string())?;
    stream.flush().map_err(|error| error.to_string())?;

    let channel = config.hub.subscribe(key.clone());

    // A channel that kept its last frame paints before the producer is asked
    // anything. Switching back to a session you were just in is a socket and a
    // copy, not a render.
    let (cached_scene, generation) = {
        let scene = channel
            .scene
            .lock()
            .map(|scene| scene.clone())
            .unwrap_or_default();
        let generation = *channel
            .generation
            .lock()
            .map_err(|error| error.to_string())?;
        (scene, generation)
    };
    let mut seen = generation;
    if !cached_scene.is_empty() {
        let message = format!("data: {cached_scene}\n\n");
        if stream.write_all(message.as_bytes()).is_err() || stream.flush().is_err() {
            config.hub.unsubscribe(&key, &channel);
            return Ok(());
        }
    }

    if channel.subscribers.load(Ordering::Acquire) == 1 {
        start_pump(
            config,
            &channel,
            width,
            height,
            sidebar,
            passthrough.clone(),
        );
    }

    loop {
        let (generation, scene) = {
            let guard = channel
                .generation
                .lock()
                .map_err(|error| error.to_string())?;
            if *guard == seen {
                let _ = channel
                    .wake
                    .wait_timeout(guard, Duration::from_secs(15))
                    .map_err(|error| error.to_string())?;
                continue;
            }
            let generation = *guard;
            drop(guard);
            let scene = channel
                .scene
                .lock()
                .map(|scene| scene.clone())
                .unwrap_or_default();
            (generation, scene)
        };
        if scene.is_empty() {
            continue;
        }
        seen = generation;
        let message = format!("data: {scene}\n\n");
        if stream.write_all(message.as_bytes()).is_err() || stream.flush().is_err() {
            config.hub.unsubscribe(&key, &channel);
            return Ok(());
        }
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

/**
 * A stable tag for the page bytes. Not a cryptographic hash: it only has to
 * change when the page does, and a host that ships one binary per build has
 * nothing to collide with.
 */
fn etag_for(body: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in body {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("\"{:x}-{:x}\"", hash, body.len())
}

fn respond_page(stream: &mut TcpStream, body: &[u8], etag: &str) -> Result<(), String> {
    let head = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: {}\r\netag: {etag}\r\ncache-control: no-cache\r\nconnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(head.as_bytes())
        .map_err(|e| e.to_string())?;
    stream.write_all(body).map_err(|e| e.to_string())
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

#[allow(dead_code)]
fn exists(path: &Path) -> bool {
    path.is_file()
}
