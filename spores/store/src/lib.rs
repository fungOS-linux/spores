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
use serde_json::Value;
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
const STAR: u32 = 0xf2c14e;

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

struct Row {
    name: String,
    summary: String,
    version: String,
    state: String,
    rating: Option<f64>,
    publisher: String,
}

/// The remote catalog, as the store service publishes it. A host that grants no
/// origin answers nothing, and the view falls back to what is installed.
fn remote() -> Option<serde_json::Value> {
    let request = http::Request {
        url: format!("{ORIGIN}/api/catalog"),
        method: http::Method::Get,
        headers: vec![http::Header { name: "accept".to_string(), value: "application/json".to_string() }],
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

fn catalog_rows() -> Result<(Vec<Row>, String), String> {
    let installed = catalog::entries().map_err(|error| format!("catalog: {error:?}"))?;
    let mut rows: Vec<Row> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    if let Some(remote) = remote() {
        for entry in remote["entries"].as_array().cloned().unwrap_or_default() {
            let id = entry["id"].as_str().unwrap_or("").to_string();
            let document = &entry["document"];
            seen.insert(id.clone());
            rows.push(Row {
                name: document["name"].as_str().or(entry["id"].as_str()).unwrap_or("?").to_string(),
                summary: document["summary"].as_str().unwrap_or("").to_string(),
                version: entry["version"].as_str().unwrap_or("").to_string(),
                state: entry["state"].as_str().unwrap_or("available").to_string(),
                rating: entry["ratings"]["current"]["weighted"].as_f64().filter(|value| *value > 0.0),
                publisher: entry["publisher"].as_str().unwrap_or("unknown").to_string(),
            });
        }
    }
    for entry in installed {
        if seen.contains(&entry.id) {
            continue;
        }
        rows.push(Row {
            name: if entry.title.is_empty() { entry.name.clone() } else { entry.title.clone() },
            summary: entry.summary.clone(),
            version: entry.version.clone(),
            state: "installed".to_string(),
            rating: None,
            publisher: "installed".to_string(),
        });
    }
    rows.sort_by_key(|row| row.name.to_lowercase());
    Ok((rows, "store".to_string()))
}

/// Who is asking. Without a gateway on this path the honest answer is the
/// machine, not an invented account.
fn account() -> String {
    "local".to_string()
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

struct StoreView {
    rows: Vec<Row>,
    source: String,
    account: String,
    width: f32,
    height: f32,
}

impl Render for StoreView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        // Featured is the thing most worth doing: an update you already have,
        // then something new, then anything at all. A shopfront with no
        // recommendation is a filing cabinet.
        let featured = self
            .rows
            .iter()
            .find(|row| row.state == "update")
            .or_else(|| self.rows.iter().find(|row| row.state == "available"))
            .or_else(|| self.rows.first());

        let by_state = |state: &'static str| {
            self.rows
                .iter()
                .filter(move |row| row.state == state)
                .collect::<Vec<&Row>>()
        };
        let updates = by_state("update");
        let available = by_state("available");
        let installed = by_state("installed");

        let mut body = div()
            .flex()
            .flex_col()
            .gap_6()
            .p_6()
            .flex_1()
            .overflow_hidden();
        if let Some(row) = featured {
            body = body.child(hero(row, self.width));
        }
        if !updates.is_empty() {
            body = body.child(shelf("Updates", &updates));
        }
        if !available.is_empty() {
            body = body.child(shelf("On the store", &available));
        }
        if !installed.is_empty() {
            body = body.child(shelf("Installed on this machine", &installed));
        }

        let _ = self.height;
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

/// The featured band: art, name, one sentence, and the state you would act on.
fn hero(row: &Row, width: f32) -> impl IntoElement {
    let tint = tint(&row.name);
    let short = width < 760.0;
    div()
        .flex()
        .items_center()
        .gap_6()
        .p_6()
        .h_72()
        .rounded_2xl()
        .bg(rgb(PANEL_RAISED))
        .border_1()
        .border_color(rgb(EDGE))
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .w(px(if short { 96.0 } else { 168.0 }))
                .h(px(if short { 96.0 } else { 168.0 }))
                .rounded_2xl()
                .bg(rgb(tint))
                .text_color(rgb(BG))
                .text_3xl()
                .font_weight(FontWeight::SEMIBOLD)
                .child(SharedString::from(initial(&row.name))),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .flex_1()
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(ACCENT))
                        .child(SharedString::from("FEATURED")),
                )
                .child(
                    div()
                        .text_3xl()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(SharedString::from(display_name(row))),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(MUTED))
                        .child(SharedString::from(row.summary.clone())),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(state_pill(&row.state))
                        .child(
                            div()
                                .px_4()
                                .py_2()
                                .rounded_full()
                                .bg(rgb(ACCENT))
                                .text_color(rgb(BG))
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .child(SharedString::from(if row.state == "update" {
                                    "Update"
                                } else if row.state == "available" {
                                    "Get"
                                } else {
                                    "Open"
                                })),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child(SharedString::from(format!("{} · {}", row.publisher, row.version))),
                        ),
                ),
        )
}

