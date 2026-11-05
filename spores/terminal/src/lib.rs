//! Terminal Spore: Mycelium directory + one live VT session.
//!
//! The host owns Mycelium identity/routing and terminal pixels. This component
//! owns the directory view, bounded session pumping, VT state and the cell grid.
//! It has no sockets, SSH client, credentials, local process, or filesystem.

wit_bindgen::generate!({ path: "wit", world: "terminal-spore", generate_all });

mod ghostty;

use serde::Serialize;
use soil::mycelium::{directory, session};
use soil::terminal::surface;
use std::cell::RefCell;
use std::sync::Mutex;
use terminal_core::Grid;

const MAX_TARGETS: u32 = 64;
const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
const READ_CHUNK: u32 = 64 * 1024;
const READ_TIMEOUT_MS: u32 = 50;
const MAX_READS: u32 = 4096;
const MAX_EMPTY_READS: u32 = 20;
const DEFAULT_COLS: u32 = 80;
const DEFAULT_ROWS: u32 = 24;

struct Component;
export!(Component);

static SNAPSHOT: Mutex<Option<Snapshot>> = Mutex::new(None);

thread_local! {
    static LIVE: RefCell<Option<Live>> = const { RefCell::new(None) };
}

struct Live {
    terminal: ghostty::Terminal,
    session: u64,
    view: u64,
    generation: u64,
    cols: u32,
    rows: u32,
}

#[derive(Debug, Clone, Serialize)]
struct Snapshot {
    targets: Vec<TargetView>,
    terminal: Option<TerminalView>,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct TargetView {
    id: String,
    label: String,
    kind: String,
    site: Option<String>,
    health: String,
    shell: bool,
}

#[derive(Debug, Clone, Serialize)]
struct TerminalView {
    cols: u32,
    rows: u32,
    text: String,
    cells: Vec<CellView>,
    cursor_row: u32,
    cursor_col: u32,
    cursor_visible: bool,
    title: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct CellView {
    text: String,
    width: u8,
    fg: u32,
    bg: u32,
    flags: u32,
}

impl Component {
    fn record(snapshot: Snapshot) {
        *SNAPSHOT.lock().expect("snapshot poisoned") = Some(snapshot);
    }

    fn empty() -> Snapshot {
        Snapshot {
            targets: Vec::new(),
            terminal: None,
            error: None,
        }
    }

    fn present_live() -> Result<(), String> {
        LIVE.with(|live| {
            let mut live = live.borrow_mut();
            let live = live.as_mut().ok_or("no live terminal")?;
            live.generation = live.generation.saturating_add(1);
            let grid = live.terminal.grid();
            let frame = frame_from_grid(&grid, live.generation);
            surface::present(live.view, &frame).map_err(|error| format!("present: {error:?}"))?;
            let mut snapshot = SNAPSHOT
                .lock()
                .expect("snapshot poisoned")
                .clone()
                .unwrap_or_else(Self::empty);
            snapshot.terminal = Some(TerminalView {
                cols: grid.cols,
                rows: grid.rows,
                text: grid.plain_text(),
                cells: grid
                    .cells
                    .iter()
                    .map(|cell| CellView {
                        text: cell.text.clone(),
                        width: cell.width,
                        fg: cell.style.fg,
                        bg: cell.style.bg,
                        flags: flags(cell),
                    })
                    .collect(),
                cursor_row: grid.cursor_row,
                cursor_col: grid.cursor_col,
                cursor_visible: grid.cursor_visible,
                title: grid.title,
            });
            *SNAPSHOT.lock().expect("snapshot poisoned") = Some(snapshot);
            Ok(())
        })
    }
}

impl Guest for Component {
    fn run(args: Vec<String>) -> Result<(), String> {
        let mut snap = Component::empty();

        let command = args.first().map(String::as_str).unwrap_or("open-first");
        match command {
            "list" => {
                snap.targets = list_targets()?;
            }
            "open" => {
                let Some(target) = args.get(1) else {
                    return Err("usage: open <target-id>".into());
                };
                snap.targets = list_targets().unwrap_or_default();
                open_target(&mut snap, target)?;
            }
            "open-first" => {
                snap.targets = list_targets()?;
                let Some(target) = snap.targets.iter().find(|t| t.shell) else {
                    return Err("no shell-capable target in the Mycelium directory".into());
                };
                let id = target.id.clone();
                open_target(&mut snap, &id)?;
            }
            other => return Err(format!("unknown command {other:?}")),
        }

        Component::record(snap);
        Ok(())
    }

    fn feed(bytes: Vec<u8>) -> Result<(), String> {
        LIVE.with(|live| {
            let mut live = live.borrow_mut();
            let live = live.as_mut().ok_or("no live terminal")?;
            live.terminal.feed(&bytes);
            Ok(())
        })
    }

