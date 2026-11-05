//! Mock Mycelium + terminal host for the terminal Spore.
//!
//! This is the first executable vertical slice:
//! fixture directory -> fake PTY -> Spore VT parser -> host grid renderer.
//! Replace the fixture with the Mycelium adapter and the ASCII renderer with
//! the real terminal surface.

use std::collections::BTreeMap;

use anyhow::{Context, Result, anyhow, bail};
use terminal_core::ANSI_FIXTURE;
use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Engine, Store};

wasmtime::component::bindgen!({ path: "wit", world: "terminal-spore" });

use soil::mycelium::{directory, session};
use soil::terminal::surface;

const MAX_READ: u32 = 64 * 1024;
const MAX_FRAME_CELLS: usize = 4096;

struct State {
    targets: Vec<directory::Target>,
    sessions: BTreeMap<u64, MockSession>,
    views: BTreeMap<u64, MockView>,
    next_handle: u64,
    wasi: wasmtime_wasi::WasiCtx,
    table: wasmtime_wasi::ResourceTable,
}

struct MockSession {
    target: String,
    bytes: Vec<u8>,
    position: usize,
    closed: bool,
    cols: u32,
    rows: u32,
}

struct MockView {
    cols: u32,
    rows: u32,
    generation: u64,
    focused: bool,
    visible: bool,
}

impl wasmtime_wasi::WasiView for State {
    fn ctx(&mut self) -> wasmtime_wasi::WasiCtxView<'_> {
        wasmtime_wasi::WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

impl State {
    fn fixture() -> Self {
        Self {
            targets: vec![
                directory::Target {
                    id: "mycelium:agora-one".into(),
                    label: "agora-one".into(),
                    kind: "host".into(),
                    site: Some("agora".into()),
                    health: "healthy".into(),
                    shell: true,
                },
                directory::Target {
                    id: "mycelium:pris".into(),
                    label: "pris".into(),
                    kind: "host".into(),
                    site: Some("pris".into()),
                    health: "healthy".into(),
                    shell: true,
                },
                directory::Target {
                    id: "mycelium:studio".into(),
                    label: "studio".into(),
                    kind: "host".into(),
                    site: Some("studio".into()),
                    health: "degraded".into(),
                    shell: false,
                },
                directory::Target {
                    id: "mycelium:home-pi".into(),
                    label: "home-pi".into(),
                    kind: "service".into(),
                    site: Some("home".into()),
                    health: "healthy".into(),
                    shell: false,
                },
            ],
            sessions: BTreeMap::new(),
            views: BTreeMap::new(),
            next_handle: 1,
            wasi: wasmtime_wasi::WasiCtxBuilder::new().inherit_stdio().build(),
            table: wasmtime_wasi::ResourceTable::new(),
        }
    }

    fn handle(&mut self) -> u64 {
        let handle = self.next_handle;
        self.next_handle = self.next_handle.saturating_add(1);
        handle
    }

    fn target(&self, id: &str) -> Option<&directory::Target> {
        self.targets.iter().find(|target| target.id == id)
    }
}

impl directory::Host for State {
    fn targets(
        &mut self,
        kind: Option<String>,
        cursor: u64,
        limit: u32,
    ) -> Result<directory::Page, directory::Error> {
        let limit = usize::try_from(limit.min(64)).unwrap_or(64).max(1);
        let start = usize::try_from(cursor).map_err(|_| directory::Error::Invalid)?;
        let filtered: Vec<_> = self
            .targets
            .iter()
            .filter(|target| kind.as_ref().is_none_or(|kind| &target.kind == kind))
            .cloned()
            .collect();
        if start > filtered.len() {
            return Err(directory::Error::Invalid);
        }
        let end = start.saturating_add(limit).min(filtered.len());
        let next = (end < filtered.len()).then_some(end as u64);
        Ok(directory::Page {
            entries: filtered[start..end].to_vec(),
            next,
        })
    }

