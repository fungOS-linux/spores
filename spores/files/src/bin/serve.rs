//! files-serve: the `files` Spore in a browser, over a real namespace.
//!
//! It is a stand-in for Soil, not a second Spore. It supplies the `Vfs` (a
//! directory tree today, `soil:vfs` when a host links it), builds the flat
//! snapshot the module would build, renders it through the same Lua view Canvas
//! runs, and answers two GETs. What the browser draws is what the wall draws, at
//! whatever size the browser asks for.
//!
//! ```text
//! files-serve --vfs "$HOME/.local/share/fungos/vfs"
//! ```
//!
//! Point `files.myc` at it with a gateway route:
//!
//! ```json
//! { "name": "files", "hosts": ["files.myc"],
//!   "upstream": "http://127.0.0.1:8803", "service_kind": "files",
//!   "priority": 10, "dock": true }
//! ```
//!
//! The route's `service_kind` is the anycast half. With `--lease-file` the
//! server writes a renewable application lease into Mycelium's catalog, so the
//! same route can fall back to another machine running this service. The
//! installer turns that on and keeps the catalog fresh with `mycelium scan`.
//!
//! The desktop's loopback range is spoken for one service at a time — 8794 wiki,
//! 8795 worklog, 8796 the private-file gateway, 8797 SFX, 8800 store, 8801 home,
//! 8802 search — so this takes 8803, the next free shelf above them.
//!
//! Usage: files-serve [--vfs [name=]dir]... [--root name=path]... [--bind 127.0.0.1:8803]
//!                    [--sdk <dir>] [--theme <file>] [--renderer <file>]
//!                    [--max-entries N] [--max-depth N] [--refresh S]
//!                    [--snapshot <file>] [--once] [--path P] [--root-id ID]
//!                    [--lease-file F] [--lease-endpoint URL]...
//!                    [--lease-node ID] [--lease-label NAME]
//!                    [--lease-kind K] [--lease-ttl S] [--lease-refresh S]
//!                    [--lease-scan] [--mycelium-bin PATH]

#[path = "serve/lease.rs"]
mod lease;

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use files::scan::{Limits, Scan, Totals, scan};
use files::snapshot::{Root, Snapshot};
use files::vfs::StdFs;

/// One location and the directory being shown in it.
type ScanKey = (String, String);
/// Scans already paid for, reused until `--refresh` says the tree may have moved.
type ScanCache = Mutex<HashMap<ScanKey, (Instant, Arc<Scan>)>>;

/// The page is part of the binary: a view host with an install-dependent file
/// path would be a host that works only where it was built.
const PAGE: &str = include_str!("../../web/index.html");
const MAX_LINE: u64 = 8 << 10;
/// A browser asking for a scene is asking for a window, not for a poster.
const MAX_SIDE: u32 = 4096;
const DEFAULT_BIND: &str = "127.0.0.1:8803";
/// Scans kept warm at once. Every directory the reader opens is one entry; past
/// this the oldest is dropped so a long session cannot grow without a ceiling.
const MAX_CACHED_SCANS: usize = 32;

