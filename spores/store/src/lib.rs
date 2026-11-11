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
use soil::canvas_gpu::surface;
use soil::http::http;

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

fn get(path: &str) -> Result<Vec<u8>, String> {
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
        timeout_ms: 5_000,
    };
    let response = http::fetch(&request).map_err(|error| format!("fetch {path}: {error:?}"))?;
    if response.status != 200 {
        return Err(format!("fetch {path}: status {}", response.status));
    }
    Ok(response.body)
}

fn catalog() -> Result<(Vec<Row>, String), String> {
    let body = get("/api/catalog")?;
    let value: Value = serde_json::from_slice(&body).map_err(|error| format!("catalog: {error}"))?;
    let source = value["source"]["name"].as_str().unwrap_or("store").to_string();
    let mut rows = Vec::new();
    for entry in value["entries"].as_array().cloned().unwrap_or_default() {
        let document = &entry["document"];
        rows.push(Row {
            name: document["name"].as_str().or(entry["id"].as_str()).unwrap_or("?").to_string(),
            summary: document["summary"].as_str().unwrap_or("").to_string(),
            version: entry["version"].as_str().unwrap_or("").to_string(),
            state: entry["state"].as_str().unwrap_or("available").to_string(),
            rating: entry["ratings"]["current"]["weighted"]
                .as_f64()
                .filter(|value| *value > 0.0),
            publisher: entry["publisher"].as_str().unwrap_or("unknown").to_string(),
        });
    }
    rows.sort_by_key(|row| row.name.to_lowercase());
    Ok((rows, source))
}

/// Who is asking, as far as the store can tell.
fn account() -> String {
    let Ok(body) = get("/api/whoami") else {
        return "local".to_string();
    };
    let value: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    value["principal"].as_str().unwrap_or("local").to_string()
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

    let (rows, source) = catalog()?;
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
        let columns = if self.width >= 1180.0 {
            4
        } else if self.width >= 900.0 {
            3
        } else if self.width >= 620.0 {
            2
        } else {
            1
        };
        let mut grid = div().flex().flex_wrap().gap_4();
        for row in &self.rows {
            grid = grid.child(card(row));
        }

        let body = if self.rows.is_empty() {
            div()
                .p_8()
                .text_color(rgb(MUTED))
                .child(SharedString::from("This store lists nothing yet."))
        } else {
            grid
        };

        let _ = (columns, self.height);
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .child(header(&self.account, &self.source, self.width))
            .child(div().flex().flex_col().p_6().child(body))
    }
}

fn header(account: &str, source: &str, width: f32) -> impl IntoElement {
    let mut bar = div()
        .flex()
        .items_center()
        .gap_4()
        .px_6()
        .py_4()
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

fn card(row: &Row) -> impl IntoElement {
    let state_color = match row.state.as_str() {
        "installed" => GOOD,
        "update" => WARN,
        _ => ACCENT,
    };
    let monogram = row
        .name
        .chars()
        .next()
        .map(|value| value.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string());
    let rating = row
        .rating
        .map(|value| format!("★ {value:.1}"))
        .unwrap_or_else(|| "no reviews".to_string());

    div()
        .flex()
        .flex_col()
        .gap_3()
        .p_4()
        .w_72()
        .rounded_xl()
        .bg(rgb(PANEL))
        .border_1()
        .border_color(rgb(EDGE))
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .w_12()
                        .h_12()
                        .rounded_xl()
                        .bg(rgb(EDGE))
                        .text_color(rgb(ACCENT))
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(SharedString::from(monogram)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(SharedString::from(row.name.clone())),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child(SharedString::from(row.publisher.clone())),
                        ),
                ),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(MUTED))
                .child(SharedString::from(row.summary.clone())),
        )
        .child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .child(
                    div()
                        .px_2()
                        .py_1()
                        .rounded_full()
                        .bg(rgb(EDGE))
                        .text_color(rgb(state_color))
                        .text_xs()
                        .child(SharedString::from(row.state.clone())),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(STAR))
                        .child(SharedString::from(rating)),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(SharedString::from(row.version.clone())),
                ),
        )
}
