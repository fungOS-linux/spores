//! The Elmers board in a real window.
//!
//! A wasm component cannot open a window by itself: in production the Spore
//! renders offscreen and hands frames to the host through `soil:present`, and
//! the host owns the surface. This binary is the same view without a host — it
//! compiles `../src/view.rs` directly, so what appears here is the code the
//! Spore runs, with `soil:http` swapped for two files on disk.
//!
//! ```text
//! cargo run -- --runs /tmp/elmers-runs.json --graph /tmp/elmers-graph.json
//! cargo run -- --runs … --graph … --brain    # open on the graph
//! cargo run -- --brain --zoom 2.5            # …at a magnification
//! ```
//!
//! Keys are the Spore's: `1` runs, `2` brain, `j`/`k` move, `r` reload, `q` quit.

#[path = "../../src/brain.rs"]
mod brain;
#[path = "../../src/view.rs"]
mod view;

use std::{borrow::Cow, path::PathBuf, time::Duration};

use gpui::{App, AppContext as _, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};
use gpui_platform::application;
use view::{Board, View, parse_graph, parse_runs};

const LILEX: &[u8] = include_bytes!("../../assets/Lilex-Regular.ttf");
const LILEX_BOLD: &[u8] = include_bytes!("../../assets/Lilex-Bold.ttf");

/// The board is a live thing in the Spore, which refreshes on a slow tick; the
/// preview polls the files the same way, so a re-fetch in another terminal
/// shows up (and the footer's "refreshed" line keeps meaning something).
const TICK: Duration = Duration::from_secs(2);

/// Where the two responses come from. The Spore names two URLs; the preview
/// names two files, and nothing else about the view changes.
#[derive(Clone)]
struct Source {
    runs: PathBuf,
    graph: PathBuf,
}

impl Source {
    fn runs(&self) -> Result<Vec<view::Run>, String> {
        read(&self.runs).and_then(|body| parse_runs(&body))
    }

    fn graph(&self) -> Result<Vec<view::Workflow>, String> {
        read(&self.graph).and_then(|body| parse_graph(&body))
    }
}

fn read(path: &PathBuf) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))
}

/// Cheap enough to run every tick, and it keeps the footer honest: an unchanged
/// file is not a refresh.
fn stamp(path: &PathBuf) -> Option<(u64, std::time::SystemTime)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()?))
}

fn main() {
    let mut source = Source {
        runs: PathBuf::from("/tmp/elmers-runs.json"),
        graph: PathBuf::from("/tmp/elmers-graph.json"),
    };
    let mut start = View::Runs;
    let mut zoom: Option<f32> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--runs" => match args.next() {
                Some(path) => source.runs = path.into(),
                None => fail("--runs needs a path"),
            },
            "--graph" => match args.next() {
                Some(path) => source.graph = path.into(),
                None => fail("--graph needs a path"),
            },
            "--brain" => start = View::Brain,
            "--zoom" => match args.next().map(|value| value.parse::<f32>()) {
                Some(Ok(value)) => zoom = Some(value),
                _ => fail("--zoom needs a number"),
            },
            other => fail(&format!("unknown argument {other}")),
        }
    }

    application().run(move |cx: &mut App| {
        // The Spore embeds Lilex rather than trusting the host's fonts; the
        // preview does the same so both draw the same glyph shapes.
        if let Err(error) = cx
            .text_system()
            .add_fonts(vec![Cow::Borrowed(LILEX), Cow::Borrowed(LILEX_BOLD)])
        {
            eprintln!("fonts: {error:?}");
        }

        let board = cx.new(|_| {
            let mut board = Board::new();
            board.view = start;
            if let Some(zoom) = zoom {
                board.zoom_to(zoom);
            }
            board.apply_runs(source.runs());
            board.apply_graph(source.graph());
            board
        });

        let bounds = Bounds::centered(None, size(px(1180.0), px(760.0)), cx);
        let root = board.clone();
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Elmers".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |_, _cx| root.clone(),
        )
        .expect("open the Elmers window");

        bind_keys(cx, board, source);
        cx.activate(true);
    });
}

/// Keys go through the app rather than the focused element so the window never
/// needs a focus handshake to be usable, and so `q` works from the start.
fn bind_keys(cx: &mut App, board: gpui::Entity<Board>, source: Source) {
    let keys = board.clone();
    let key_source = source.clone();
    cx.observe_keystrokes(move |event, _window, cx| {
        let key = event.keystroke.key.to_ascii_lowercase();
        match key.as_str() {
            "q" => cx.quit(),
            "1" => keys.update(cx, |board, cx| {
                board.view = View::Runs;
                cx.notify();
            }),
            "2" => keys.update(cx, |board, cx| {
                board.view = View::Brain;
                // The graph changes when a workflow is edited, not on every
                // tick; reload it when the view is opened and it is stale.
                if board
                    .graph_at
                    .map(|at| at.elapsed().as_secs() >= 30)
                    .unwrap_or(true)
                {
                    board.apply_graph(key_source.graph());
                }
                cx.notify();
            }),
            "j" | "down" => keys.update(cx, |board, cx| {
                match board.view {
                    View::Runs => board.move_selection(1),
                    View::Brain => board.move_picked(1),
                }
                cx.notify();
            }),
            "k" | "up" => keys.update(cx, |board, cx| {
                match board.view {
                    View::Runs => board.move_selection(-1),
                    View::Brain => board.move_picked(-1),
                }
                cx.notify();
            }),
            "0" => keys.update(cx, |board, cx| {
                board.fit_graph();
                cx.notify();
            }),
            "r" => keys.update(cx, |board, cx| {
                match board.view {
                    View::Runs => board.apply_runs(key_source.runs()),
                    View::Brain => board.apply_graph(key_source.graph()),
                }
                cx.notify();
            }),
            _ => {}
        }
    })
    .detach();

    let tick_source = source;
    cx.spawn(async move |cx| {
        let mut seen = (stamp(&tick_source.runs), stamp(&tick_source.graph));
        loop {
            cx.background_executor().timer(TICK).await;
            let now = (stamp(&tick_source.runs), stamp(&tick_source.graph));
            if now.0 == seen.0 && now.1 == seen.1 {
                continue;
            }
            let runs = now.0 != seen.0;
            let graph = now.1 != seen.1;
            seen = now;
            let _ = board.update(cx, |board, cx| {
                if runs {
                    board.apply_runs(tick_source.runs());
                }
                if graph {
                    board.apply_graph(tick_source.graph());
                }
                cx.notify();
            });
        }
    })
    .detach();
}

fn fail(message: &str) -> ! {
    eprintln!("elmers-ui-preview: {message}");
    std::process::exit(2)
}
