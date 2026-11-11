//! elmers-ui — a read-only board of Elmers workflow runs.
//!
//! The Spore owns the view and nothing else. It asks the sidecar one question —
//! `GET /runs` — and draws the answer. There is deliberately no way to start,
//! stop, or edit a workflow from here: the run API is a list and a lookup, and
//! this package imports exactly that. A UI that can run things is a different
//! grant and a different review.
//!
//! The host owns the WebGPU device and the compositor surface; nothing in this
//! file sees a window, a swapchain, or a display handle. `soil:http` is the
//! bounded fetch the host mediates, so the URL is the only thing this Spore names.

wit_bindgen::generate!({
    path: "wit",
    world: "runner",
    with: {
        "wasi:webgpu/webgpu@0.3.0-rc.2": wgpu_wasi_webgpu::wasi::webgpu::webgpu,
    },
    generate_all,
});

use gpui::{
    AnyElement, AnyWindowHandle, AppContext as _, Context, HeadlessAppContext, IntoElement,
    PlatformTextSystem, Render, SharedString, Window, div, prelude::*, px, rgb, size,
};
use gpui_wgpu::{CosmicTextSystem, WgpuContext, WgpuHeadlessRenderer};
use soil::present::{input, surface};
use std::{borrow::Cow, sync::Arc};

const LILEX: &[u8] = include_bytes!("../assets/Lilex-Regular.ttf");
const LILEX_BOLD: &[u8] = include_bytes!("../assets/Lilex-Bold.ttf");

/// The sidecar's read-only run list. One origin, named once.
const RUNS_URL: &str = "http://127.0.0.1:8792/runs";
/// The workflow graph the dashboard's "brain" view draws.
const GRAPH_URL: &str = "http://127.0.0.1:8792/graph";
const MAX_BODY: u32 = 512 * 1024;
const TIMEOUT_MS: u32 = 5_000;

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

const HEADER_H: f32 = 44.0;
const FOOTER_H: f32 = 26.0;
const LIST_W: f32 = 420.0;
const ROW_H: f32 = 34.0;