    fn get(&mut self, id: String) -> Result<directory::Target, directory::Error> {
        self.target(&id).cloned().ok_or(directory::Error::NotFound)
    }
}

impl session::Host for State {
    fn open_shell(&mut self, target: String, cols: u32, rows: u32) -> Result<u64, session::Error> {
        let entry = self.target(&target).ok_or(session::Error::UnknownTarget)?;
        if !entry.shell {
            return Err(session::Error::Denied);
        }
        if cols == 0 || rows == 0 || cols > 400 || rows > 200 {
            return Err(session::Error::OverBudget);
        }
        let handle = self.handle();
        self.sessions.insert(
            handle,
            MockSession {
                target,
                bytes: ANSI_FIXTURE.as_bytes().to_vec(),
                position: 0,
                closed: false,
                cols,
                rows,
            },
        );
        Ok(handle)
    }

    fn read(
        &mut self,
        session: u64,
        max_bytes: u32,
        _timeout_ms: u32,
    ) -> Result<Option<Vec<u8>>, session::Error> {
        let entry = self
            .sessions
            .get_mut(&session)
            .ok_or(session::Error::Closed)?;
        if entry.closed || entry.position >= entry.bytes.len() {
            entry.closed = true;
            return Ok(None);
        }
        let max = usize::try_from(max_bytes.min(MAX_READ))
            .unwrap_or(MAX_READ as usize)
            .max(1);
        let end = entry.position.saturating_add(max).min(entry.bytes.len());
        let chunk = entry.bytes[entry.position..end].to_vec();
        entry.position = end;
        Ok(Some(chunk))
    }

    fn write(&mut self, session: u64, bytes: Vec<u8>) -> Result<u32, session::Error> {
        let entry = self
            .sessions
            .get_mut(&session)
            .ok_or(session::Error::Closed)?;
        if entry.closed {
            return Err(session::Error::Closed);
        }
        Ok(u32::try_from(bytes.len()).unwrap_or(u32::MAX))
    }

    fn resize(&mut self, session: u64, cols: u32, rows: u32) -> Result<(), session::Error> {
        if cols == 0 || rows == 0 || cols > 400 || rows > 200 {
            return Err(session::Error::OverBudget);
        }
        let entry = self
            .sessions
            .get_mut(&session)
            .ok_or(session::Error::Closed)?;
        entry.cols = cols;
        entry.rows = rows;
        Ok(())
    }

    fn status(&mut self, session: u64) -> Result<session::State, session::Error> {
        let entry = self.sessions.get(&session).ok_or(session::Error::Closed)?;
        let phase = if entry.closed {
            session::Phase::Exited
        } else {
            session::Phase::Connected
        };
        Ok(session::State {
            phase,
            exit_code: Some(0),
            message: Some(format!(
                "mock PTY for {} ({}x{})",
                entry.target, entry.cols, entry.rows
            )),
        })
    }

    fn close(&mut self, session: u64) -> Result<(), session::Error> {
        let entry = self
            .sessions
            .get_mut(&session)
            .ok_or(session::Error::Closed)?;
        entry.closed = true;
        Ok(())
    }

    fn exec(
        &mut self,
        target: String,
        argv: Vec<String>,
        _timeout_ms: u32,
    ) -> Result<session::ExecResult, session::Error> {
        let entry = self.target(&target).ok_or(session::Error::UnknownTarget)?;
        if !entry.shell {
            return Err(session::Error::Denied);
        }
        let line = format!("mock exec on {target}: {}\n", argv.join(" "));
        Ok(session::ExecResult {
            code: 0,
            stdout: line.into_bytes(),
            stderr: Vec::new(),
        })
    }
}

impl surface::Host for State {
    fn open(&mut self, cols: u32, rows: u32) -> Result<u64, surface::Failure> {
        if cols == 0 || rows == 0 || (cols as usize) * (rows as usize) > MAX_FRAME_CELLS {
            return Err(surface::Failure::OverBudget);
        }
        let handle = self.handle();
        self.views.insert(
            handle,
            MockView {
                cols,
                rows,
                generation: 0,
                focused: true,
                visible: true,
            },
        );
        Ok(handle)
    }

