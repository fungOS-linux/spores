//! The board's view: pure GPUI, no transport.
//!
//! Shared by the wasm guest (which fetches through `soil:http`) and the native
//! preview (which fetches with ureq), so the thing you look at in a window is
//! the same code the Spore runs.

use gpui::{
    AnyElement, Context, IntoElement, Render, SharedString, Window, canvas, div, prelude::*, px,
    rgb,
};

const BG: u32 = 0x0b0f14;
const PANEL: u32 = 0x0f141b;
const ROW_HOVER: u32 = 0x1b2430;
const TEXT: u32 = 0xdbe4ee;
const MUTED: u32 = 0x6d7b8c;
const OK: u32 = 0x4f9e5a;
const BAD: u32 = 0xd14b4b;
const WAIT: u32 = 0xc9a227;

pub(crate) const HEADER_H: f32 = 44.0;
const FOOTER_H: f32 = 26.0;
pub(crate) const LIST_W: f32 = 420.0;
pub(crate) const ROW_H: f32 = 34.0;

#[derive(Clone, Debug)]
pub struct Run {
    id: String,
    workflow: String,
    status: String,
    detail: String,
    when: String,
}

impl Run {
    fn status_color(&self) -> u32 {
        match self.status.as_str() {
            "ok" => OK,
            "error" => BAD,
            _ => WAIT,
        }
    }
}

/// Parse the sidecar's `{ "runs": [ … ] }` defensively: the run store is the
/// other side's contract, and a field that moves should thin the board, not
/// blank it.
pub(crate) fn parse_runs(body: &[u8]) -> Result<Vec<Run>, String> {
    let value: serde_json::Value =
        serde_json::from_slice(body).map_err(|error| format!("runs is not JSON: {error}"))?;
    let list = value
        .get("runs")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "runs response has no 'runs' array".to_string())?;
    let mut runs = Vec::with_capacity(list.len());
    for entry in list {
        let text = |key: &str| {
            entry
                .get(key)
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        // Pretty-printed here rather than at paint time: this runs once per
        // response, and the detail pane is repainted on every frame of an
        // animation.
        let detail = {
            let error = text("error");
            if !error.is_empty() {
                error
            } else {
                let result = entry.get("result").cloned().unwrap_or_default();
                if result.is_null() {
                    String::new()
                } else {
                    serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string())
                }
            }
        };
        let when = entry.get("started_at").map(wall_clock).unwrap_or_default();
        runs.push(Run {
            id: text("id"),
            workflow: text("workflow"),
            status: text("status"),
            detail,
            when,
        });
    }
    // Newest first, which is the only order a board is read in.
    runs.reverse();
    Ok(runs)
}

/// A payload in a code block: monospace, its own surface, hairline border.
/// JSON is written to be read against its own indentation, so the pane gives it
/// the width and the background a terminal would.
fn code_block(body: &str) -> impl IntoElement {
    div()
        .mt(px(2.0))
        .p(px(8.0))
        .rounded(px(4.0))
        .bg(rgb(0x090d12))
        .border_1()
        .border_color(rgb(0x1f2933))
        .overflow_hidden()
        .child(
            div()
                .font_family("Lilex")
                .text_size(px(10.0))
                .text_color(rgb(0xb9c4d1))
                .child(SharedString::from(body.to_string())),
        )
}

/// The run store writes `started_at` as `"YYYY-MM-DD HH:MM:SS"`; a numeric
/// epoch second is also accepted, because that is what the field was when this
/// view was written. Both carry a wall clock, and the clock is all this shows —
/// the board has no calendar and no timezone.
fn wall_clock(value: &serde_json::Value) -> String {
    if let Some(secs) = value.as_i64() {
        let day = secs.rem_euclid(86_400);
        return format!("{:02}:{:02}", day / 3600, (day % 3600) / 60);
    }
    value
        .as_str()
        .and_then(|text| text.split_once(' '))
        .and_then(|(_, clock)| clock.get(0..5))
        .unwrap_or_default()
        .to_string()
}

/// Which surface the window is showing. Both read from the same sidecar.
#[derive(Clone, Copy, PartialEq)]
pub enum View {
    Runs,
    Brain,
}

/// One workflow as `GET /graph` describes it: identity, owner, and what it
/// calls. Everything the brain draws is in here.
pub struct Workflow {
    pub(crate) name: String,
    pub(crate) owner: String,
    /// Workflows this one runs — the graph's hand-offs.
    pub(crate) links: Vec<String>,
    /// Capabilities its steps call, deduplicated: an edge to each, once.
    pub(crate) calls: Vec<String>,
    /// Every step, called or not: the node's size is how much is behind it.
    pub(crate) steps: usize,
    pub(crate) model_spend: String,
    pub(crate) gated: bool,
}