fn shelf(title: &str, rows: &[&Row]) -> impl IntoElement {
    let shown = rows.len().min(5);
    let mut strip = div().flex().gap_4();
    for row in rows.iter().take(shown) {
        strip = strip.child(card(row));
    }
    let more = rows.len().saturating_sub(shown);
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .flex()
                .justify_between()
                .items_end()
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(SharedString::from(title.to_string())),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(SharedString::from(if more == 0 {
                            format!("{}", rows.len())
                        } else {
                            format!("{} · {} more", rows.len(), more)
                        })),
                ),
        )
        .child(strip)
}

fn card(row: &Row) -> impl IntoElement {
    let tint = tint(&row.name);
    div()
        .flex()
        .flex_col()
        .gap_3()
        .p_4()
        .w(px(228.0))
        .rounded_xl()
        .bg(rgb(PANEL))
        .border_1()
        .border_color(rgb(EDGE))
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .w_12()
                .h_12()
                .rounded_xl()
                .bg(rgb(tint))
                .text_color(rgb(BG))
                .text_lg()
                .font_weight(FontWeight::SEMIBOLD)
                .child(SharedString::from(initial(&row.name))),
        )
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(SharedString::from(display_name(row))),
        )
        .child(
            div()
                .h(px(34.0))
                .text_xs()
                .text_color(rgb(MUTED))
                .child(SharedString::from(row.summary.clone())),
        )
        .child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .child(state_pill(&row.state))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(STAR))
                        .child(SharedString::from(rating(row))),
                ),
        )
}

fn state_pill(state: &str) -> impl IntoElement {
    let color = match state {
        "installed" => GOOD,
        "update" => WARN,
        _ => ACCENT,
    };
    div()
        .px_2()
        .py_1()
        .rounded_full()
        .bg(rgb(EDGE))
        .text_color(rgb(color))
        .text_xs()
        .child(SharedString::from(state.to_string()))
}

fn rating(row: &Row) -> String {
    row.rating
        .map(|value| format!("★ {value:.1}"))
        .unwrap_or_else(|| "—".to_string())
}

/// A name a person reads: `dev.fungos.gallery` is the id, `Gallery` is the app.
fn display_name(row: &Row) -> String {
    if row.name.contains('.') {
        let last = row.name.rsplit('.').next().unwrap_or(&row.name);
        let mut characters = last.chars();
        return match characters.next() {
            Some(first) => first.to_uppercase().chain(characters).collect(),
            None => row.name.clone(),
        };
    }
    row.name.clone()
}

fn initial(name: &str) -> String {
    display_name(&Row {
        name: name.to_string(),
        summary: String::new(),
        version: String::new(),
        state: String::new(),
        rating: None,
        publisher: String::new(),
    })
    .chars()
    .next()
    .map(|value| value.to_uppercase().to_string())
    .unwrap_or_else(|| "?".to_string())
}

/// Generated art: a stable tint per Spore, so a shelf reads as distinct things
/// before any listing artwork exists. A real icon, when the store has one,
/// replaces this.
fn tint(seed: &str) -> u32 {
    const PALETTE: [u32; 6] = [0x8b9dff, 0x5ee0c0, 0xffb86c, 0xff8fab, 0x9d7bff, 0x6fd3ff];
    let mut hash: u32 = 2_166_136_261;
    for byte in seed.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    PALETTE[(hash as usize) % PALETTE.len()]
}

fn header(account: &str, source: &str, width: f32) -> impl IntoElement {
    let mut bar = div()
        .flex()
        .items_center()
        .gap_4()
        .px_6()
        .py_3()
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
        );
    if width >= 700.0 {
        bar = bar.child(
            div()
                .flex_1()
                .px_3()
                .py_2()
                .rounded_lg()
                .bg(rgb(BG))
                .border_1()
                .border_color(rgb(EDGE))
                .text_sm()
                .text_color(rgb(MUTED))
                .child(SharedString::from("Search the store")),
        );
    }
    bar.child(
        div()
            .text_xs()
            .text_color(rgb(MUTED))
            .child(SharedString::from(format!("{source} · verified catalog"))),
    )
}

fn chip(account: &str) -> impl IntoElement {
    let name = account.strip_prefix("mycelium:").unwrap_or(account).to_string();
    let initial = name
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
                        .child(SharedString::from(name)),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(SharedString::from(if account == "local" {
                            "this machine"
                        } else {
                            "signed in"
                        })),
                ),
        )
}
