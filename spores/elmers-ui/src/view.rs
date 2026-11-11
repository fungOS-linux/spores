//! The board's view: pure GPUI, no transport.
//!
//! Shared by the wasm guest (which fetches through `soil:http`) and the native
//! preview (which fetches with ureq), so the thing you look at in a window is
//! the same code the Spore runs.

use gpui::{
    AnyElement, Context, IntoElement, Render, SharedString, Window, div, prelude::*, px, rgb,
};

const BG: u32 = 0x0b0f14;
const PANEL: u32 = 0x0f141b;
const ROW_HOVER: u32 = 0x1b2430;
const TEXT: u32 = 0xdbe4ee;
const MUTED: u32 = 0x6d7b8c;
const OK: u32 = 0x4f9e5a;
const BAD: u32 = 0xd14b4b;
const WAIT: u32 = 0xc9a227;
const SPEND: u32 = 0xf0a26b;
const PICKED: u32 = 0x2c3a4c;
const LINKED: u32 = 0x1e2a37;

pub(crate) const HEADER_H: f32 = 44.0;
const FOOTER_H: f32 = 26.0;
pub(crate) const LIST_W: f32 = 420.0;
pub(crate) const ROW_H: f32 = 34.0;

#[derive(Clone, Debug)]
pub struct Run {
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
        let detail = {
            let error = text("error");
            if !error.is_empty() {
                error
            } else {
                let result = entry.get("result").cloned().unwrap_or_default();
                if result.is_null() {
                    String::new()
                } else {
                    result.to_string()
                }
            }
        };
        let when = entry.get("started_at").map(wall_clock).unwrap_or_default();
        runs.push(Run {
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
/// calls. Everything the brain view draws is in here.
pub struct Workflow {
    name: String,
    owner: String,
    links: Vec<String>,
    model_spend: String,
    gated: bool,
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
        workflows.push(Workflow {
            name: text("name"),
            owner,
            links,
            model_spend: text("model_spend"),
            gated: entry
                .get("gated")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
        });
    }
    Ok(workflows)
}

pub struct Board {
    pub view: View,
    workflows: Vec<Workflow>,
    picked: usize,
    graph_error: Option<SharedString>,
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
            graph_error: None,
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
                let keep = self.picked.min(workflows.len().saturating_sub(1));
                self.workflows = workflows;
                self.picked = keep;
                self.graph_error = None;
            }
            Err(error) => self.graph_error = Some(SharedString::from(error)),
        }
        self.graph_at = Some(std::time::Instant::now());
    }

    pub fn move_picked(&mut self, delta: i32) {
        if self.workflows.is_empty() {
            return;
        }
        let max = self.workflows.len() as i32 - 1;
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
            .child(
                div()
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
                    ),
            )
    }

    fn list(&self) -> impl IntoElement {
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
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(MUTED))
                            .child(SharedString::from(run.detail.clone())),
                    );
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

    /// The brain: one column per owning function, every workflow a card.
    ///
    /// The fork has no line or path primitive in the headless renderer, so the
    /// graph is drawn by grouping and emphasis rather than by edges: selecting
    /// a workflow lights up the ones it calls, wherever they sit.
    fn brain_body(&self) -> AnyElement {
        let mut owners: Vec<&str> = Vec::new();
        for workflow in &self.workflows {
            if !owners.contains(&workflow.owner.as_str()) {
                owners.push(&workflow.owner);
            }
        }
        let picked = self.workflows.get(self.picked);
        let linked: Vec<&str> = picked
            .map(|workflow| workflow.links.iter().map(String::as_str).collect())
            .unwrap_or_default();

        let mut body = div()
            .flex_1()
            .p(px(12.0))
            .flex()
            .flex_row()
            .gap(px(10.0))
            .overflow_hidden();
        if self.workflows.is_empty() {
            body = body.child(div().text_size(px(12.0)).text_color(rgb(MUTED)).child(
                match &self.graph_error {
                    Some(error) => error.clone(),
                    None => SharedString::from("loading the graph…"),
                },
            ));
            return body.into_any_element();
        }
        body.children(owners.iter().map(|owner| {
            let cards: Vec<&Workflow> = self
                .workflows
                .iter()
                .filter(|workflow| workflow.owner == *owner)
                .collect();
            div()
                .w(px(196.0))
                .flex()
                .flex_col()
                .gap(px(3.0))
                .overflow_hidden()
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgb(WAIT))
                        .child(SharedString::from(format!("{owner} · {}", cards.len()))),
                )
                .children(cards.into_iter().map(|workflow| {
                    let is_picked = picked.map(|p| p.name == workflow.name).unwrap_or(false);
                    let is_linked = linked.contains(&workflow.name.as_str());
                    let fill = if is_picked {
                        PICKED
                    } else if is_linked {
                        LINKED
                    } else {
                        PANEL
                    };
                    let dot = if workflow.gated {
                        BAD
                    } else if workflow.model_spend != "none" && !workflow.model_spend.is_empty() {
                        SPEND
                    } else {
                        MUTED
                    };
                    div()
                        .px(px(6.0))
                        .py(px(3.0))
                        .bg(rgb(fill))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(6.0))
                        .child(div().w(px(5.0)).h(px(5.0)).rounded(px(3.0)).bg(rgb(dot)))
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(10.0))
                                .text_color(rgb(if is_picked { TEXT } else { 0xb9c4d1 }))
                                .child(SharedString::from(workflow.name.clone())),
                        )
                }))
        }))
        .into_any_element()
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
                        View::Brain => "j/k pick   r reload graph   1 runs   2 brain   q quit",
                    }),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(MUTED))
                    .child(SharedString::from(match self.fetched_at {
                        Some(at) => format!(
                            "{} runs · refreshed {:.0}s ago",
                            self.runs.len(),
                            at.elapsed().as_secs_f32()
                        ),
                        None => format!("{} runs", self.runs.len()),
                    })),
            )
    }
}

impl Render for Board {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
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
                    .child(self.list())
                    .child(self.detail())
                    .into_any_element(),
                View::Brain => self.brain_body(),
            })
            .child(self.footer())
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
}