pub(crate) fn parse_graph(body: &[u8]) -> Result<Vec<Workflow>, String> {
    let value: serde_json::Value =
        serde_json::from_slice(body).map_err(|error| format!("graph is not JSON: {error}"))?;
    let list = value
        .get("workflows")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "graph response has no 'workflows' array".to_string())?;
    let mut workflows = Vec::with_capacity(list.len());
    for entry in list {
        let text = |key: &str| {
            entry
                .get(key)
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let owner = entry
            .get("owners")
            .and_then(serde_json::Value::as_array)
            .and_then(|owners| owners.first())
            .and_then(serde_json::Value::as_str)
            .filter(|owner| !owner.is_empty())
            .unwrap_or("unowned")
            .to_string();
        let links = entry
            .get("links")
            .and_then(serde_json::Value::as_array)
            .map(|links| {
                links
                    .iter()
                    .filter_map(|link| link.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let steps = entry
            .get("steps")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut calls: Vec<String> = Vec::new();
        for step in &steps {
            let called = step.get("kind").and_then(serde_json::Value::as_str) == Some("call");
            let name = step.get("name").and_then(serde_json::Value::as_str);
            if let (true, Some(name)) = (called, name)
                && !calls.iter().any(|seen| seen == name)
            {
                calls.push(name.to_string());
            }
        }
        workflows.push(Workflow {
            name: text("name"),
            owner,
            links,
            calls,
            steps: steps.len(),
            model_spend: text("model_spend"),
            gated: entry
                .get("gated")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
        });
    }
    Ok(workflows)
}

/// What the query bar is doing. `Search` finds a node and flies to it;
/// `Command` runs one of a handful of verbs, so the picture can be driven
/// without hunting for a key.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Search,
    Command,
}

#[derive(Clone, Debug)]
pub struct Query {
    pub mode: Mode,
    pub text: String,
    /// Index into `matches`, moved with j/k or the arrows before `Enter`.
    pub cursor: usize,
    pub matches: Vec<usize>,
}

/// What outlives the process. The Spore itself is stateless by design — no
/// filesystem grant — so this is the host's to keep; the preview writes it to
/// disk and hands it back on the next run, and a hosted Spore could do the same
/// through whatever capability its host offers.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct State {
    pub brain: bool,
    pub zoom: f32,
    pub pan: (f32, f32),
    pub picked: usize,
}

pub struct Board {
    pub view: View,
    workflows: Vec<Workflow>,
    /// The picked node. The runs list has its own cursor (`selected`); this one
    /// indexes the brain's nodes, workflows and capabilities alike.
    picked: usize,
    /// The node under the pointer, if any.
    hover: Option<usize>,
    /// Where the graph is looking. Reset with `0`, which is the only way back
    /// from a zoom someone got lost in.
    camera: crate::brain::Camera,
    /// A pan in progress: where the drag began, and the camera pan at that
    /// moment. A release without movement is a click, not a drag.
    drag: Option<(gpui::Point<gpui::Pixels>, (f32, f32))>,
    /// The settled graph. Built when the graph arrives rather than when the tab
    /// is opened: the forces run once, and an unanswered tab is a blank canvas.
    brain: Option<std::rc::Rc<crate::brain::Brain>>,
    /// Where the graph was last painted, in window coordinates. Hit-testing
    /// needs it and only the paint callback knows it; a cell is cheaper than
    /// threading bounds through the element tree.
    graph_bounds: std::rc::Rc<std::cell::Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    graph_error: Option<SharedString>,
    /// The query bar, when it is open.
    pub query: Option<Query>,
    /// The right-click menu, when one is open. `menu.rs` owns how it looks;
    /// this owns what its rows do.
    pub menu: Option<crate::menu::Menu>,
    /// A camera move in flight, and the node it is heading for if the move has
    /// not been aimed yet (the canvas bounds are only known after a paint).
    camera_target: Option<crate::brain::Camera>,
    focus_target: Option<usize>,
    /// Set whenever something worth remembering changed, so the host can write
    /// the state out without this module touching a filesystem.
    dirty: bool,
    pub graph_at: Option<std::time::Instant>,
    pub runs: Vec<Run>,
    pub selected: usize,
    error: Option<SharedString>,
    pub fetched_at: Option<std::time::Instant>,
}

impl Default for Board {
    fn default() -> Self {
        Self::new()
    }
}

impl Board {
    pub fn new() -> Self {
        Self {
            view: View::Runs,
            workflows: Vec::new(),
            picked: 0,
            hover: None,
            camera: Default::default(),
            drag: None,
            brain: None,
            graph_bounds: Default::default(),
            graph_error: None,
            query: None,
            menu: None,
            camera_target: None,
            focus_target: None,
            dirty: false,
            graph_at: None,
            runs: Vec::new(),
            selected: 0,
            error: None,
            fetched_at: None,
        }
    }

    /// The caller fetches; the view decides what a result means. That keeps the
    /// transport (soil:http in the Spore, ureq in the native preview) out of the
    /// view entirely.
    pub fn apply_runs(&mut self, result: Result<Vec<Run>, String>) {
        match result {
            Ok(runs) => {
                let keep = self.selected.min(runs.len().saturating_sub(1));
                self.runs = runs;
                self.selected = keep;
                self.error = None;
            }
            Err(error) => self.error = Some(SharedString::from(error)),
        }
        self.fetched_at = Some(std::time::Instant::now());
    }

    /// The graph changes only when a workflow is edited, so it is fetched on
    /// demand rather than on the run refresh tick.
    pub fn apply_graph(&mut self, result: Result<Vec<Workflow>, String>) {
        match result {
            Ok(workflows) => {
                let brain = std::rc::Rc::new(crate::brain::Brain::build(&workflows));
                self.picked = self.picked.min(brain.node_count().saturating_sub(1));
                self.hover = None;
                self.brain = Some(brain);
                self.workflows = workflows;
                self.graph_error = None;
            }
            Err(error) => self.graph_error = Some(SharedString::from(error)),
        }
        self.graph_at = Some(std::time::Instant::now());
    }

    /// Back to the whole graph. The one key that recovers from a zoom someone
    /// got lost in, so it is advertised next to the zoom hint.
    pub fn fit_graph(&mut self) {
        self.camera = Default::default();
    }

    /// Open the graph at a given magnification. The preview uses it to prove
    /// the zoom renders, and it is the seam a host would use if it ever wanted
    /// to remember where someone was looking.
    pub fn zoom_to(&mut self, zoom: f32) {
        self.camera.zoom = zoom.clamp(crate::brain::Camera::MIN, crate::brain::Camera::MAX);
    }

    /// One frame of anything that moves: the graph settling, a camera flying.
    /// Returns whether another frame is wanted, so the host can stop asking.
    pub fn tick(&mut self) -> bool {
        let mut working = false;

        if let Some(brain) = self.brain.clone()
            && brain.settling()
        {
            let mut next = (*brain).clone();
            // A few ticks per frame: enough to settle in about a second, few
            // enough that the settling is something you can watch.
            next.step(6);
            self.brain = Some(std::rc::Rc::new(next));
            working = true;
        }

        // A focus is aimed here rather than when it is asked for, because only a
        // paint knows the canvas bounds — and it is re-aimed every frame while
        // the graph is still settling, or the camera would arrive at where the
        // node used to be.
        let settling = self
            .brain
            .as_ref()
            .map(|brain| brain.settling())
            .unwrap_or(false);
        if let Some(node) = self.focus_target
            && let Some(bounds) = self.graph_bounds.get()
            && let Some(brain) = self.brain.as_ref()
            && let Some((x, y)) = brain.position_of(node)
        {
            let zoom = self.camera.zoom.max(2.4);
            self.camera_target = Some(crate::brain::Camera::centred_on(x, y, zoom, bounds));
        }

        if let Some(target) = self.camera_target {
            self.camera = crate::brain::Camera::ease(self.camera, target, 0.22);
            if self.camera.arrived(target) {
                self.camera = target;
                self.camera_target = None;
                if !settling {
                    self.focus_target = None;
                }
            } else {
                working = true;
            }
            self.dirty = true;
        }
        working
    }

    /// Whether anything the host should persist has changed since it last asked.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    pub fn state(&self) -> State {
        let (pan_x, pan_y) = self.camera.pan;
        State {
            brain: self.view == View::Brain,
            zoom: self.camera.zoom,
            pan: (pan_x, pan_y),
            picked: self.picked,
        }
    }

    pub fn restore(&mut self, state: State) {
        self.view = if state.brain { View::Brain } else { View::Runs };
        self.camera = crate::brain::Camera {
            zoom: if state.zoom > 0.0 { state.zoom } else { 1.0 },
            pan: state.pan,
        };
        self.picked = state.picked;
    }

    pub fn open_query(&mut self, mode: Mode) {
        self.query = Some(Query {
            mode,
            text: String::new(),
            cursor: 0,
            matches: Vec::new(),
        });
        self.dirty = true;
    }

    pub fn close_query(&mut self) {
        self.query = None;
    }

    pub fn query_open(&self) -> bool {
        self.query.is_some()
    }

    /// A typed character. Search refilters on every keystroke; a command line
    /// does not, because its arguments are not node names.
    pub fn type_char(&mut self, text: &str) {
        let Some(query) = self.query.as_mut() else {
            return;
        };
        query.text.push_str(text);
        query.cursor = 0;
        let text = query.text.clone();
        if query.mode == Mode::Search {
            query.matches = self
                .brain
                .as_ref()
                .map(|brain| brain.find(&text))
                .unwrap_or_default();
        }
    }

    pub fn backspace(&mut self) {
        let Some(query) = self.query.as_mut() else {
            return;
        };
        query.text.pop();
        query.cursor = 0;
        let text = query.text.clone();
        if query.mode == Mode::Search {
            query.matches = self
                .brain
                .as_ref()
                .map(|brain| brain.find(&text))
                .unwrap_or_default();
        }
    }

    pub fn move_query_cursor(&mut self, delta: i32) {
        let Some(query) = self.query.as_mut() else {
            return;
        };
        if query.matches.is_empty() {
            return;
        }
        let max = query.matches.len() as i32 - 1;
        query.cursor = (query.cursor as i32 + delta).clamp(0, max) as usize;
    }

    /// `Enter`: fly to the highlighted match, or run the typed command.
    pub fn commit_query(&mut self) {
        let Some(query) = self.query.take() else {
            return;
        };
        match query.mode {
            Mode::Search => {
                if let Some(&node) = query.matches.get(query.cursor) {
                    self.focus(node);
                }
            }
            Mode::Command => self.run_command(&query.text),
        }
        self.dirty = true;
    }

    /// The command vocabulary is deliberately tiny: the ones a person reaches
    /// for while looking at a graph, not a second interface.
    fn run_command(&mut self, text: &str) {
        let mut words = text.split_whitespace();
        match words.next().unwrap_or_default() {
            "runs" => self.view = View::Runs,
            "brain" => self.view = View::Brain,
            "fit" => self.camera = Default::default(),
            "zoom" => {
                if let Some(zoom) = words.next().and_then(|value| value.parse::<f32>().ok()) {
                    self.zoom_to(zoom);
                }
            }
            "focus" => {
                let rest = text.trim_start_matches("focus").trim();
                if let Some(node) = self
                    .brain
                    .as_ref()
                    .and_then(|brain| brain.find(rest).first().copied())
                {
                    self.focus(node);
                }
            }
            _ => {}
        }
    }

    /// Open a menu for a node at a window position.
    pub fn open_node_menu(&mut self, node: Option<usize>, at: gpui::Point<gpui::Pixels>) {
        let title = node
            .and_then(|node| self.brain.as_ref().and_then(|brain| brain.name_of(node)))
            .unwrap_or("graph")
            .to_string();
        let mut menu = crate::menu::Menu::new(at, title)
            .item("focus", "Focus", "f")
            .item("zoom-in", "Zoom in", "scroll")
            .item("zoom-out", "Zoom out", "")
            .item("fit", "Fit graph", "0")
            .item("runs", "Show runs", "1");
        if node.is_none() {
            menu = crate::menu::Menu::new(at, "graph").item("fit", "Fit graph", "0");
        }
        if let Some(node) = node {
            self.picked = node;
        }
        // Keep the menu on screen. The canvas is the only rectangle this view
        // knows, and it is the one the menu is opened over.
        if let Some(bounds) = self.graph_bounds.get() {
            let bottom = f32::from(bounds.origin.y + bounds.size.height);
            let overflow = f32::from(at.y) + menu.height() - bottom;
            if overflow > 0.0 {
                menu.at.y = at.y - px(overflow);
            }
        }
        self.menu = Some(menu);
        self.dirty = true;
    }

    /// Open a menu for a run, from the list or the graph's panel.
    pub fn open_run_menu(&mut self, run: usize, at: gpui::Point<gpui::Pixels>) {
        let title = self
            .runs
            .get(run)
            .map(|run| run.workflow.clone())
            .unwrap_or_default();
        self.selected = run;
        let mut menu = crate::menu::Menu::new(at, title)
            .item("open", "Open run", "enter")
            .item("copy", "Copy run id", "");
        if let Some(bounds) = self.graph_bounds.get() {
            let bottom = f32::from(bounds.origin.y + bounds.size.height);
            let overflow = f32::from(at.y) + menu.height() - bottom;
            if overflow > 0.0 {
                menu.at.y = at.y - px(overflow);
            }
        }
        self.menu = Some(menu);
        self.dirty = true;
    }

    pub fn close_menu(&mut self) {
        self.menu = None;
    }

    /// Run the row a menu reported. Ids are matched here rather than in
    /// `menu.rs`, because only the view knows what it can do.
    pub fn run_menu(&mut self, id: &str, cx: &mut gpui::App) {
        match id {
            "focus" => {
                let node = self.picked_node();
                self.focus(node);
            }
            "zoom-in" => self.zoom_to(self.camera.zoom * 1.5),
            "zoom-out" => self.zoom_to(self.camera.zoom / 1.5),
            "fit" => self.fit_graph(),
            "runs" | "open" => self.view = View::Runs,
            "copy" => {
                if let Some(run) = self.runs.get(self.selected) {
                    use gpui::ClipboardItem;
                    cx.write_to_clipboard(ClipboardItem::new_string(run.id.clone()));
                }
            }
            _ => {}
        }
        self.menu = None;
        self.dirty = true;
    }

    /// Pick a node and fly the camera to it.
    pub fn focus(&mut self, node: usize) {
        self.picked = node;
        self.focus_target = Some(node);
        self.dirty = true;
    }

    /// The node a name resolves to, best match first. The preview uses it to
    /// open on a node; `focus` in the command bar uses the same ranking.
    pub fn find_node(&self, name: &str) -> Option<usize> {
        self.brain.as_ref()?.find(name).first().copied()
    }

    /// The node the keyboard is on, for `f` and for the footer.
    pub fn picked_node(&self) -> usize {
        self.brain
            .as_ref()
            .map(|brain| self.picked.min(brain.node_count().saturating_sub(1)))
            .unwrap_or(0)
    }

    pub fn move_picked(&mut self, delta: i32) {
        let count = self
            .brain
            .as_ref()
            .map(|brain| brain.node_count())
            .unwrap_or(0);
        if count == 0 {
            return;
        }
        let max = count as i32 - 1;
        self.picked = (self.picked as i32 + delta).clamp(0, max) as usize;
    }

    pub fn move_selection(&mut self, delta: i32) {
        if self.runs.is_empty() {
            return;
        }
        let max = self.runs.len() as i32 - 1;
        let next = (self.selected as i32 + delta).clamp(0, max);
        self.selected = next as usize;
    }

    fn counts(&self) -> (usize, usize, usize) {
        let mut ok = 0;
        let mut bad = 0;
        let mut other = 0;
        for run in &self.runs {
            match run.status.as_str() {
                "ok" => ok += 1,
                "error" => bad += 1,
                _ => other += 1,
            }
        }
        (ok, bad, other)
    }

    fn tab(&self, label: &'static str, view: View, key: &'static str) -> impl IntoElement {
        let active = self.view == view;
        div()
            .px(px(8.0))
            .py(px(3.0))
            .rounded(px(6.0))
            .bg(rgb(if active { ROW_HOVER } else { PANEL }))
            .text_size(px(11.0))
            .text_color(rgb(if active { TEXT } else { MUTED }))
            .child(SharedString::from(format!("{label} {key}")))
    }

    fn header(&self) -> impl IntoElement {
        let (ok, bad, other) = self.counts();
        div()
            .h(px(HEADER_H))
            .px(px(14.0))
            .flex()
            .items_center()
            .justify_between()
            .bg(rgb(PANEL))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgb(TEXT))
                            .child("Elmers"),
                    )
                    .child(self.tab("runs", View::Runs, "1"))
                    .child(self.tab("brain", View::Brain, "2")),
            )
            .child(match (self.view, &self.brain) {
                (View::Runs, _) => div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(OK))
                            .child(format!("{ok} ok")),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(BAD))
                            .child(format!("{bad} error")),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(WAIT))
                            .child(format!("{other} other")),
                    )
                    .into_any_element(),
                (View::Brain, Some(brain)) => {
                    let (workflows, capabilities) = brain.summary();
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(rgb(WAIT))
                                .child(format!("{} domains", brain.station_count())),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(rgb(TEXT))
                                .child(format!("{workflows} workflows")),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(rgb(MUTED))
                                .child(format!("{capabilities} capabilities")),
                        )
                        .into_any_element()
                }
                (View::Brain, None) => div().into_any_element(),
            })
    }

    fn list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(LIST_W))
            // The detail pane holds a single un-wrapped line of JSON whose
            // min-content width is wider than the window. Without this the
            // list is the sibling flexbox shrinks, and it disappears.
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .overflow_hidden()
            .children(self.runs.iter().enumerate().map(|(index, run)| {
                let selected = index == self.selected;
                let mut row = div()
                    .id(("run", index))
                    .h(px(ROW_H))
                    .w_full()
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0));
                if selected {
                    row = row.bg(rgb(ROW_HOVER));
                }
                row.child(
                    div()
                        .w(px(6.0))
                        .h(px(6.0))
                        .rounded(px(3.0))
                        .bg(rgb(run.status_color())),
                )
                .child(
                    div()
                        .flex_1()
                        .text_size(px(12.0))
                        .text_color(rgb(if selected { TEXT } else { 0xb9c4d1 }))
                        .child(SharedString::from(run.workflow.clone())),
                )
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(rgb(MUTED))
                        .child(SharedString::from(run.when.clone())),
                )
                // A click selects; nothing here starts or stops work, so this is
                // the whole of the list's behaviour.
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(move |this, _: &gpui::MouseDownEvent, _window, cx| {
                        this.selected = index;
                        this.close_menu();
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    gpui::MouseButton::Right,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, _window, cx| {
                        this.open_run_menu(index, event.position);
                        cx.notify();
                    }),
                )
                .into_any_element()
            }))
    }

    fn detail(&self) -> impl IntoElement {
        let selected = self.runs.get(self.selected);
        let mut panel = div()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .bg(rgb(BG));
        match selected {
            Some(run) => {
                panel = panel
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .text_color(rgb(TEXT))
                                    .child(SharedString::from(run.workflow.clone())),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(rgb(run.status_color()))
                                    .child(SharedString::from(run.status.clone())),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(rgb(MUTED))
                                    .child(SharedString::from(run.when.clone())),
                            ),
                    )
                    .child(code_block(&run.detail));
            }
            None => {
                panel = panel.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(MUTED))
                        .child("no runs"),
                );
            }
        }
        if let Some(error) = &self.error {
            panel = panel.child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(BAD))
                    .child(error.clone()),
            );
        }
        panel
    }

    /// The brain: the force layout from `brain.rs`, painted by a canvas, with a
    /// panel that a click opens.
    ///
    /// `brain.rs` owns the layout, the colours, the labels and the hit test; this
    /// function only translates pointer events into a picked node and gives the
    /// detail panel a place to stand.
    fn brain_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(brain) = &self.brain else {
            return div()
                .flex_1()
                .p(px(12.0))
                .child(div().text_size(px(12.0)).text_color(rgb(MUTED)).child(
                    match &self.graph_error {
                        Some(error) => error.clone(),
                        None => SharedString::from("loading the graph…"),
                    },
                ))
                .into_any_element();
        };

        // The paint callback is the only place the graph's bounds are known, so
        // it records them for the hit test that the next mouse event runs.
        let bounds = self.graph_bounds.clone();
        let graph = brain.clone();
        let picked = self.picked.min(brain.node_count().saturating_sub(1));
        let hover = self.hover;
        let camera = self.camera;
        let canvas_element = div()
            .id("brain")
            .size_full()
            .on_scroll(cx.listener(|this, event: &gpui::ScrollEvent, _window, cx| {
                let Some(bounds) = this.graph_bounds.get() else {
                    return;
                };
                // Lines for a wheel, pixels for a trackpad: both become the same
                // multiplicative step, so the two devices zoom alike.
                let step = match event.delta {
                    gpui::ScrollDelta::Lines(delta) => delta.y,
                    gpui::ScrollDelta::Pixels(delta) => f32::from(delta.y) / 20.0,
                };
                if step == 0.0 {
                    return;
                }
                this.camera = this
                    .camera
                    .zoomed(1.12_f32.powf(step), event.position, bounds);
                cx.notify();
            }))
            .on_pinch(cx.listener(|this, event: &gpui::PinchEvent, _window, cx| {
                if let Some(bounds) = this.graph_bounds.get() {
                    this.camera = this
                        .camera
                        .zoomed(1.0 + event.delta, event.position, bounds);
                    cx.notify();
                }
            }))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, _window, cx| {
                    // A double click is "take me there": the same flight `f` and
                    // the search bar use, aimed at the node under the pointer.
                    if event.click_count >= 2 {
                        let node = match (this.brain.as_ref(), this.graph_bounds.get()) {
                            (Some(brain), Some(bounds)) => {
                                brain.hit(bounds, this.camera, event.position)
                            }
                            _ => None,
                        };
                        if let Some(node) = node {
                            this.focus(node);
                        }
                        this.drag = None;
                        cx.notify();
                        return;
                    }
                    this.drag = Some((event.position, this.camera.pan));
                    this.close_menu();
                    cx.notify();
                }),
            )
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(|this, event: &gpui::MouseDownEvent, _window, cx| {
                    let node = match (this.brain.as_ref(), this.graph_bounds.get()) {
                        (Some(brain), Some(bounds)) => {
                            brain.hit(bounds, this.camera, event.position)
                        }
                        _ => None,
                    };
                    this.open_node_menu(node, event.position);
                    cx.notify();
                }),
            )
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, _window, cx| {
                    // Dragging pans; merely moving hovers. A press that has not
                    // travelled far is still a click waiting to happen.
                    if let Some((start, pan)) = this.drag {
                        let dx = f32::from(event.position.x - start.x);
                        let dy = f32::from(event.position.y - start.y);
                        if dx.abs() + dy.abs() > 2.0 {
                            this.camera.pan = (pan.0 + dx, pan.1 + dy);
                            this.hover = None;
                            cx.notify();
                        }
                        return;
                    }
                    let hovered = match (this.brain.as_ref(), this.graph_bounds.get()) {
                        (Some(brain), Some(bounds)) => {
                            brain.hit(bounds, this.camera, event.position)
                        }
                        _ => None,
                    };
                    if this.hover != hovered {
                        this.hover = hovered;
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseUpEvent, _window, cx| {
                    let dragged = this.drag.take().filter(|(start, _)| {
                        f32::from(event.position.x - start.x).abs()
                            + f32::from(event.position.y - start.y).abs()
                            > 2.0
                    });
                    if dragged.is_some() {
                        cx.notify();
                        return;
                    }
                    let clicked = match (this.brain.as_ref(), this.graph_bounds.get()) {
                        (Some(brain), Some(bounds)) => {
                            brain.hit(bounds, this.camera, event.position)
                        }
                        _ => None,
                    };
                    if let Some(node) = clicked {
                        this.picked = node;
                    }
                    cx.notify();
                }),
            )
            .child(
                canvas(
                    move |bounds_in, _, _| {
                        bounds.set(Some(bounds_in));
                    },
                    move |bounds_in, _, window, cx| {
                        graph.paint(bounds_in, camera, Some(picked), hover, window, cx)
                    },
                )
                .size_full(),
            );

        div()
            .flex_1()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.query_strip())
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .overflow_hidden()
                    .child(div().flex_1().overflow_hidden().child(canvas_element))
                    .child(self.brain_detail(cx)),
            )
            .into_any_element()
    }

    /// The query bar and its matches. It sits above the canvas rather than
    /// floating over it: this view has no absolute positioning to float with,
    /// and a strip that pushes the picture down reads as a mode, which it is.
    fn query_strip(&self) -> AnyElement {
        let Some(query) = &self.query else {
            return div().into_any_element();
        };
        let prompt = match query.mode {
            Mode::Search => "/",
            Mode::Command => ":",
        };
        let mut strip = div()
            .flex()
            .flex_col()
            .bg(rgb(0x301010))
            .border_b(px(1.0))
            .border_color(rgb(0x1f2933))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(12.0))
                    .h(px(28.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(WAIT))
                            .child(SharedString::from(prompt)),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(TEXT))
                            .child(SharedString::from(query.text.clone())),
                    )
                    .child(div().text_size(px(12.0)).text_color(rgb(WAIT)).child("▌"))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(10.0))
                            .text_color(rgb(MUTED))
                            .child(SharedString::from(match query.mode {
                                Mode::Search => "enter opens · ↑↓ picks · esc closes",
                                Mode::Command => {
                                    "runs · brain · fit · zoom N · focus NAME · esc closes"
                                }
                            })),
                    ),
            );
        if query.mode == Mode::Search {
            let brain = self.brain.as_ref();
            strip = strip.children(query.matches.iter().take(6).enumerate().map(
                |(index, node)| {
                    let name = brain
                        .and_then(|brain| brain.name_of(*node))
                        .unwrap_or_default()
                        .to_string();
                    let station = brain
                        .and_then(|brain| brain.station_of(*node))
                        .map(|station| station.name.clone())
                        .unwrap_or_default();
                    let active = index == query.cursor;
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .px(px(14.0))
                        .h(px(20.0))
                        .bg(rgb(if active { 0x606020 } else { 0x203040 }))
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(11.0))
                                .text_color(rgb(if active { TEXT } else { 0xb9c4d1 }))
                                .child(SharedString::from(name)),
                        )
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(rgb(MUTED))
                                .child(SharedString::from(station)),
                        )
                },
            ));
        }
        strip.into_any_element()
    }

    /// The panel a click opens: what the node is, and — for a workflow — the
    /// runs of it the board already has. A run row is a door back into the
    /// runs view, opened on that run.
    fn brain_detail(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(detail) = self
            .brain
            .as_ref()
            .and_then(|brain| brain.detail(self.picked))
        else {
            return div().into_any_element();
        };
        let mut panel = div()
            .w(px(320.0))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .p(px(14.0))
            .bg(rgb(PANEL))
            .border_l(px(1.0))
            .border_color(rgb(0x1f2933))
            .overflow_hidden()
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(rgb(TEXT))
                    .child(SharedString::from(detail.name.clone())),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(WAIT))
                            .child(SharedString::from(detail.kind)),
                    )
                    .child(div().text_size(px(10.0)).text_color(rgb(MUTED)).child(
                        SharedString::from(detail.station.clone().unwrap_or_default()),
                    )),
            );
        for (label, value) in &detail.rows {
            panel = panel.child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(
                        div()
                            .w(px(78.0))
                            .flex_shrink_0()
                            .text_size(px(10.0))
                            .text_color(rgb(MUTED))
                            .child(SharedString::from(*label)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(10.0))
                            .text_color(rgb(0xb9c4d1))
                            .child(SharedString::from(value.clone())),
                    ),
            );
        }

        let runs: Vec<(usize, &Run)> = self
            .runs
            .iter()
            .enumerate()
            .filter(|(_, run)| run.workflow == detail.name)
            .take(8)
            .collect();
        if !runs.is_empty() {
            panel = panel.child(
                div()
                    .mt(px(6.0))
                    .text_size(px(10.0))
                    .text_color(rgb(MUTED))
                    .child(format!("recent runs · {}", runs.len())),
            );
            panel = panel.children(runs.into_iter().map(|(index, run)| {
                div()
                    .id(("run", index))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |this, _: &gpui::MouseDownEvent, _window, cx| {
                            this.selected = index;
                            this.view = View::Runs;
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .w(px(5.0))
                            .h(px(5.0))
                            .rounded(px(3.0))
                            .bg(rgb(run.status_color())),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(10.0))
                            .text_color(rgb(0xb9c4d1))
                            .child(SharedString::from(run.when.clone())),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(MUTED))
                            .child(SharedString::from(run.status.clone())),
                    )
            }));
        }
        if !detail.callers.is_empty() {
            panel = panel
                .child(
                    div()
                        .mt(px(6.0))
                        .text_size(px(10.0))
                        .text_color(rgb(MUTED))
                        .child(format!("called by · {}", detail.callers.len())),
                )
                .children(detail.callers.iter().take(20).map(|caller| {
                    div()
                        .text_size(px(10.0))
                        .text_color(rgb(0xb9c4d1))
                        .child(SharedString::from(caller.clone()))
                }));
        }
        panel.into_any_element()
    }

    fn footer(&self) -> impl IntoElement {
        div()
            .h(px(FOOTER_H))
            .px(px(14.0))
            .flex()
            .items_center()
            .justify_between()
            .bg(rgb(PANEL))
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(MUTED))
                    .child(match self.view {
                        View::Runs => "j/k move   r refresh   1 runs   2 brain   q quit",
                        View::Brain => {
                            "/ search   : command   f focus   drag pan   scroll zoom   0 fit   1 runs   2 brain   q quit"
                        }
                    }),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(MUTED))
                    .child(SharedString::from(match (self.view, &self.brain) {
                        (View::Brain, Some(brain)) => {
                            match (
                                brain.name_of(self.picked),
                                brain.station_of(self.picked),
                            ) {
                                (Some(name), Some(station)) => {
                                    format!("{name} · {}", station.name)
                                }
                                (Some(name), None) => name.to_string(),
                                _ => format!("{} domains", brain.station_count()),
                            }
                        }
                        _ => match self.fetched_at {
                            Some(at) => format!(
                                "{} runs · refreshed {:.0}s ago",
                                self.runs.len(),
                                at.elapsed().as_secs_f32()
                            ),
                            None => format!("{} runs", self.runs.len()),
                        },
                    })),
            )
    }
}

