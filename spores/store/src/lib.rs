//! Store view Spore: GPUI lays out the shopfront, `wasi:webgpu` renders it, and
//! `soil:canvas-gpu` presents the texture to Canvas.
//!
//! The guest never gets a window or a swapchain, and never opens a socket: the
//! catalog is read with `soil:http`, which the host checks against its origin
//! allowlist. What the Spore owns is the shopfront — the account chip, the
//! cards, the states a client can be in. Canvas owns the window and the input.
//!
//! The same package also ships a `web` surface (`web/index.html`) for a browser
//! that has no Canvas; both read `/api/catalog` from the same store service, so
//! there is one catalog and two ways to draw it.

wit_bindgen::generate!({
    path: "wit",
    world: "runner",
    with: {
        "wasi:webgpu/webgpu@0.3.0-rc.2": wgpu_wasi_webgpu::wasi::webgpu::webgpu,
    },
    generate_all,
});

use std::borrow::Cow;
use std::sync::Arc;

use gpui::prelude::*;
use gpui::{
    div, px, rgb, AnyWindowHandle, Context, FontWeight, HeadlessAppContext, IntoElement,
    ParentElement, PlatformTextSystem as _, Render, SharedString, Styled, Window,
};
use gpui_wgpu::{CosmicTextSystem, WgpuContext, WgpuHeadlessRenderer};
use soil::http::http;
use soil::present::surface;
use soil::spore::catalog;

/// IBM Plex Sans, SIL Open Font License 1.1. A wasm component has no system
/// fonts, so the Spore ships the one face its view names.
const FONT: &[u8] = include_bytes!("../assets/IBMPlexSans-Regular.ttf");
const FONT_FAMILY: &str = "IBM Plex Sans";

/// The store this Spore reads. The host decides whether the origin is allowed;
/// a fleet store points this at its own index by repackaging with a different
/// value, which is a package revision, not a setting the guest can flip.
const ORIGIN: &str = "http://127.0.0.1:8800";

const BG: u32 = 0x0b0d12;
const PANEL: u32 = 0x141821;
const PANEL_RAISED: u32 = 0x1a2233;
const EDGE: u32 = 0x222735;
const TEXT: u32 = 0xe9ecf3;
const MUTED: u32 = 0x98a1b3;
const ACCENT: u32 = 0x8b9dff;
const GOOD: u32 = 0x7ad0a3;
const WARN: u32 = 0xf2c14e;

struct Component;
export!(Component);

impl Guest for Component {
    async fn run() -> u32 {
        match render() {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("store-view: {error}");
                1
            }
        }
    }
}

/// The index the store service served: its name when one answered, whether
/// its signature checked out, and how many packages it carries.
struct Source {
    name: Option<String>,
    verified: bool,
    total: usize,
}

/// A package's own mark: the badge and colour its manifest declares for a
/// launcher row, and later the artwork a listing points at. The store draws
/// what the package says about itself rather than inventing a colour for it.
struct Mark {
    badge: String,
    color: Option<u32>,
}

impl Mark {
    fn of(title: &str, badge: &str, color: Option<u32>) -> Self {
        let badge = if badge.is_empty() || badge == "?" {
            title
                .chars()
                .next()
                .map(|value| value.to_uppercase().to_string())
                .unwrap_or_else(|| "?".to_string())
        } else {
            badge.to_string()
        };
        Self { badge, color }
    }
}

struct Row {
    /// What a person reads. A listing's name when the index has one, the
    /// installed package's own title when it does not, and the id's last
    /// segment as the last resort.
    title: String,
    id: String,
    summary: String,
    description: String,
    version: String,
    installed_version: Option<String>,
    state: String,
    mark: Mark,
    /// What applying the offering would cost, when the store has planned it:
    /// `live` applies now, `restart` at the next launch, `reboot` after one.
    change: Option<Change>,
}

struct Change {
    class: String,
    reason: String,
}

/// One GET against the store service. The host owns the origin policy: a
/// refusal or a transport failure is `None`, not an empty answer.
fn get(path: &str) -> Option<serde_json::Value> {
    let request = http::Request {
        url: format!("{ORIGIN}{path}"),
        method: http::Method::Get,
        headers: vec![http::Header {
            name: "accept".to_string(),
            value: "application/json".to_string(),
        }],
        body: None,
        credential: None,
        max_bytes: 1 << 20,
        timeout_ms: 4_000,
    };
    let response = http::fetch(&request).ok()?;
    if response.status != 200 {
        return None;
    }
    serde_json::from_slice(&response.body).ok()
}