const USAGE: &str = "\
files-serve — serve the files view to a browser

  --vfs [name=]dir     a fungOS VFS workspace (default: $FUNGOS_HOME/vfs)
  --root name=path     a local directory (opt-in; not the VFS; repeatable)
  --bind <addr:port>   listen address (default 127.0.0.1:8803)
  --root-id <id>       location to open first (default: the first --root)
  --path <dir>         directory to open first (default: the location root)
  --sdk <dir>          fungos-sdk checkout holding lua/fungos/ui.lua (default $FUNGOS_SDK)
  --theme <file>       preview palette and type scale (default ../themes/graphite.lua)
  --renderer <file>    the scene renderer (default <repo>/tools/shot.lua)
  --spore-root <dir>   the directory holding ui/files.lua (default: this checkout)
  --lua <bin>          Lua interpreter (default \"lua\")
  --max-entries <n>    entries one scan may visit (default 100000)
  --max-depth <n>      directory depth one scan may enter (default 24)
  --refresh <secs>     how long a scan is reused before the tree is walked again (default 20)
  --snapshot <file>    render this snapshot instead of scanning (design previews)
  --once               print the snapshot Lua and exit (no server, no Lua needed)
  --lease-file <file>  publish a Mycelium application lease and keep it renewed
  --lease-kind <kind>  service kind for the lease (default files)
  --lease-node <id>    local Mycelium node id (default: ask the mycelium CLI)
  --lease-label <name> node label in the catalog (default: the hostname)
  --lease-endpoint <u> reachable endpoint (repeatable; default: loopback only)
  --lease-ttl <secs>   lease validity, clamped to 1..300 (default 300)
  --lease-refresh <s>  lease renewal interval (default 60)
  --lease-scan         run `mycelium scan` so the lease is observed (default off)
  --lease-scan-secs <s> scan interval (default 120)
  --mycelium-bin <bin> mycelium CLI (default $MYCELIUM_HOME/bin/mycelium)
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("files-serve: {error}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Clone)]
enum Backend {
    /// A local directory tree reached through `StdFs`. This is the opt-in
    /// fallback for a machine with no VFS; it is not the default.
    Directory(PathBuf),
    /// A `fungos-vfs` workspace. The namespace and its metadata live in
    /// `.fungos/vfs.sqlite3`, so a folder listing is a query, not a disk walk.
    Vfs(PathBuf),
}

impl Backend {
    fn path(&self) -> &Path {
        match self {
            Self::Directory(path) | Self::Vfs(path) => path,
        }
    }

    fn is_vfs(&self) -> bool {
        matches!(self, Self::Vfs(_))
    }
}

#[derive(Clone)]
struct Location {
    id: String,
    name: String,
    backend: Backend,
}

struct Config {
    bind: String,
    locations: Vec<Location>,
    start_root: String,
    start_path: String,
    sdk: Option<PathBuf>,
    theme: PathBuf,
    renderer: PathBuf,
    lua: String,
    limits: Limits,
    refresh: Duration,
    snapshot_file: Option<PathBuf>,
}

fn run(args: &[String]) -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut locations: Vec<Location> = Vec::new();
    for value in options(args, "--vfs") {
        let (name, path) = match value.split_once('=') {
            Some((name, path)) if !name.is_empty() => (name.to_string(), PathBuf::from(path)),
            Some(_) => return Err(format!("--vfs {value}: the name before `=` is empty")),
            None => ("VFS".to_string(), PathBuf::from(&value)),
        };
        let path = expand_home(&path);
        files::vfs_scan::Workspace::open(path.clone())
            .map_err(|error| format!("--vfs {value}: {error}"))?;
        let id = slug(&name);
        if locations.iter().any(|location| location.id == id) {
            return Err(format!(
                "--vfs {value}: two locations would share the id {id}"
            ));
        }
        locations.push(Location {
            id,
            name,
            backend: Backend::Vfs(path),
        });
    }
    for value in options(args, "--root") {
        let (name, path) = match value.split_once('=') {
            Some((name, path)) if !name.is_empty() => (name.to_string(), PathBuf::from(path)),
            Some(_) => return Err(format!("--root {value}: the name before `=` is empty")),
            None => {
                let path = PathBuf::from(&value);
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Files")
                    .to_string();
                (name, path)
            }
        };
        let path = expand_home(&path);
        if !path.is_dir() {
            return Err(format!(
                "--root {value}: {} is not a directory",
                path.display()
            ));
        }
        let id = slug(&name);
        if locations
            .iter()
            .any(|location: &Location| location.id == id)
        {
            return Err(format!(
                "--root {value}: two locations would share the id {id}"
            ));
        }
        locations.push(Location {
            id,
            name,
            backend: Backend::Directory(path),
        });
    }
    if locations.is_empty() {
        if let Some(path) = default_vfs_root() {
            locations.push(Location {
                id: "vfs".to_string(),
                name: "VFS".to_string(),
                backend: Backend::Vfs(path),
            });
        } else {
            let base = std::env::current_dir().map_err(|error| format!("cwd: {error}"))?;
            locations.push(Location {
                id: "files".to_string(),
                name: "Files".to_string(),
                backend: Backend::Directory(base),
            });
        }
    }

    let start_root = option(args, "--root-id").unwrap_or_else(|| locations[0].id.clone());
    if !locations.iter().any(|location| location.id == start_root) {
        return Err(format!("--root-id {start_root}: no such --root or --vfs"));
    }
    let limits = Limits {
        max_entries: number(args, "--max-entries")?
            .unwrap_or(files::scan::MAX_ENTRIES)
            .max(1),
        max_depth: number(args, "--max-depth")?
            .unwrap_or(u64::from(files::scan::MAX_DEPTH))
            .min(u64::from(u32::MAX)) as u32,
        ..Limits::default()
    };

    let config = Config {
        bind: option(args, "--bind").unwrap_or_else(|| DEFAULT_BIND.to_string()),
        locations,
        start_root,
        start_path: sanitize_path(&option(args, "--path").unwrap_or_default())?,
        // The renderer resolves `fungos.ui` through `FUNGOS_SDK`, and a service
        // that inherits whatever the shell happened to export breaks the first
        // time it is started from somewhere else. Name it once, here.
        // `--once` prints the snapshot and never renders, so it does not need
        // Lua or an SDK. Everything that draws does.
        sdk: option(args, "--sdk")
            .or_else(|| std::env::var("FUNGOS_SDK").ok())
            .map(PathBuf::from),
        theme: option(args, "--theme")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("../themes/graphite.lua")),
        renderer: option(args, "--renderer")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("../../tools/shot.lua")),
        lua: option(args, "--lua").unwrap_or_else(|| "lua".to_string()),
        limits,
        refresh: Duration::from_secs(number(args, "--refresh")?.unwrap_or(20).max(1)),
        snapshot_file: option(args, "--snapshot").map(PathBuf::from),
    };

    let once = args.iter().any(|arg| arg == "--once");
    if !once {
        let sdk = config
            .sdk
            .as_ref()
            .ok_or("no SDK: pass --sdk or set FUNGOS_SDK (the checkout with lua/fungos/ui.lua)")?;
        if !sdk.join("lua/fungos/ui.lua").exists() {
            return Err(format!(
                "{} has no lua/fungos/ui.lua — not an SDK checkout",
                sdk.display()
            ));
        }
        for path in [&config.theme, &config.renderer] {
            if !path.exists() {
                return Err(format!("{} does not exist", path.display()));
            }
        }
    }

    // A packaged install does not have this checkout: name the Spore directory
    // or the view is a path that went away when the build directory did.
    let spore_root = option(args, "--spore-root")
        .or_else(|| std::env::var("FILES_SPORE_ROOT").ok())
        .map(PathBuf::from)
        .unwrap_or(root);
    let view = spore_root.join("ui/files.lua");
    if !view.exists() {
        return Err(format!(
            "{} does not exist (pass --spore-root <dir> or set FILES_SPORE_ROOT)",
            view.display()
        ));
    }

    // `--once` is a census, not a service: it never publishes a lease.
    let lease = if once {
        None
    } else {
        lease_runtime(args, &config.bind)?
    };

    if once {
        // `--once` is a census, not a viewer: wait for every location rather
        // than print "scanning" and exit before the background scans land.
        let snapshot = build_snapshot(
            &config.locations,
            &config.start_root,
            &config.start_path,
            config.limits,
            &ScanSession {
                cache: Arc::new(Mutex::new(HashMap::new())),
                warming: Arc::new(Mutex::new(HashSet::new())),
                refresh: config.refresh,
                wait_for_all: true,
            },
        )?;
        print!("{}", snapshot.to_lua());
        return Ok(());
    }

    // Bind first, scan on the first request. A four-location tree can take long
    // enough to walk that an eager probe looks like a service that never
    // started; the gateway is the thing that retries, so it should get a socket
    // immediately.

    let listener = TcpListener::bind(&config.bind)
        .map_err(|error| format!("bind {}: {error}", config.bind))?;
    let where_ = config.snapshot_file.as_ref().map_or_else(
        || {
            config
                .locations
                .iter()
                .map(|location| format!("{}={}", location.name, location.backend.path().display()))
                .collect::<Vec<_>>()
                .join(", ")
        },
        |path| format!("snapshot {}", path.display()),
    );
    println!("files-serve: {where_} on http://{}", config.bind);

    if let Some((lease, scan, refresh)) = lease {
        println!(
            "files-serve: lease kind={} node={} endpoints={} path={}",
            lease.kind,
            lease.node_id,
            lease.endpoints.join(","),
            lease.path.display()
        );
        lease::start(lease, scan, refresh);
    }

    let state = Arc::new(State {
        config,
        scans: Arc::new(Mutex::new(HashMap::new())),
        warming: Arc::new(Mutex::new(HashSet::new())),
        scene: Mutex::new(None),
        view,
        counter: AtomicU64::new(0),
    });
    for connection in listener.incoming() {
        match connection {
            Ok(stream) => {
                let state = Arc::clone(&state);
                // A browser opens several connections at once, and a client that
                // connects and then stalls must cost its own thread and nothing
                // else.
                std::thread::spawn(move || {
                    if let Err(error) = handle(stream, &state) {
                        eprintln!("files-serve: {error}");
                    }
                });
            }
            Err(error) => eprintln!("files-serve: accept: {error}"),
        }
    }
    Ok(())
}