    fn present() -> Result<(), String> {
        Component::present_live()
    }

    fn resize(cols: u32, rows: u32) -> Result<(), String> {
        if cols == 0 || rows == 0 || cols > 400 || rows > 200 {
            return Err("resize over budget".into());
        }
        LIVE.with(|live| {
            let mut live = live.borrow_mut();
            let live = live.as_mut().ok_or("no live terminal")?;
            live.terminal.resize(cols, rows);
            live.cols = cols;
            live.rows = rows;
            Ok(())
        })
    }

    fn stop() -> Result<(), String> {
        LIVE.with(|live| {
            if let Some(live) = live.borrow_mut().take() {
                let _ = session::close(live.session);
                let _ = surface::close(live.view);
            }
            Ok(())
        })
    }

    fn snapshot() -> Result<Vec<u8>, String> {
        let snapshot = SNAPSHOT
            .lock()
            .expect("snapshot poisoned")
            .clone()
            .unwrap_or_else(Component::empty);
        serde_json::to_vec(&snapshot).map_err(|error| format!("snapshot: {error}"))
    }
}

fn list_targets() -> Result<Vec<TargetView>, String> {
    let page = directory::targets(None, 0, MAX_TARGETS)
        .map_err(|error| format!("mycelium directory: {error:?}"))?;
    Ok(page
        .entries
        .into_iter()
        .map(|target| TargetView {
            id: target.id,
            label: target.label,
            kind: target.kind,
            site: target.site,
            health: target.health,
            shell: target.shell,
        })
        .collect())
}

fn open_target(snap: &mut Snapshot, target: &str) -> Result<(), String> {
    let view = surface::open(DEFAULT_COLS, DEFAULT_ROWS)
        .map_err(|error| format!("terminal surface: {error:?}"))?;
    let session = session::open_shell(target, DEFAULT_COLS, DEFAULT_ROWS)
        .map_err(|error| format!("open shell: {error:?}"))?;
    let terminal = ghostty::Terminal::new(DEFAULT_COLS, DEFAULT_ROWS)?;

    LIVE.with(|live| {
        *live.borrow_mut() = Some(Live {
            terminal,
            session,
            view,
            generation: 0,
            cols: DEFAULT_COLS,
            rows: DEFAULT_ROWS,
        });
    });

    let mut total = 0usize;
    let mut empty = 0u32;
    for _ in 0..MAX_READS {
        match session::read(session, READ_CHUNK, READ_TIMEOUT_MS) {
            Ok(Some(chunk)) => {
                if chunk.is_empty() {
                    empty = empty.saturating_add(1);
                    if empty >= MAX_EMPTY_READS {
                        break;
                    }
                    continue;
                }
                empty = 0;
                total = total.saturating_add(chunk.len());
                if total > MAX_FRAME_BYTES {
                    return Err("session exceeded the bounded frame budget".into());
                }
                Component::feed(chunk)?;
            }
            Ok(None) => break,
            Err(session::Error::Closed) => break,
            Err(error) => return Err(format!("read: {error:?}")),
        }
    }

    if let Ok(state) = session::status(session) {
        snap.error = state.message;
    }
    Component::present_live()?;
    if let Some(terminal) = SNAPSHOT
        .lock()
        .expect("snapshot poisoned")
        .as_ref()
        .and_then(|snapshot| snapshot.terminal.clone())
    {
        snap.terminal = Some(terminal);
    }
    Ok(())
}

fn flags(grid_cell: &terminal_core::Cell) -> u32 {
    let style = &grid_cell.style;
    (style.bold as u32)
        | ((style.italic as u32) << 1)
        | ((style.underline as u32) << 2)
        | ((style.reverse as u32) << 3)
        | ((style.dim as u32) << 4)
        | ((style.strikethrough as u32) << 5)
}

fn frame_from_grid(grid: &Grid, generation: u64) -> surface::Frame {
    let cells = grid
        .cells
        .iter()
        .map(|cell| surface::Cell {
            text: cell.text.clone(),
            width: cell.width,
            style: surface::Style {
                fg: cell.style.fg,
                bg: cell.style.bg,
                bold: cell.style.bold,
                italic: cell.style.italic,
                underline: cell.style.underline,
                reverse: cell.style.reverse,
                dim: cell.style.dim,
                strikethrough: cell.style.strikethrough,
            },
        })
        .collect();
    surface::Frame {
        cols: grid.cols,
        rows: grid.rows,
        cells,
        cursor: surface::Cursor {
            row: grid.cursor_row,
            column: grid.cursor_col,
            visible: grid.cursor_visible,
            shape: surface::CursorShape::Block,
        },
        title: grid.title.clone(),
        generation,
    }
}