/// What the store host knows about a package it has on disk. The catalog rows
/// describe what is for sale; this describes what the package says about
/// itself, which is the same answer for a row and for the machine list.
#[derive(Default)]
struct Facts {
    title: String,
    summary: String,
    description: String,
    badge: String,
    color: Option<u32>,
}

fn facts_of() -> std::collections::BTreeMap<String, Facts> {
    let mut facts = std::collections::BTreeMap::new();
    let Some(apps) = get("/api/apps") else {
        return facts;
    };
    for app in apps["apps"].as_array().cloned().unwrap_or_default() {
        let id = app["id"].as_str().unwrap_or_default().to_string();
        if id.is_empty() {
            continue;
        }
        facts.insert(
            id,
            Facts {
                title: title_of(&app["name"].as_str().unwrap_or_default().to_string()),
                summary: app["summary"].as_str().unwrap_or_default().to_string(),
                description: app["description"].as_str().unwrap_or_default().to_string(),
                badge: app["badge"].as_str().unwrap_or_default().to_string(),
                color: parse_color(app["color"].as_str().unwrap_or_default()),
            },
        );
    }
    facts
}

/// The store's own plan, which is the closest thing to release notes this
/// system has: it says what changed and what applying it costs.
fn plan_of() -> std::collections::BTreeMap<String, Change> {
    let mut plan = std::collections::BTreeMap::new();
    let Some(value) = get("/api/plan") else {
        return plan;
    };
    for update in value["updates"].as_array().cloned().unwrap_or_default() {
        let id = update["id"].as_str().unwrap_or_default().to_string();
        if id.is_empty() {
            continue;
        }
        plan.insert(
            id,
            Change {
                class: update["class"].as_str().unwrap_or_default().to_string(),
                reason: update["reason"].as_str().unwrap_or_default().to_string(),
            },
        );
    }
    plan
}

fn catalog_rows() -> Result<(Vec<Row>, Source), String> {
    let installed = catalog::entries().map_err(|error| format!("catalog: {error:?}"))?;
    let facts = facts_of();
    let plan = plan_of();

    let mut source = Source {
        name: None,
        verified: false,
        total: 0,
    };
    let mut rows: Vec<Row> = Vec::new();
    let mut listed = std::collections::BTreeSet::new();
    if let Some(remote) = get("/api/catalog") {
        source.name = remote["source"]["name"].as_str().map(str::to_string);
        source.verified = remote["source"]["verified"].as_bool().unwrap_or(false);
        let entries = remote["entries"].as_array().cloned().unwrap_or_default();
        source.total = entries.len();
        for entry in entries {
            let id = entry["id"].as_str().unwrap_or_default().to_string();
            if id.is_empty() {
                continue;
            }
            let document = &entry["document"];
            let local = facts.get(&id);
            // Until the index carries a listing, the package installed on this
            // machine is the only honest source of a name, a sentence and a
            // mark: it described itself when it was built. Nothing here is
            // invented, and an absent description stays absent.
            let title = document["name"]
                .as_str()
                .map(str::to_string)
                .or_else(|| local.map(|facts| facts.title.clone()))
                .unwrap_or_else(|| title_of(&id));
            rows.push(Row {
                mark: Mark::of(
                    &title,
                    local.map(|facts| facts.badge.as_str()).unwrap_or_default(),
                    local.and_then(|facts| facts.color),
                ),
                title,
                summary: document["summary"]
                    .as_str()
                    .map(str::to_string)
                    .or_else(|| local.map(|facts| facts.summary.clone()))
                    .unwrap_or_default(),
                description: local
                    .map(|facts| facts.description.clone())
                    .unwrap_or_default(),
                version: entry["version"].as_str().unwrap_or_default().to_string(),
                installed_version: entry["installed_version"].as_str().map(str::to_string),
                state: entry["state"].as_str().unwrap_or("available").to_string(),
                change: plan.get(&id).map(|change| Change {
                    class: change.class.clone(),
                    reason: change.reason.clone(),
                }),
                id: id.clone(),
            });
            listed.insert(id);
        }
    }
    for entry in installed {
        if listed.contains(&entry.id) {
            continue;
        }
        let local = facts.get(&entry.id);
        let title = local
            .map(|facts| facts.title.clone())
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| title_of(&entry.id));
        rows.push(Row {
            mark: Mark::of(
                &title,
                local.map(|facts| facts.badge.as_str()).unwrap_or_default(),
                local.and_then(|facts| facts.color),
            ),
            summary: entry.summary,
            description: local
                .map(|facts| facts.description.clone())
                .unwrap_or_default(),
            version: entry.version,
            installed_version: None,
            state: "installed".to_string(),
            change: None,
            title,
            id: entry.id,
        });
    }
    rows.sort_by_key(|row| row.title.to_lowercase());
    Ok((rows, source))
}

