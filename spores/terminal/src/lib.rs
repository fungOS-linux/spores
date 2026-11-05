//! Terminal Spore: Mycelium directory + one VT session.
//!
//! The host owns Mycelium identity/routing and terminal pixels. This component
//! owns the directory view, bounded session pumping, VT state and the cell grid.
//! It has no sockets, SSH client, credentials, local process, or filesystem.

wit_bindgen::generate!({ path: "wit", world: "terminal-spore", generate_all });

use serde::Serialize;
use soil::mycelium::{directory, session};
use soil::terminal::surface;
use std::sync::Mutex;
use terminal_core::{Grid, render};

const MAX_TARGETS: u32 = 64;
const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
const READ_CHUNK: u32 = 64 * 1024;
const READ_TIMEOUT_MS: u32 = 200;
const MAX_READS: u32 = 4096;
const DEFAULT_COLS: u32 = 80;
const DEFAULT_ROWS: u32 = 24;

struct Component;
export!(Component);

static SNAPSHOT: Mutex<Option<Snapshot>> = Mutex::new(None);

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
    cursor_row: u32,
    cursor_col: u32,
    cursor_visible: bool,
    title: Option<String>,
}

impl Component {
    fn record(snapshot: Snapshot) {
        *SNAPSHOT.lock().expect("snapshot poisoned") = Some(snapshot);
    }
}

impl Guest for Component {
    fn run(args: Vec<String>) -> Result<(), String> {
        let mut snap = Snapshot {
            targets: Vec::new(),
            terminal: None,
            error: None,
        };

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

        Self::record(snap);
        Ok(())
    }

    fn snapshot() -> Result<Vec<u8>, String> {
        let snapshot = SNAPSHOT
            .lock()
            .expect("snapshot poisoned")
            .clone()
            .unwrap_or_else(|| Snapshot {
                targets: Vec::new(),
                terminal: None,
                error: None,
            });
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

    let mut bytes = Vec::new();
    for _ in 0..MAX_READS {
        match session::read(session, READ_CHUNK, READ_TIMEOUT_MS) {
            Ok(Some(chunk)) => {
                bytes.extend_from_slice(&chunk);
                if bytes.len() > MAX_FRAME_BYTES {
                    let _ = session::close(session);
                    return Err("session exceeded the bounded frame budget".into());
                }
            }
            Ok(None) => break,
            Err(session::Error::Closed) => break,
            Err(error) => {
                let _ = session::close(session);
                return Err(format!("read: {error:?}"));
            }
        }
    }

    let state = session::status(session).ok();
    let _ = session::close(session);

    let grid = render(&bytes, DEFAULT_COLS, DEFAULT_ROWS);
    let frame = frame_from_grid(&grid, 1);
    let presented =
        surface::present(view, &frame).map_err(|error| format!("present: {error:?}"))?;
    let _ = surface::close(view);

    snap.terminal = Some(TerminalView {
        cols: grid.cols,
        rows: grid.rows,
        text: grid.plain_text(),
        cursor_row: grid.cursor_row,
        cursor_col: grid.cursor_col,
        cursor_visible: grid.cursor_visible,
        title: grid.title,
    });
    if let Some(state) = state {
        snap.error = state.message;
    }
    let _ = presented;
    Ok(())
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