impl Render for Board {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .font_family("Lilex")
            .child(self.header())
            .child(match self.view {
                View::Runs => div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .overflow_hidden()
                    .child(self.list(cx))
                    .child(self.detail())
                    .into_any_element(),
                View::Brain => self.brain_body(cx),
            })
            .child(self.footer())
            .children(self.menu.as_ref().map(|menu| {
                menu.element(cx.listener(|this, id, _window, cx| {
                    this.run_menu(id, cx);
                }))
            }))
    }
}

/// The fixtures are copied from what the live sidecar answers, because the
/// interesting cases here are all shape mismatches — a field that arrives as a
/// string where the board assumed a number, an owners list, a null error.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_live_run_keeps_its_clock_and_its_result() {
        let body = br#"{"runs":[{"error":null,"finished_at":"2026-11-11 05:28:37",
            "id":"r1","result":{"event_type":"push","skipped":"not a pull_request"},
            "started_at":"2026-11-11 05:28:37","status":"ok","trigger":"pubsub",
            "workflow":"notifications/pr","workflow_id":"r1"}]}"#;
        let runs = parse_runs(body).expect("a real response parses");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].workflow, "notifications/pr");
        assert_eq!(
            runs[0].when, "05:28",
            "the board draws a wall clock, not a date"
        );
        assert!(runs[0].detail.contains("push"), "the result is the detail");
    }

    #[test]
    fn newest_first_is_the_only_order_a_board_is_read_in() {
        let body = br#"{"runs":[{"workflow":"old","status":"ok"},
            {"workflow":"new","status":"error"}]}"#;
        let runs = parse_runs(body).expect("a minimal run parses");
        assert_eq!(runs[0].workflow, "new");
        assert_eq!(runs[1].workflow, "old");
    }

    #[test]
    fn a_response_without_the_array_is_an_error_and_not_an_empty_board() {
        let error = parse_runs(br#"{"runs":null}"#).expect_err("null is not a run list");
        assert!(error.contains("runs"), "{error}");
    }

    #[test]
    fn a_live_workflow_keeps_its_owner_and_links() {
        let body = br#"{"workflows":[{"gated":true,"links":["agent","notify"],
            "model_spend":"always","name":"pr","owners":["cto"],
            "summary":"s","unresolved_run":false,"writes":[]}]}"#;
        let workflows = parse_graph(body).expect("a real workflow parses");
        assert_eq!(workflows[0].name, "pr");
        assert_eq!(workflows[0].owner, "cto");
        assert_eq!(workflows[0].links, vec!["agent", "notify"]);
        assert!(
            workflows[0].gated,
            "a gate is a red dot and must survive parsing"
        );
    }

    #[test]
    fn a_workflow_nobody_owns_still_gets_a_column() {
        let body = br#"{"workflows":[{"name":"orphan"}]}"#;
        let workflows = parse_graph(body).expect("a sparse workflow parses");
        assert_eq!(workflows[0].owner, "unowned");
        assert!(workflows[0].links.is_empty());
        assert!(!workflows[0].gated);
    }

    #[test]
    fn a_pointer_over_a_dot_picks_that_dot_and_nowhere_else_does() {
        let body = br#"{"workflows":[
            {"name":"dev/a","owners":["cto"],"steps":[{"kind":"call","name":"agent"}]},
            {"name":"dev/b","owners":["cto"]},
            {"name":"ops/c","owners":["coo"]}
        ]}"#;
        let workflows = parse_graph(body).expect("a small graph parses");
        let brain = crate::brain::Brain::build(&workflows);
        let bounds = gpui::Bounds {
            origin: gpui::point(gpui::px(0.0), gpui::px(0.0)),
            size: gpui::size(
                gpui::px(crate::brain::SPACE.0),
                gpui::px(crate::brain::SPACE.1),
            ),
        };
        // Far from every node: nothing is picked, so a click on blank canvas
        // does not silently open whichever node happens to be first.
        let camera = crate::brain::Camera::default();
        assert_eq!(
            brain.hit(bounds, camera, gpui::point(gpui::px(2.0), gpui::px(2.0))),
            None
        );

        let mut seen = 0;
        while let Some(node) = brain.workflow_at(seen) {
            let centre = brain.centre_of(node).expect("a placed node has a centre");
            assert_eq!(
                brain.hit(bounds, camera, centre),
                Some(node),
                "a pointer on a node's centre must pick that node"
            );
            seen += 1;
        }
        assert_eq!(
            seen,
            workflows.len(),
            "every workflow is reachable by pointer"
        );
    }

    #[test]
    fn search_finds_by_name_then_station_and_puts_exact_matches_first() {
        let body = br#"{"workflows":[
            {"name":"notifications/pr","owners":["cto"],"steps":[{"kind":"call","name":"agent"}]},
            {"name":"notifications/marbles","owners":["cto"]},
            {"name":"dev/pr-close","owners":["coo"]},
            {"name":"report/weekly","owners":["cmo"]}
        ]}"#;
        let workflows = parse_graph(body).expect("a small graph parses");
        let brain = crate::brain::Brain::build(&workflows);

        let hits = brain.find("notifications");
        assert_eq!(hits.len(), 2, "a domain name finds its workflows");

        let exact = brain.find("notifications/pr");
        assert_eq!(
            brain.name_of(exact[0]),
            Some("notifications/pr"),
            "an exact name comes first"
        );

        let by_owner = brain.find("cmo");
        assert!(
            by_owner
                .iter()
                .filter_map(|node| brain.name_of(*node))
                .any(|name| name == "report/weekly"),
            "an owner finds what it owns"
        );
        assert!(brain.find("nothing-here").is_empty());
    }

    #[test]
    fn the_saved_state_survives_a_round_trip() {
        let mut board = Board::new();
        board.view = View::Brain;
        board.zoom_to(2.5);
        board.camera.pan = (37.0, -12.0);
        board.picked = 7;
        let saved = board.state();

        let mut restored = Board::new();
        restored.restore(saved.clone());
        assert_eq!(restored.state(), saved);
        assert!(restored.state().brain, "the view that was open reopens");

        // A file written by an older build without a field still loads.
        let older: State = serde_json::from_str(r#"{"brain":true}"#).expect("partial state loads");
        assert_eq!(older.picked, 0);
    }

    #[test]
    fn a_focus_flies_the_camera_to_the_node() {
        let body = br#"{"workflows":[
            {"name":"dev/a","owners":["cto"],"steps":[{"kind":"call","name":"agent"}]},
            {"name":"ops/c","owners":["coo"]}
        ]}"#;
        let workflows = parse_graph(body).expect("a small graph parses");
        let mut board = Board::new();
        board.apply_graph(Ok(workflows));
        let bounds = gpui::Bounds {
            origin: gpui::point(gpui::px(0.0), gpui::px(0.0)),
            size: gpui::size(
                gpui::px(crate::brain::SPACE.0),
                gpui::px(crate::brain::SPACE.1),
            ),
        };
        board.graph_bounds.set(Some(bounds));

        board.focus(1);
        assert!(board.tick(), "a focus asks for frames until it arrives");
        let mut guard = 0;
        while board.tick() && guard < 200 {
            guard += 1;
        }
        let node = board.picked_node();
        let (x, y) = board
            .brain
            .as_ref()
            .and_then(|brain| brain.position_of(node))
            .expect("the focused node has a place");
        let (origin, scale) = board.camera.frame(bounds);
        let landed_x = f32::from(origin.x) + x * scale;
        let landed_y = f32::from(origin.y) + y * scale;
        assert!(
            (landed_x - crate::brain::SPACE.0 / 2.0).abs() < 4.0
                && (landed_y - crate::brain::SPACE.1 / 2.0).abs() < 4.0,
            "the focused node ends up in the middle: {landed_x}, {landed_y}"
        );
    }

    #[test]
    fn zooming_keeps_the_node_under_the_pointer_and_panning_moves_it() {
        let body = br#"{"workflows":[
            {"name":"dev/a","owners":["cto"],"steps":[{"kind":"call","name":"agent"}]},
            {"name":"ops/c","owners":["coo"]}
        ]}"#;
        let workflows = parse_graph(body).expect("a small graph parses");
        let brain = crate::brain::Brain::build(&workflows);
        let bounds = gpui::Bounds {
            origin: gpui::point(gpui::px(0.0), gpui::px(0.0)),
            size: gpui::size(
                gpui::px(crate::brain::SPACE.0),
                gpui::px(crate::brain::SPACE.1),
            ),
        };
        let node = brain.workflow_at(0).expect("the first workflow is a node");
        let centre = brain.centre_of(node).expect("a placed node has a centre");

        // Zooming about a point is only worth having if that point stays put.
        let zoomed = crate::brain::Camera::default().zoomed(3.0, centre, bounds);
        assert_eq!(
            brain.hit(bounds, zoomed, centre),
            Some(node),
            "the node under the pointer must not slide out from under it"
        );

        // Panning by a drag moves the picture by exactly that drag.
        let panned = crate::brain::Camera {
            zoom: 1.0,
            pan: (40.0, -25.0),
        };
        let moved = gpui::point(centre.x + gpui::px(40.0), centre.y - gpui::px(25.0));
        assert_eq!(
            brain.hit(bounds, panned, moved),
            Some(node),
            "a pan shifts the picture by exactly the drag"
        );
    }
}