/// `#rrggbb` as the renderer wants it. A colour the manifest got wrong is the
/// same as no colour: the tile falls back instead of drawing something odd.
fn parse_color(value: &str) -> Option<u32> {
    let hex = value.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    u32::from_str_radix(hex, 16).ok()
}

/// Who is asking. A loopback request is the machine itself; a request through
/// the Mycelium gateway names the principal it authenticated.
struct Account {
    name: String,
    note: String,
}

fn account() -> Account {
    if let Some(value) = get("/api/whoami") {
        let principal = value["principal"].as_str().unwrap_or("local");
        return Account {
            name: principal
                .strip_prefix("mycelium:")
                .unwrap_or(principal)
                .to_string(),
            note: if value["through_gateway"].as_bool().unwrap_or(false) {
                "signed in"
            } else {
                "this machine"
            }
            .to_string(),
        };
    }
    Account {
        name: "local".to_string(),
        note: "this machine".to_string(),
    }
}

fn render() -> Result<(), String> {
    let (width, height) = surface::size();
    if width == 0 || height == 0 {
        return Err("canvas surface has zero extent".to_string());
    }

    let instance = wgpu_wasi_webgpu::instance();
    let adapter = wit_bindgen::block_on(instance.request_adapter(&Default::default()))
        .map_err(|error| format!("request adapter: {error}"))?;
    let (device, queue) = wit_bindgen::block_on(adapter.request_device(&Default::default()))
        .map_err(|error| format!("request device: {error}"))?;
    let (context, format) = WgpuContext::from_offscreen(instance, adapter, device, queue)
        .map_err(|error| format!("offscreen context: {error}"))?;

    let frame_device = context.device.clone();
    let frame_queue = context.queue.clone();
    let renderer_context = context.clone();
    let text_system = Arc::new(CosmicTextSystem::new_without_system_fonts(FONT_FAMILY));
    text_system
        .add_fonts(vec![Cow::Borrowed(FONT)])
        .map_err(|error| format!("load view font: {error}"))?;

    let mut app = HeadlessAppContext::with_platform(
        text_system,
        Arc::new(()),
        move || -> anyhow::Result<Option<Box<dyn gpui::PlatformHeadlessRenderer>>> {
            Ok(Some(Box::new(WgpuHeadlessRenderer::with_context(
                renderer_context.clone(),
                format,
            )?)))
        },
    );

    let (rows, source) = catalog_rows()?;
    let account = account();
    let viewport = (width as f32, height as f32);
    let window = app
        .open_window_scaled(
            gpui::size(px(viewport.0), px(viewport.1)),
            1.0,
            move |_, cx| {
                cx.new(|_| StoreView {
                    rows,
                    source,
                    account,
                    width: viewport.0,
                    height: viewport.1,
                })
            },
        )
        .map_err(|error| format!("open headless window: {error}"))?;
    app.run_until_parked();
    let handle: AnyWindowHandle = window.into();
    let image = app
        .capture_screenshot(handle)
        .map_err(|error| format!("capture GPUI frame: {error}"))?;

    let width = image.width();
    let height = image.height();
    let texture = frame_device.create_texture(&wgpu::TextureDescriptor {
        label: Some("store GPUI frame"),
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
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    frame_queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        image.as_raw(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(
                width
                    .checked_mul(4)
                    .ok_or_else(|| "GPUI frame row overflow".to_string())?,
            ),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    let texture = wgpu_wasi_webgpu::host_texture(texture);
    surface::present(&texture).map_err(|error| format!("canvas present: {error}"))
}

/// A window with no scrollbar has to be arithmetic: every section states the
/// height it needs, and the view decides what fits before it draws anything.
///
/// The page is one column, never the whole window: a store is read, and a row
/// that runs from edge to edge is a row whose eye has to travel.
const MAX_COLUMN: f32 = 920.0;
const MIN_MARGIN: f32 = 32.0;
const HEADER_H: f32 = 56.0;
const PAGE_PAD: f32 = 24.0;
const STRIP_H: f32 = 48.0;
const SECTION_GAP: f32 = 20.0;
const HEAD_H: f32 = 22.0;
const HEAD_GAP: f32 = 8.0;
const ROW_H: f32 = 74.0;
const ROW_GAP: f32 = 8.0;
const TILE: f32 = 40.0;
const CHIP_W: f32 = 220.0;
const CHIP_H: f32 = 38.0;
const CHIP_GAP: f32 = 8.0;

fn column(width: f32) -> f32 {
    (width - MIN_MARGIN * 2.0).min(MAX_COLUMN)
}

fn page_margin(width: f32) -> f32 {
    ((width - column(width)) / 2.0).max(MIN_MARGIN.min(width / 2.0))
}

struct StoreView {
    rows: Vec<Row>,
    source: Source,
    account: Account,
    width: f32,
    height: f32,
}

impl Render for StoreView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let by_state = |state: &'static str| {
            self.rows
                .iter()
                .filter(move |row| row.state == state)
                .collect::<Vec<&Row>>()
        };
        let updates = by_state("update");
        let available = by_state("available");
        let installed = by_state("installed");

        let budget = (self.height - HEADER_H - PAGE_PAD * 2.0 - STRIP_H - SECTION_GAP).max(0.0);
        let (update_rows, available_rows, machine_space) =
            fit_lists(updates.len(), available.len(), budget);

        let mut body = div()
            .flex()
            .flex_col()
            .gap_5()
            .py_6()
            .px(px(page_margin(self.width)))
            .flex_1()
            .overflow_hidden()
            .child(status_strip(
                updates.len(),
                available.len(),
                installed.len(),
                &self.source,
            ));
        if update_rows > 0 {
            body = body.child(action_section(
                "Updates",
                &updates[..update_rows],
                updates.len(),
                "Update",
            ));
        }
        if available_rows > 0 {
            body = body.child(action_section(
                "New in the store",
                &available[..available_rows],
                available.len(),
                "Get",
            ));
        }
        if !installed.is_empty() {
            body = body.child(machine_section(
                &installed,
                column(self.width),
                machine_space,
                self.source.name.is_none(),
            ));
        }
        if self.rows.is_empty() {
            body = body.child(nothing_to_show(&self.source));
        }

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .child(header(&self.account, &self.source, self.width))
            .child(body)
    }
}