/// The lease and the scan that makes myceliumd observe it, plus the renewal
/// interval. Kept separate from `Config` because `--once` never publishes.
type LeaseRuntime = (lease::Lease, Option<lease::Scan>, Duration);

fn lease_runtime(args: &[String], bind: &str) -> Result<Option<LeaseRuntime>, String> {
    let Some(path) = option(args, "--lease-file") else {
        return Ok(None);
    };
    let kind = option(args, "--lease-kind").unwrap_or_else(|| "files".to_string());
    let bin = option(args, "--mycelium-bin")
        .map(PathBuf::from)
        .or_else(lease::default_bin);
    let node_id = match option(args, "--lease-node") {
        Some(node) if !node.is_empty() => node,
        _ => {
            let bin = bin
                .as_ref()
                .ok_or("--lease-file needs --lease-node or a mycelium CLI (--mycelium-bin)")?;
            lease::local_node_id(bin)?
        }
    };
    let mut endpoints = options(args, "--lease-endpoint");
    if endpoints.is_empty() {
        // A loopback lease is still useful: the local gateway is the observer
        // and may use it. Remote peers will ignore it, which is correct.
        endpoints.push(format!("http://127.0.0.1:{}", port_of(bind)));
    }
    let ttl_secs = number(args, "--lease-ttl")?.unwrap_or(lease::MAX_TTL_SECS);
    let refresh = Duration::from_secs(number(args, "--lease-refresh")?.unwrap_or(60).max(5));
    let scan = if args.iter().any(|arg| arg == "--lease-scan") {
        let bin = bin.ok_or("--lease-scan needs --mycelium-bin or a mycelium install")?;
        Some(lease::Scan {
            bin,
            every: Duration::from_secs(number(args, "--lease-scan-secs")?.unwrap_or(120).max(30)),
        })
    } else {
        None
    };
    Ok(Some((
        lease::Lease {
            path: PathBuf::from(path),
            service_id: format!("service/{node_id}/application/{kind}"),
            kind,
            node_id,
            label: Some(
                option(args, "--lease-label")
                    .filter(|label| !label.is_empty())
                    .unwrap_or_else(lease::hostname),
            ),
            endpoints,
            ttl_secs,
            application: option(args, "--lease-application")
                .unwrap_or_else(|| "dev.fungos.files".to_string()),
            version: option(args, "--lease-version")
                .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string()),
        },
        scan,
        refresh,
    )))
}