#[derive(Clone, Debug)]
struct Run {
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
fn parse_runs(body: &[u8]) -> Result<Vec<Run>, String> {
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
        let when = {
            let started = entry.get("started_at").and_then(serde_json::Value::as_i64);
            match started {
                Some(secs) => format_clock(secs),
                None => String::new(),
            }
        };
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

/// `started_at` is epoch seconds; the board has no calendar and no timezone, so
/// it shows the clock reading a person would compare against "now".
fn format_clock(secs: i64) -> String {
    let day = secs.rem_euclid(86_400);
    let hh = day / 3600;
    let mm = (day % 3600) / 60;
    format!("{hh:02}:{mm:02}")
}

/// Which surface the window is showing. Both read from the same sidecar.
#[derive(Clone, Copy, PartialEq)]
enum View {
    Runs,
    Brain,
}

/// One workflow as `GET /graph` describes it: identity, owner, and what it
/// calls. Everything the brain view draws is in here.
struct Workflow {
    name: String,
    owner: String,
    links: Vec<String>,
    model_spend: String,
    gated: bool,
}

fn parse_graph(body: &[u8]) -> Result<Vec<Workflow>, String> {
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

struct Board {
    view: View,
    workflows: Vec<Workflow>,
    picked: usize,
    graph_error: Option<SharedString>,
    graph_at: Option<std::time::Instant>,
    runs: Vec<Run>,
    selected: usize,
    error: Option<SharedString>,
    fetched_at: Option<std::time::Instant>,
    frames: u64,
}

impl Board {
    fn new() -> Self {
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
            frames: 0,
        }
    }

    fn refresh(&mut self) {
        match fetch_runs(RUNS_URL) {
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
    fn refresh_graph(&mut self) {
        match fetch_body(GRAPH_URL).and_then(|body| parse_graph(&body)) {
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

    fn move_picked(&mut self, delta: i32) {
        if self.workflows.is_empty() {
            return;
        }
        let max = self.workflows.len() as i32 - 1;
        self.picked = (self.picked as i32 + delta).clamp(0, max) as usize;
    }

    fn move_selection(&mut self, delta: i32) {
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

/// The only host call in the package: one bounded, allowlisted GET.
fn fetch_body(url: &str) -> Result<Vec<u8>, String> {
    use soil::http::http as abi;
    let response = abi::fetch(&abi::Request {
        url: url.to_string(),
        method: abi::Method::Get,
        headers: vec![abi::Header {
            name: "accept".to_string(),
            value: "application/json".to_string(),
        }],
        body: None,
        credential: None,
        max_bytes: MAX_BODY,
        timeout_ms: TIMEOUT_MS,
    })
    .map_err(|error| format!("http: {error:?}"))?;
    if response.status != 200 {
        return Err(format!("{url}: HTTP {}", response.status));
    }
    Ok(response.body)
}

fn fetch_runs(url: &str) -> Result<Vec<Run>, String> {
    parse_runs(&fetch_body(url)?)
}

fn present_frame(
    context: &WgpuContext,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> anyhow::Result<()> {
    let texture = context.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("elmers-ui frame"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    context.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    let texture = wgpu_wasi_webgpu::host_texture(texture);
    surface::present(&texture).map_err(|error| anyhow::anyhow!("present: {error}"))?;
    Ok(())
}

fn render_frame() -> anyhow::Result<()> {
    let (width, height) = surface::size();
    let instance = wgpu_wasi_webgpu::instance();
    let adapter = wit_bindgen::block_on(instance.request_adapter(&Default::default()))?;
    let (device, queue) = wit_bindgen::block_on(adapter.request_device(&Default::default()))?;
    let (context, format) = WgpuContext::from_offscreen(instance, adapter, device, queue)?;

    let text = Arc::new(CosmicTextSystem::new_without_system_fonts("Lilex"));
    text.add_fonts(vec![Cow::Borrowed(LILEX), Cow::Borrowed(LILEX_BOLD)])?;

    let factory_context = context.clone();
    let mut cx = HeadlessAppContext::with_platform(text, Arc::new(()), move || {
        Ok(Some(Box::new(WgpuHeadlessRenderer::with_context(
            factory_context.clone(),
            format,
        )?)))
    });

    let scale = surface::scale() as f32;
    let window =
        cx.open_window_scaled(size(px(width as f32), px(height as f32)), scale, |_, cx| {
            cx.new(|_| {
                let mut board = Board::new();
                board.refresh();
                board
            })
        })?;
    let any: AnyWindowHandle = window.into();

    let mut window_size = (width, height);
    let mut dirty = true;
    loop {
        let size = surface::size();
        if size != window_size {
            window_size = size;
            window.update(&mut cx, |_, window, _| {
                window.resize(gpui::size(px(size.0 as f32), px(size.1 as f32)));
            })?;
            dirty = true;
        }

        // A board is a live thing: refresh on a slow tick so a run that finishes
        // while someone is watching does not need a key press to appear.
        let stale = window.update(&mut cx, |board, _, _| {
            board.frames += 1;
            board
                .fetched_at
                .map(|at| at.elapsed().as_secs() >= 5)
                .unwrap_or(true)
        })?;
        if stale {
            let runs = window.update(&mut cx, |board, _, cx| {
                board.refresh();
                cx.notify();
                board.runs.len()
            })?;
            let _ = runs;
            dirty = true;
        }

        if dirty {
            cx.update_window(any, |_, window, app| {
                let _ = window.draw(app);
            })?;
            let image = cx.capture_screenshot(any)?;
            present_frame(&context, image.width(), image.height(), image.as_raw())?;
            dirty = false;
        }

        if !input::interactive() {
            break;
        }
        while let Some(event) = input::poll(0) {
            match event {
                input::Event::KeyInput(key) => {
                    let name = key.key.to_ascii_lowercase();
                    match name.as_str() {
                        "q" => return Ok(()),
                        "j" | "arrow-down" => {
                            window.update(&mut cx, |board, _, cx| {
                                match board.view {
                                    View::Runs => board.move_selection(1),
                                    View::Brain => board.move_picked(1),
                                }
                                cx.notify();
                            })?;
                            dirty = true;
                        }
                        "k" | "arrow-up" => {
                            window.update(&mut cx, |board, _, cx| {
                                match board.view {
                                    View::Runs => board.move_selection(-1),
                                    View::Brain => board.move_picked(-1),
                                }
                                cx.notify();
                            })?;
                            dirty = true;
                        }
                        "1" => {
                            window.update(&mut cx, |board, _, cx| {
                                board.view = View::Runs;
                                cx.notify();
                            })?;
                            dirty = true;
                        }
                        "2" | "tab" => {
                            window.update(&mut cx, |board, _, cx| {
                                board.view = View::Brain;
                                // The graph changes when a workflow is edited,
                                // not on every tick; reload it when the view is
                                // opened and it is already stale.
                                let stale = board
                                    .graph_at
                                    .map(|at| at.elapsed().as_secs() >= 30)
                                    .unwrap_or(true);
                                if stale {
                                    board.refresh_graph();
                                }
                                cx.notify();
                            })?;
                            dirty = true;
                        }
                        "r" => {
                            window.update(&mut cx, |board, _, cx| {
                                match board.view {
                                    View::Runs => board.refresh(),
                                    View::Brain => board.refresh_graph(),
                                }
                                cx.notify();
                            })?;
                            dirty = true;
                        }
                        _ => {}
                    }
                }
                // A click is a selection, not an action: the detail pane follows
                // the pointer, and nothing here can start work.
                input::Event::Button(button)
                    if button.pressed && button.button == input::PointerButton::Left =>
                {
                    window.update(&mut cx, |board, _, cx| {
                        if button.x < LIST_W {
                            let index = ((button.y - HEADER_H) / ROW_H).max(0.0) as usize;
                            if index < board.runs.len() {
                                board.selected = index;
                                cx.notify();
                            }
                        }
                    })?;
                    dirty = true;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

struct Component;

impl Guest for Component {
    async fn run() -> u32 {
        match render_frame() {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("elmers-ui: {error:#}");
                1
            }
        }
    }
}

export!(Component);