/// What fits in the window before anything is cut off. Updates come first — a
/// package you already own is worth more than one you have never met — and
/// whatever is left over belongs to the machine list.
fn fit_lists(updates: usize, available: usize, budget: f32) -> (usize, usize, f32) {
    let head = HEAD_H + HEAD_GAP;
    let mut space = budget;
    let mut shown = [0usize; 2];
    for (index, total) in [updates, available].into_iter().enumerate() {
        if total == 0 {
            continue;
        }
        let room = ((space - head + ROW_GAP) / (ROW_H + ROW_GAP)).floor().max(0.0) as usize;
        let take = total.min(room);
        shown[index] = take;
        space = (space
            - head
            - take as f32 * ROW_H
            - take.saturating_sub(1) as f32 * ROW_GAP
            - SECTION_GAP)
            .max(0.0);
    }
    (shown[0], shown[1], space)
}

/// One line that says what is waiting, instead of a billboard that says nothing.
fn status_strip(
    updates: usize,
    available: usize,
    installed: usize,
    source: &Source,
) -> impl IntoElement {
    let headline = match (updates, available) {
        (0, 0) if installed == 0 => "Nothing installed, nothing listed".to_string(),
        (0, 0) => "Everything installed is current".to_string(),
        (0, new) => format!("{new} new in the store"),
        (1, 0) => "1 update ready".to_string(),
        (ready, 0) => format!("{ready} updates ready"),
        (1, new) => format!("1 update ready · {new} new in the store"),
        (ready, new) => format!("{ready} updates ready · {new} new in the store"),
    };
    let index = match &source.name {
        Some(name) => format!("{installed} installed · {} packages · {name}", source.total),
        None => format!("{installed} installed · no store reached"),
    };
    div()
        .flex()
        .justify_between()
        .items_center()
        .h(px(STRIP_H))
        .px_4()
        .rounded_xl()
        .bg(rgb(PANEL_RAISED))
        .border_1()
        .border_color(rgb(EDGE))
        .child(
            div()
                .text_base()
                .font_weight(FontWeight::MEDIUM)
                .child(SharedString::from(headline)),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(MUTED))
                .child(SharedString::from(index)),
        )
}

