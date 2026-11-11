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

/// Animation cadence. The graph settles at a few ticks per frame and a focus
/// flies, so this only runs while something is moving.
const FRAME: Duration = Duration::from_millis(16);

/// Where the view's state lives between runs. The Spore cannot do this itself
/// (no filesystem grant, by design), so the host keeps it.
fn state_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join(".local/state/elmers-ui/view.json")
}

fn read_state(path: &PathBuf) -> Option<view::State> {
    let body = std::fs::read(path).ok()?;
    serde_json::from_slice(&body).ok()
}

fn write_state(path: &PathBuf, state: &view::State) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(body) = serde_json::to_vec_pretty(state) {
        let _ = std::fs::write(path, body);
    }
}

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
    let mut arguments_asked_for_a_view = false;
    let mut query: Option<String> = None;
    let mut focus: Option<String> = None;

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
            "--brain" => {
                start = View::Brain;
                arguments_asked_for_a_view = true;
            }
            "--query" => match args.next() {
                Some(text) => {
                    start = View::Brain;
                    arguments_asked_for_a_view = true;
                    query = Some(text);
                }
                None => fail("--query needs text"),
            },
            "--focus" => match args.next() {
                Some(name) => {
                    start = View::Brain;
                    arguments_asked_for_a_view = true;
                    focus = Some(name);
                }
                None => fail("--focus needs a node name"),
            },
            "--zoom" => match args.next().map(|value| value.parse::<f32>()) {
                Some(Ok(value)) => zoom = Some(value),
                _ => fail("--zoom needs a number"),
            },
            other => fail(&format!("unknown argument {other}")),
        }
    }

    application().run(move |cx: &mut App| {
        // The kit's globals first: theme, input handlers and the root that its
        // menus and popovers resolve their overlays through.
        gpui_component::init(cx);
        gpui_component::Theme::change(gpui_component::ThemeMode::Dark, None, cx);

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
            if let Some(saved) = read_state(&state_path()) {
                board.restore(saved);
            }
            if arguments_asked_for_a_view {
                board.view = start;
            }
            if let Some(zoom) = zoom {
                board.zoom_to(zoom);
            }
            board.apply_runs(source.runs());
            board.apply_graph(source.graph());
            if let Some(text) = query.clone() {
                board.open_query(view::Mode::Search);
                board.type_char(&text);
            }
            if let Some(name) = focus.clone()
                && let Some(node) = board.find_node(&name)
            {
                board.focus(node);
            }
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
            move |window, cx| cx.new(|cx| gpui_component::Root::new(root.clone(), window, cx)),
        )
        .expect("open the Elmers window");

        // The kit builds context menus later than the click; the rows need a
        // way back to this view.
        let attached = board.clone();
        board.update(cx, |board, _| board.attach(&attached));

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

        // A query bar owns the keyboard while it is open, otherwise `/` would
        // type a slash and then quit the app.
        if keys.read(cx).query_open() {
            match key.as_str() {
                "escape" => keys.update(cx, |board, cx| {
                    board.close_query();
                    cx.notify();
                }),
                "enter" => keys.update(cx, |board, cx| {
                    board.commit_query();
                    cx.notify();
                }),
                "backspace" => keys.update(cx, |board, cx| {
                    board.backspace();
                    cx.notify();
                }),
                "down" | "tab" => keys.update(cx, |board, cx| {
                    board.move_query_cursor(1);
                    cx.notify();
                }),
                "up" => keys.update(cx, |board, cx| {
                    board.move_query_cursor(-1);
                    cx.notify();
                }),
                _ => {
                    let typed = event
                        .keystroke
                        .key_char
                        .clone()
                        .filter(|text| !text.is_empty())
                        .unwrap_or_else(|| key.clone());
                    if typed.chars().count() == 1 && !event.keystroke.modifiers.modified() {
                        keys.update(cx, |board, cx| {
                            board.type_char(&typed);
                            cx.notify();
                        });
                    }
                }
            }
            return;
        }

        match key.as_str() {
            "/" | "shift-/slash" => keys.update(cx, |board, cx| {
                board.open_query(view::Mode::Search);
                cx.notify();
            }),
            ":" | "shift-semicolon" => keys.update(cx, |board, cx| {
                board.open_query(view::Mode::Command);
                cx.notify();
            }),
            "f" => keys.update(cx, |board, cx| {
                let picked = board.picked_node();
                board.focus(picked);
                cx.notify();
            }),
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

    // Anything that moves asks for the next frame until it stops: the graph
    // settling, a camera flying to a node.
    let frames = board.clone();
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(FRAME).await;
            let mut more = false;
            let _ = frames.update(cx, |board, _| {
                more = board.tick();
            });
            if more {
                let _ = frames.update(cx, |_, cx| cx.notify());
            }
        }
    })
    .detach();

    // The state file follows the hour hand, not the second hand: written when
    // something changed and at most once a second.
    let saver = board.clone();
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(FRAME * 60).await;
            let state = saver.update(cx, |board, _| {
                if board.take_dirty() {
                    Some(board.state())
                } else {
                    None
                }
            });
            if let Some(state) = state {
                write_state(&state_path(), &state);
            }
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