fn port_of(bind: &str) -> String {
    bind.rsplit(':')
        .next()
        .filter(|port| !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()))
        .unwrap_or("8803")
        .to_string()
}

struct State {
    config: Config,
    scans: Arc<ScanCache>,
    warming: Arc<Mutex<HashSet<ScanKey>>>,
    scene: Mutex<Option<(SceneKey, String)>>,
    view: PathBuf,
    counter: AtomicU64,
}

#[derive(Clone, PartialEq, Eq)]
struct SceneKey {
    fingerprint: u64,
    width: u32,
    height: u32,
}

fn handle(mut stream: TcpStream, state: &State) -> Result<(), String> {
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

    // The page asks whether the scene changed; answering that is what keeps it
    // from repainting a page that has not moved. The body of a GET is empty, so
    // reading past the headers would only wait out the read timeout.
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
    let (path, query) = target.split_once('?').unwrap_or((target, ""));

    match path {
        "/" | "/index.html" => respond(
            &mut stream,
            200,
            "text/html; charset=utf-8",
            PAGE,
            &[("cache-control", "no-store")],
        ),
        "/scene.json" => {
            let width = query_value(query, "w").unwrap_or(1182).clamp(1, MAX_SIDE);
            let height = query_value(query, "h").unwrap_or(1569).clamp(1, MAX_SIDE);
            let requested_root = query_text(query, "root");
            let requested_path = match query_text(query, "path") {
                Some(path) => match sanitize_path(&path) {
                    Ok(path) => path,
                    Err(error) => {
                        return respond(&mut stream, 400, "text/plain; charset=utf-8", &error, &[]);
                    }
                },
                None => state.config.start_path.clone(),
            };

            let (key, body) = match scene(state, width, height, requested_root, requested_path) {
                Ok(scene) => scene,
                Err(error) => {
                    // The render's own words, not a status code: a snapshot with
                    // a shape the view did not expect says so on stderr and the
                    // page shows it.
                    return respond(&mut stream, 500, "text/plain; charset=utf-8", &error, &[]);
                }
            };
            let etag = format!("\"{:016x}-{width}-{height}\"", key.fingerprint);
            if header(&headers, "if-none-match")
                .is_some_and(|value| value.split(',').any(|tag| tag.trim() == etag))
            {
                eprintln!("files-serve: GET {path}?{query} 304 unchanged");
                return respond(&mut stream, 304, "application/json", "", &[("etag", &etag)]);
            }
            eprintln!("files-serve: GET {path}?{query} 200 sent");
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

fn scene(
    state: &State,
    width: u32,
    height: u32,
    requested_root: Option<String>,
    requested_path: String,
) -> Result<(SceneKey, String), String> {
    let (snapshot, key) = match &state.config.snapshot_file {
        Some(path) => {
            let stamp = std::fs::metadata(path)
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |age| age.as_millis() as u64);
            (
                None,
                SceneKey {
                    fingerprint: stamp,
                    width,
                    height,
                },
            )
        }
        None => {
            let root_id = requested_root.unwrap_or_else(|| state.config.start_root.clone());
            if !state
                .config
                .locations
                .iter()
                .any(|location| location.id == root_id)
            {
                return Err(format!("no such location: {root_id}"));
            }
            let snapshot = build_snapshot(
                &state.config.locations,
                &root_id,
                &requested_path,
                state.config.limits,
                &ScanSession {
                    cache: Arc::clone(&state.scans),
                    warming: Arc::clone(&state.warming),
                    refresh: state.config.refresh,
                    wait_for_all: false,
                },
            )?;
            let key = SceneKey {
                fingerprint: snapshot.fingerprint(),
                width,
                height,
            };
            (Some(snapshot), key)
        }
    };

    {
        let cached = state
            .scene
            .lock()
            .map_err(|error| format!("scene lock: {error}"))?;
        if let Some((cached_key, body)) = cached.as_ref()
            && *cached_key == key
        {
            return Ok((key, body.clone()));
        }
    }

    let body = render_scene(state, snapshot.as_ref(), width, height)?;
    let mut cached = state
        .scene
        .lock()
        .map_err(|error| format!("scene lock: {error}"))?;
    *cached = Some((key.clone(), body.clone()));
    Ok((key, body))
}

/// The cache and the policy around it. Grouping these keeps the snapshot call
/// about the page, not about every knob the census happens to have today.
struct ScanSession {
    cache: Arc<ScanCache>,
    warming: Arc<Mutex<HashSet<ScanKey>>>,
    refresh: Duration,
    wait_for_all: bool,
}

/// One walk per location, reused for `refresh` seconds. The selected location is
/// walked for the directory being shown; the others are walked for their totals
/// alone, because a storage picker that names a location has to say how big it
/// is before it is opened.
fn build_snapshot(
    locations: &[Location],
    root_id: &str,
    path: &str,
    limits: Limits,
    session: &ScanSession,
) -> Result<Snapshot, String> {
    let cache = &session.cache;
    let warming = &session.warming;
    let refresh = session.refresh;
    let wait_for_all = session.wait_for_all;
    let selected = locations
        .iter()
        .find(|location| location.id == root_id)
        .ok_or_else(|| format!("no such location: {root_id}"))?;

    let mut roots = Vec::new();
    let mut selected_scan = None;
    let mut pending_warm = Vec::new();
    for location in locations {
        if location.id == selected.id {
            // The folder being read must exist before the page can show it, so
            // this one scan is synchronous.
            let scan = cached_scan(location, path, limits, cache, refresh)?;
            roots.push(Root {
                id: location.id.clone(),
                name: location.name.clone(),
                totals: scan.totals,
                available: true,
            });
            selected_scan = Some(scan);
            continue;
        }

        // A VFS root total is one indexed query, so it does not need the
        // rail's background-warm dance. It is also the namespace the page is
        // for, not a disk tree whose walk can outrun the gateway.
        if location.backend.is_vfs() {
            let scan = cached_scan(location, "", limits, cache, refresh)?;
            roots.push(Root {
                id: location.id.clone(),
                name: location.name.clone(),
                totals: scan.totals,
                available: true,
            });
            continue;
        }

        // Every other location is a number in the rail, not the page. Show the
        // last count while a fresh one is being taken, or "scanning" when there
        // is none. Blocking the first paint on four cold walks is how a page
        // takes longer than the gateway will wait.
        let key = (location.id.clone(), String::new());
        let cached = {
            let cache = cache
                .lock()
                .map_err(|error| format!("scan lock: {error}"))?;
            cache.get(&key).map(|(_, scan)| Arc::clone(scan))
        };
        if wait_for_all {
            let scan = cached_scan(location, "", limits, cache, refresh)?;
            roots.push(Root {
                id: location.id.clone(),
                name: location.name.clone(),
                totals: scan.totals,
                available: true,
            });
            continue;
        }
        match &cached {
            Some(scan) => roots.push(Root {
                id: location.id.clone(),
                name: location.name.clone(),
                totals: scan.totals,
                available: true,
            }),
            None => roots.push(Root {
                id: location.id.clone(),
                name: location.name.clone(),
                totals: Totals::default(),
                available: false,
            }),
        }
        let stale = {
            let cache = cache
                .lock()
                .map_err(|error| format!("scan lock: {error}"))?;
            cache
                .get(&key)
                .is_none_or(|(at, _)| at.elapsed() >= refresh)
        };
        if stale {
            pending_warm.push(location.clone());
        }
    }
    let scan = selected_scan.ok_or("the selected location produced no scan")?;
    // Start the background census only after the selected location has been
    // walked. Four cold walks beside the one the page is waiting for turn a
    // five-second first paint into a fifteen-second one.
    for location in pending_warm {
        warm_scan(location, "", limits, cache, warming);
    }
    let error = if !path.is_empty() && !scan.listing_seen {
        // An empty folder and a folder that is gone look identical in a total;
        // the view has to be able to tell the reader which one they are in.
        Some(format!("{path}: not found"))
    } else if scan.totals.scanned == 0 && scan.first_error.is_some() {
        scan.first_error.clone()
    } else {
        None
    };
    Ok(Snapshot::new(
        &selected.name,
        &selected.id,
        path,
        roots,
        &scan,
        now_ms(),
        error,
    ))
}

fn cached_scan(
    location: &Location,
    dir: &str,
    limits: Limits,
    cache: &Arc<ScanCache>,
    refresh: Duration,
) -> Result<Arc<Scan>, String> {
    let key = (location.id.clone(), dir.to_string());
    {
        let cache = cache
            .lock()
            .map_err(|error| format!("scan lock: {error}"))?;
        if let Some((at, scan)) = cache.get(&key)
            && at.elapsed() < refresh
        {
            return Ok(Arc::clone(scan));
        }
    }

    let scan = match &location.backend {
        Backend::Directory(base) => {
            let mut vfs = StdFs::new(base);
            Arc::new(scan(&mut vfs, "", dir, limits))
        }
        Backend::Vfs(root) => {
            let workspace = files::vfs_scan::Workspace::open(root.clone())?;
            Arc::new(workspace.scan(dir, limits)?)
        }
    };

    store_scan(cache, key, Arc::clone(&scan))?;
    Ok(scan)
}

fn store_scan(cache: &Arc<ScanCache>, key: ScanKey, scan: Arc<Scan>) -> Result<(), String> {
    let mut cache = cache
        .lock()
        .map_err(|error| format!("scan lock: {error}"))?;
    if cache.len() >= MAX_CACHED_SCANS
        && let Some(oldest) = cache
            .iter()
            .min_by_key(|(_, (at, _))| *at)
            .map(|(key, _)| key.clone())
    {
        cache.remove(&oldest);
    }
    cache.insert(key, (Instant::now(), scan));
    Ok(())
}

/// Take one location's totals in the background. The page keeps the last
/// number (or "scanning") until this lands, so a cold tree never becomes a
/// request that outlives the gateway's upstream timeout.
fn warm_scan(
    location: Location,
    dir: &str,
    limits: Limits,
    cache: &Arc<ScanCache>,
    warming: &Arc<Mutex<HashSet<ScanKey>>>,
) {
    let key = (location.id.clone(), dir.to_string());
    {
        let Ok(mut active) = warming.lock() else {
            return;
        };
        // One background walk at a time: a page poll every five seconds will
        // fill the rest of the rail, and the next reader's folder is never
        // queued behind four censuses at once.
        if !active.is_empty() || !active.insert(key.clone()) {
            return;
        }
    }
    let cache = Arc::clone(cache);
    let warming = Arc::clone(warming);
    let dir = dir.to_string();
    std::thread::spawn(move || {
        let result = match &location.backend {
            Backend::Directory(base) => {
                let mut vfs = StdFs::new(base);
                Some(scan(&mut vfs, "", &dir, limits))
            }
            // VFS censuses are synchronous and index-backed; this path is only
            // for the opt-in directory backend.
            Backend::Vfs(_) => None,
        };
        if let Some(result) = result {
            let _ = store_scan(&cache, key.clone(), Arc::new(result));
        }
        if let Ok(mut active) = warming.lock() {
            active.remove(&key);
        }
    });
}

/// Render through the view the wall runs. `tools/shot.lua` is the renderer: it
/// supplies the `canvas` ABI, resolves the SDK library and writes the scene
/// document. Anything this file drew itself would be a second renderer.
fn render_scene(
    state: &State,
    snapshot: Option<&Snapshot>,
    width: u32,
    height: u32,
) -> Result<String, String> {
    let stamp = state.counter.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir();
    let snapshot_path = match (snapshot, &state.config.snapshot_file) {
        (Some(snapshot), _) => {
            let path = dir.join(format!("files-snapshot-{}-{stamp}.lua", std::process::id()));
            std::fs::write(&path, snapshot.to_lua())
                .map_err(|error| format!("write {}: {error}", path.display()))?;
            Some(path)
        }
        (None, Some(path)) => Some(path.clone()),
        (None, None) => None,
    };
    let Some(snapshot_path) = snapshot_path else {
        return Err("no snapshot to render".to_string());
    };
    let out = dir.join(format!("files-scene-{}-{stamp}.json", std::process::id()));

    let sdk = state
        .config
        .sdk
        .as_ref()
        .ok_or("no SDK: pass --sdk or set FUNGOS_SDK")?;
    let result = Command::new(&state.config.lua)
        .env("FUNGOS_SDK", sdk)
        .arg(&state.config.renderer)
        .arg(&state.view)
        .arg(&snapshot_path)
        .arg(&state.config.theme)
        .arg(&out)
        .arg(width.to_string())
        .arg(height.to_string())
        .output()
        .map_err(|error| format!("run {}: {error}", state.config.lua))?;

    if snapshot.is_some() {
        let _ = std::fs::remove_file(&snapshot_path);
    }
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

fn option(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1).cloned())
}

fn options(args: &[String], name: &str) -> Vec<String> {
    args.iter()
        .enumerate()
        .filter(|(_, arg)| *arg == name)
        .filter_map(|(index, _)| args.get(index + 1).cloned())
        .collect()
}

fn number(args: &[String], name: &str) -> Result<Option<u64>, String> {
    option(args, name)
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| format!("{name} {value}: not a number"))
        })
        .transpose()
}