fn action_section(title: &str, rows: &[&Row], total: usize, action: &str) -> impl IntoElement {
    let mut list = div().flex().flex_col().gap_2();
    for row in rows {
        list = list.child(package_row(row, Some(action)));
    }
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(section_head(title, rows.len(), total))
        .child(list)
}

/// One package, in the shape a person reads it: its own mark, what it is, what
/// it says about itself, where it is going, and — when the host can act — the
/// one thing you can do about it.
fn package_row(row: &Row, action: Option<&str>) -> impl IntoElement {
    let secondary = if !row.description.is_empty() {
        row.description.clone()
    } else if !row.summary.is_empty() {
        row.summary.clone()
    } else if let Some(change) = row.change.as_ref() {
        change.reason.clone()
    } else {
        row.id.clone()
    };
    // An update that keeps its version number is a rebuild: the digest moved,
    // the label did not. Saying so is cheaper than looking broken.
    let version = match &row.installed_version {
        Some(installed) if installed != &row.version => format!("{installed} → {}", row.version),
        Some(_) => format!("{} · rebuild", row.version),
        None => row.version.clone(),
    };
    div()
        .flex()
        .items_center()
        .gap_3()
        .h(px(ROW_H))
        .px_3()
        .rounded_lg()
        .bg(rgb(PANEL))
        .border_1()
        .border_color(rgb(EDGE))
        .child(tile(&row.mark, TILE))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .flex_1()
                .overflow_hidden()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(SharedString::from(row.title.clone())),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .whitespace_normal()
                        .line_clamp(2)
                        .child(SharedString::from(secondary)),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .when_some(row.change.as_ref(), |cluster, change| {
                    cluster.child(change_chip(&change.class))
                })
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(SharedString::from(version)),
                ),
        )
        .when_some(action, |row, label| row.child(button(label)))
}

/// The mark a package chose for itself: a rounded square in its own colour with
/// the badge it declared. A package that declared nothing gets a quiet tile
/// rather than a colour the store made up.
fn tile(mark: &Mark, size: f32) -> impl IntoElement {
    let declared = mark.color.is_some();
    let fill = mark.color.unwrap_or(PANEL_RAISED);
    let ink = if declared { BG } else { MUTED };
    div()
        .flex()
        .items_center()
        .justify_center()
        .w(px(size))
        .h(px(size))
        .rounded_lg()
        .bg(rgb(fill))
        .border_1()
        .border_color(rgb(if declared { fill } else { EDGE }))
        .child(
            div()
                .text_size(px(size * 0.4))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(ink))
                .child(SharedString::from(mark.badge.clone())),
        )
}

/// `live`, `restart`, `reboot`: what the store will make you do to get the
/// change. Quiet, because it is a fact about the update and not a warning.
fn change_chip(class: &str) -> impl IntoElement {
    div()
        .px_2()
        .py_1()
        .rounded_md()
        .bg(rgb(EDGE))
        .text_color(rgb(MUTED))
        .text_size(px(10.0))
        .font_weight(FontWeight::MEDIUM)
        .child(SharedString::from(class.to_uppercase()))
}

fn button(label: &str) -> impl IntoElement {
    div()
        .px_3()
        .py_1()
        .rounded_full()
        .bg(rgb(ACCENT))
        .text_color(rgb(BG))
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .child(SharedString::from(label.to_string()))
}

/// An installed package is a fact, not an action.
///
/// With a store to compare against, what is installed is a footnote: a line of
/// names under the things you can act on. When no store answers it is the whole
/// page, and then it earns the room a description needs.
fn machine_section(rows: &[&Row], width: f32, space: f32, detailed: bool) -> impl IntoElement {
    if detailed {
        let room = ((space - HEAD_H - HEAD_GAP + ROW_GAP) / (ROW_H + ROW_GAP))
            .floor()
            .max(0.0) as usize;
        let shown = rows.len().min(room.max(1));
        let mut list = div().flex().flex_col().gap_2();
        for row in rows.iter().take(shown) {
            list = list.child(package_row(row, None));
        }
        return div()
            .flex()
            .flex_col()
            .gap_2()
            .child(section_head("On this machine", shown, rows.len()))
            .child(list);
    }
    let columns = chips_per_line(width);
    let lines = ((space - HEAD_H - HEAD_GAP + CHIP_GAP) / (CHIP_H + CHIP_GAP))
        .floor()
        .max(0.0) as usize;
    let shown = rows.len().min((lines * columns).max(1));
    let mut grid = div().flex().flex_wrap().gap_2();
    for row in rows.iter().take(shown) {
        grid = grid.child(installed_chip(row));
    }
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(section_head("On this machine", shown, rows.len()))
        .child(grid)
}