    fn present(
        &mut self,
        view: u64,
        frame: surface::Frame,
    ) -> Result<surface::State, surface::Failure> {
        let entry = self.views.get_mut(&view).ok_or(surface::Failure::Closed)?;
        if frame.cols != entry.cols || frame.rows != entry.rows {
            return Err(surface::Failure::Invalid);
        }
        if frame.cells.len() != (frame.cols as usize) * (frame.rows as usize) {
            return Err(surface::Failure::Invalid);
        }
        if frame.generation < entry.generation {
            return Err(surface::Failure::Stale);
        }
        entry.generation = frame.generation;

        println!(
            "--- frame {}x{} cursor=({}, {}) visible={} title={:?} ---",
            frame.cols,
            frame.rows,
            frame.cursor.row,
            frame.cursor.column,
            frame.cursor.visible,
            frame.title
        );
        for row in 0..frame.rows {
            let mut line = String::new();
            for col in 0..frame.cols {
                let index = (row as usize) * (frame.cols as usize) + (col as usize);
                let cell = &frame.cells[index];
                line.push_str(if cell.text.is_empty() {
                    " "
                } else {
                    &cell.text
                });
            }
            println!("{}", line.trim_end());
        }

        Ok(surface::State {
            generation: entry.generation,
            cols: entry.cols,
            rows: entry.rows,
            focused: entry.focused,
            visible: entry.visible,
        })
    }

    fn next_input(
        &mut self,
        view: u64,
        _timeout_ms: u32,
    ) -> Result<Option<surface::Input>, surface::Failure> {
        if !self.views.contains_key(&view) {
            return Err(surface::Failure::Closed);
        }
        Ok(None)
    }

    fn observe(&mut self, view: u64) -> Result<surface::State, surface::Failure> {
        let entry = self.views.get(&view).ok_or(surface::Failure::Closed)?;
        Ok(surface::State {
            generation: entry.generation,
            cols: entry.cols,
            rows: entry.rows,
            focused: entry.focused,
            visible: entry.visible,
        })
    }

    fn close(&mut self, view: u64) -> Result<(), surface::Failure> {
        self.views
            .remove(&view)
            .map(|_| ())
            .ok_or(surface::Failure::Closed)
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let guest_args = if args.is_empty() {
        vec!["open-first".to_string()]
    } else {
        args
    };
    let module = std::env::var("TERMINAL_SPORE")
        .unwrap_or_else(|_| "spores/terminal/module.wasm".to_string());

    let engine = Engine::default();
    let component = Component::from_file(&engine, &module)
        .map_err(anyhow::Error::from)
        .with_context(|| format!("load component {module}"))?;

    let mut linker = Linker::<State>::new(&engine);
    wasmtime_wasi::p2::add_to_linker_sync::<State>(&mut linker)
        .map_err(anyhow::Error::from)
        .context("link wasi")?;
    directory::add_to_linker::<State, HasSelf<State>>(&mut linker, |state| state)
        .map_err(anyhow::Error::from)
        .context("link soil:mycelium/directory")?;
    session::add_to_linker::<State, HasSelf<State>>(&mut linker, |state| state)
        .map_err(anyhow::Error::from)
        .context("link soil:mycelium/session")?;
    surface::add_to_linker::<State, HasSelf<State>>(&mut linker, |state| state)
        .map_err(anyhow::Error::from)
        .context("link soil:terminal/surface")?;

    let mut store = Store::new(&engine, State::fixture());
    let bindings = TerminalSpore::instantiate(&mut store, &component, &linker)
        .map_err(anyhow::Error::from)
        .context("instantiate terminal Spore")?;

    match bindings.call_run(&mut store, &guest_args) {
        Ok(Ok(())) => {}
        Ok(Err(message)) => bail!("guest failed: {message}"),
        Err(error) => return Err(anyhow!("guest trapped: {error:#}")),
    }

    let snapshot = bindings
        .call_snapshot(&mut store)
        .map_err(anyhow::Error::from)?
        .map_err(|message| anyhow!("snapshot: {message}"))?;
    println!("{}", String::from_utf8_lossy(&snapshot));
    Ok(())
}