/// The standard user VFS workspace, if this machine has one.
fn default_vfs_root() -> Option<PathBuf> {
    let data = std::env::var_os("FUNGOS_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_DATA_HOME").map(|path| PathBuf::from(path).join("fungos"))
        })
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share/fungos"))
        })?;
    let root = data.join("vfs");
    root.join(".fungos/vfs.sqlite3").is_file().then_some(root)
}

fn expand_home(path: &Path) -> PathBuf {
    if let Ok(rest) = path.strip_prefix("~") {
        std::env::var_os("HOME")
            .map(|home| Path::new(&home).join(rest))
            .unwrap_or_else(|| path.to_path_buf())
    } else {
        path.to_path_buf()
    }
}

fn slug(name: &str) -> String {
    let mut id = String::new();
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            id.push(character.to_ascii_lowercase());
        } else if !id.ends_with('-') && !id.is_empty() {
            id.push('-');
        }
    }
    while id.ends_with('-') {
        id.pop();
    }
    if id.is_empty() {
        "files".to_string()
    } else {
        id
    }
}

/// A canonical location-relative path, or the reason it is not one.
fn sanitize_path(raw: &str) -> Result<String, String> {
    if raw.starts_with('/') {
        return Err("path must be relative to the location".to_string());
    }
    let mut parts = Vec::new();
    for part in raw.split('/') {
        match part {
            "" | "." => continue,
            ".." => return Err("path may not leave the location".to_string()),
            _ => parts.push(part),
        }
    }
    Ok(parts.join("/"))
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

fn query_text(query: &str, name: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then(|| percent_decode(value))
    })
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
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
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |age| age.as_millis() as u64)
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
        400 => "Bad Request",
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
        .and_then(|()| stream.write_all(body.as_bytes()))
        .and_then(|()| stream.flush())
        .map_err(|error| format!("respond: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_canonical_and_relative() {
        assert_eq!(sanitize_path("a//b/./c").unwrap(), "a/b/c");
        assert_eq!(sanitize_path("").unwrap(), "");
        assert!(sanitize_path("/etc").is_err(), "absolute is not relative");
        assert!(
            sanitize_path("a/../b").is_err(),
            "leaving the location is refused"
        );
    }

    #[test]
    fn location_ids_are_stable_and_never_empty() {
        assert_eq!(slug("My Studio"), "my-studio");
        assert_eq!(slug("studio"), "studio");
        assert_eq!(slug("///"), "files");
    }

    #[test]
    fn a_percent_encoded_path_decodes_once() {
        assert_eq!(percent_decode("a%20b%2Fc"), "a b/c");
        assert_eq!(percent_decode("a+b"), "a b");
        assert_eq!(percent_decode("100%"), "100%");
    }
}