fn installed_chip(row: &Row) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_2()
        .w(px(CHIP_W))
        .h(px(CHIP_H))
        .px_2()
        .rounded_lg()
        .bg(rgb(PANEL))
        .border_1()
        .border_color(rgb(EDGE))
        .child(tile(&row.mark, 22.0))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .overflow_hidden()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(SharedString::from(row.title.clone())),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(SharedString::from(row.version.clone())),
                ),
        )
}

fn chips_per_line(width: f32) -> usize {
    let usable = (width + CHIP_GAP).max(CHIP_W);
    (usable / (CHIP_W + CHIP_GAP)).floor().max(1.0) as usize
}

fn section_head(title: &str, shown: usize, total: usize) -> impl IntoElement {
    let detail = if shown < total {
        format!("{shown} of {total}")
    } else {
        total.to_string()
    };
    div()
        .flex()
        .justify_between()
        .items_center()
        .h(px(HEAD_H))
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(SharedString::from(title.to_string())),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(MUTED))
                .child(SharedString::from(detail)),
        )
}

fn nothing_to_show(source: &Source) -> impl IntoElement {
    let message = match &source.name {
        Some(name) => format!("{name} listed no packages, and none are installed here."),
        None => "No store answered and nothing is installed here.".to_string(),
    };
    div()
        .flex()
        .items_center()
        .justify_center()
        .flex_1()
        .child(
            div()
                .text_sm()
                .text_color(rgb(MUTED))
                .child(SharedString::from(message)),
        )
}

fn header(account: &Account, source: &Source, width: f32) -> impl IntoElement {
    let mark = match (&source.name, source.verified) {
        (None, _) => ("no store reached", MUTED),
        (Some(_), true) => ("signature verified", GOOD),
        (Some(_), false) => ("index unsigned", WARN),
    };
    div()
        .flex()
        .items_center()
        .gap_4()
        .px(px(page_margin(width)))
        .h(px(HEADER_H))
        .bg(rgb(PANEL))
        .border_b_1()
        .border_color(rgb(EDGE))
        .child(chip(account))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_lg()
                .font_weight(FontWeight::SEMIBOLD)
                .child(SharedString::from("Spore"))
                .child(
                    div()
                        .text_color(rgb(ACCENT))
                        .child(SharedString::from("Store")),
                ),
        )
        .child(div().flex_1())
        .child(
            div()
                .text_xs()
                .text_color(rgb(mark.1))
                .child(SharedString::from(mark.0)),
        )
}

fn chip(account: &Account) -> impl IntoElement {
    let initial = account
        .name
        .chars()
        .next()
        .map(|value| value.to_uppercase().to_string())
        .unwrap_or_else(|| "L".to_string());
    div()
        .flex()
        .items_center()
        .gap_2()
        .pl_1()
        .pr_3()
        .py_1()
        .rounded_full()
        .bg(rgb(BG))
        .border_1()
        .border_color(rgb(EDGE))
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .w_8()
                .h_8()
                .rounded_full()
                .bg(rgb(ACCENT))
                .text_color(rgb(BG))
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .child(SharedString::from(initial)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .child(SharedString::from(account.name.clone())),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(SharedString::from(account.note.clone())),
                ),
        )
}

/// `dev.fungos.vitals-view` has no listing yet, so the best name available is
/// its own last segment, read as words. A manifest's `title` wins wherever one
/// is declared; the host applies the same rule to its own catalog.
fn title_of(id: &str) -> String {
    let tail = id.rsplit('.').next().unwrap_or(id);
    let mut title = String::with_capacity(tail.len());
    for (index, word) in tail.split(['-', '_']).enumerate() {
        if index > 0 {
            title.push(' ');
        }
        let mut characters = word.chars();
        if let Some(first) = characters.next() {
            title.extend(first.to_uppercase());
            title.push_str(characters.as_str());
        }
    }
    title
}
