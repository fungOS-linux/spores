//! Wasmtime host for the installed-Spore browser.

use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};
use spore_catalog::{LaunchTarget, Launcher, SporeCatalog, SporeEntry, SurfaceKind};
use wasmtime::component::HasSelf;
use wasmtime::{Engine, Store};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

wasmtime::component::bindgen!({ path: "wit", world: "spore-browser" });

struct State {
    catalog: SporeCatalog,
    launcher: SystemLauncher,
    wasi: WasiCtx,
    table: ResourceTable,
}

impl WasiView for State {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

impl soil::spore::catalog::Host for State {
    fn entries(&mut self) -> Result<Vec<soil::spore::catalog::Entry>, soil::spore::catalog::Error> {
        Ok(self
            .catalog
            .entries()
            .iter()
            .map(|entry| soil::spore::catalog::Entry {
                id: entry.id.clone(),
                name: entry.name.clone(),
                version: entry.version.clone(),
                summary: entry.summary.clone(),
                workspace: entry.workspace.clone(),
                default_surface: entry.manifest.launch.default.clone(),
                // A declared surface is a record now: the page needs its name
                // and artifact to open it, and its kind to say whether this
                // host can.
                surfaces: entry
                    .surfaces()
                    .into_iter()
                    .map(|surface| soil::spore::catalog::Surface {
                        name: surface.name,
                        kind: match surface.kind {
                            SurfaceKind::Canvas => soil::spore::catalog::SurfaceKind::Canvas,
                            SurfaceKind::Web => soil::spore::catalog::SurfaceKind::Web,
                        },
                        entry: surface.entry,
                        requires: surface.requires,
                    })
                    .collect(),
            })
            .collect())
    }

    fn launch(
        &mut self,
        id: String,
        surface: String,
    ) -> Result<soil::spore::catalog::Launched, soil::spore::catalog::Error> {
        let entry = self
            .catalog
            .get(&id)
            .cloned()
            .ok_or(soil::spore::catalog::Error::NotFound)?;
        // Which surfaces exist, and whether this host can present one, is the
        // manifest's answer -- not a second one written here.
        let target =
            entry
                .resolve(&surface, &SystemLauncher::KINDS)
                .map_err(|error| match error.code() {
                    "not-found" => soil::spore::catalog::Error::NotFound,
                    _ => soil::spore::catalog::Error::Unsupported,
                })?;
        let opened = self
            .launcher
            .launch(&entry, &target)
            .map_err(|_| soil::spore::catalog::Error::Unsupported)?;
        Ok(soil::spore::catalog::Launched {
            id,
            surface,
            target: opened,
        })
    }
}

struct SystemLauncher;

impl SystemLauncher {
    /// The kinds this host can present. A host without a browser view would
    /// leave `Web` out here and refuse those surfaces before opening anything.
    const KINDS: [SurfaceKind; 2] = [SurfaceKind::Canvas, SurfaceKind::Web];
}

impl Launcher for SystemLauncher {
    fn launch(&mut self, entry: &SporeEntry, target: &LaunchTarget<'_>) -> Result<String, String> {
        match target.kind {
            Some(SurfaceKind::Canvas) => {
                let workspace = entry
                    .workspace
                    .as_deref()
                    .ok_or_else(|| "Spore has no Canvas workspace".to_string())?;
                let mut child =
                    Command::new(std::env::var("CANVASCTL").unwrap_or_else(|_| "canvasctl".into()))
                        .stdin(Stdio::piped())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()
                        .map_err(|error| format!("start canvasctl: {error}"))?;
                child
                    .stdin
                    .as_mut()
                    .ok_or_else(|| "canvasctl stdin unavailable".to_string())?
                    .write_all(format!("workspace switch {workspace}\n").as_bytes())
                    .map_err(|error| format!("write canvasctl: {error}"))?;
                let status = child
                    .wait()
                    .map_err(|error| format!("wait canvasctl: {error}"))?;
                if !status.success() {
                    return Err(format!("canvasctl exited {status}"));
                }
                Ok(format!("workspace:{workspace}"))
            }
            Some(SurfaceKind::Web) => {
                let surface_id = entry
                    .workspace
                    .as_deref()
                    .unwrap_or(&entry.name)
                    .trim_end_matches("-dev");
                let url = format!("http://127.0.0.1:8088/?surface={surface_id}");
                open_url(&url)?;
                Ok(url)
            }
            None => Err(format!("unsupported launch surface {}", target.surface)),
        }
    }
}

#[cfg(target_os = "macos")]
fn open_url(url: &str) -> Result<(), String> {
    let status = Command::new("open")
        .arg(url)
        .status()
        .map_err(|error| format!("open {url}: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("open {url} exited {status}"))
}

#[cfg(not(target_os = "macos"))]
fn open_url(url: &str) -> Result<(), String> {
    let status = Command::new("xdg-open")
        .arg(url)
        .status()
        .map_err(|error| format!("xdg-open {url}: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("xdg-open {url} exited {status}"))
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1).peekable();
    let mut root = std::env::var("SPORE_ROOT").unwrap_or_else(|_| "spores".into());
    if args.peek().map(String::as_str) == Some("--root") {
        args.next();
        root = args.next().ok_or_else(|| anyhow!("--root needs a path"))?;
    }
    let command = args.next().unwrap_or_else(|| "list".into());
    let module = std::env::var("SPORE_BROWSER")
        .unwrap_or_else(|_| "spores/spore-browser/module.wasm".into());

    let catalog = SporeCatalog::scan(&root).context("scan Spore catalog")?;
    let engine = Engine::default();
    let component = wasmtime::component::Component::from_file(&engine, &module)
        .map_err(anyhow::Error::from)
        .with_context(|| format!("load component {module}"))?;
    let mut linker = wasmtime::component::Linker::<State>::new(&engine);
    wasmtime_wasi::p2::add_to_linker_sync::<State>(&mut linker)
        .map_err(anyhow::Error::from)
        .context("link wasi")?;
    soil::spore::catalog::add_to_linker::<State, HasSelf<State>>(&mut linker, |state| state)
        .map_err(anyhow::Error::from)
        .context("link soil:spore")?;
    let mut store = Store::new(
        &engine,
        State {
            catalog,
            launcher: SystemLauncher,
            wasi: WasiCtxBuilder::new().inherit_stdio().build(),
            table: ResourceTable::new(),
        },
    );
    let bindings = SporeBrowser::instantiate(&mut store, &component, &linker)
        .map_err(anyhow::Error::from)
        .context("instantiate launcher Spore")?;
    match command.as_str() {
        "list" => {
            let json = bindings
                .call_snapshot(&mut store)
                .map_err(anyhow::Error::from)?
                .map_err(|error| anyhow!("snapshot: {error}"))?;
            println!("{}", String::from_utf8_lossy(&json));
        }
        "launch" => {
            let id = args.next().ok_or_else(|| anyhow!("launch needs an id"))?;
            let surface = args
                .next()
                .ok_or_else(|| anyhow!("launch needs a surface"))?;
            let target = bindings
                .call_launch(&mut store, &id, &surface)
                .map_err(anyhow::Error::from)?
                .map_err(|error| anyhow!("launch: {error}"))?;
            println!("{target}");
        }
        other => bail!("unknown command {other}; use list or launch"),
    }
    Ok(())
}
