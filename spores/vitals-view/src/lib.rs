//! Vitals view Spore: GPUI lays out the cards, `wasi:webgpu` renders them, and
//! `soil:canvas-gpu` presents the texture to Canvas.
//!
//! The guest never gets a window or a swapchain. It reads the Spore-owned
//! `vitals.view.v1` record, builds a GPUI view, renders through the
//! Soil-provided GPU device, and hands the resulting texture to the host.

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
    div, px, rgb, AnyWindowHandle, Context, HeadlessAppContext, IntoElement, ParentElement,
    PlatformTextSystem as _, Render, SharedString, Styled, Window,
};
use gpui_wgpu::{CosmicTextSystem, WgpuContext, WgpuHeadlessRenderer};
use serde_json::{json, Value};
use soil::canvas_gpu::surface;
use soil::records::records;

/// IBM Plex Sans, SIL Open Font License 1.1. A wasm component has no system
/// fonts, so the Spore ships the one face its view names.
const FONT: &[u8] = include_bytes!("../assets/IBMPlexSans-Regular.ttf");
const FONT_FAMILY: &str = "IBM Plex Sans";

struct Component;
export!(Component);

impl Guest for Component {
    async fn run() -> u32 {
        match render() {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("vitals-view: {error}");
                1
            }
        }
    }
}

fn load_view() -> Value {
    let key = records::Key {
        collection: "vitals.view".to_string(),
        id: "owner".to_string(),
    };
    let entry = match records::get(&key) {
        Ok(entry) => entry,
        Err(records::Error::NotFound) => return empty_view("owner"),
        Err(error) => {
            eprintln!("vitals-view: get vitals.view/owner: {error:?}");
            return empty_view("owner");
        }
    };
    match serde_json::from_slice(&entry.body) {
        Ok(view) => view,
        Err(error) => {
            eprintln!("vitals-view: parse vitals.view/owner: {error}");
            empty_view("owner")
        }
    }
}

fn empty_view(wearer: &str) -> Value {
    json!({
        "schema": "vitals.view.v1",
        "title": "Vitals",
        "wearer": wearer,
        "freshness": "unknown",
        "empty": true,
        "cards": [],
        "steps": {"rows": []},
    })
}

fn render() -> Result<(), String> {
    let (width, height) = surface::size();
    let (width, height) = (width.max(1), height.max(1));

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

    let view = load_view();
    let window = app
        .open_window_scaled(
            gpui::size(px(width as f32), px(height as f32)),
            1.0,
            move |_, cx| cx.new(|_| VitalsView { view }),
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
        label: Some("vitals GPUI frame"),
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

struct VitalsView {
    view: Value,
}

impl Render for VitalsView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let title = self.view["title"].as_str().unwrap_or("Vitals");
        let wearer = self.view["wearer"].as_str().unwrap_or("unknown");
        let freshness = self.view["freshness"].as_str().unwrap_or("unknown");
        let cards = self.view["cards"].as_array().cloned().unwrap_or_default();
        let rows = self.view["steps"]["rows"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        let mut root = div()
            .flex()
            .flex_col()
            .gap_4()
            .p_6()
            .size_full()
            .bg(rgb(0x0d1117))
            .text_color(rgb(0xe6edf3))
            .child(
                div()
                    .text_2xl()
                    .child(SharedString::from(title.to_string())),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x8b949e))
                    .child(SharedString::from(format!(
                        "Wearer: {wearer} · {freshness}"
                    ))),
            );

        if cards.is_empty() {
            root = root.child(
                div()
                    .p_4()
                    .rounded_lg()
                    .bg(rgb(0x161b22))
                    .text_color(rgb(0x8b949e))
                    .child("No vitals view recorded yet. Run the vitals Spore seed/once first."),
            );
        } else {
            let mut grid = div().flex().flex_wrap().gap_4();
            for card in &cards {
                grid = grid.child(card_element(card));
            }
            root = root.child(grid);
        }

        if !rows.is_empty() {
            let mut steps = div()
                .flex()
                .flex_col()
                .gap_1()
                .p_3()
                .rounded_lg()
                .bg(rgb(0x161b22))
                .child(div().text_sm().text_color(rgb(0x8b949e)).child("Steps"));
            for row in &rows {
                let device = row["device"].as_str().unwrap_or("device");
                let count = row["count"].as_u64().unwrap_or(0);
                steps = steps.child(
                    div()
                        .flex()
                        .justify_between()
                        .gap_8()
                        .child(SharedString::from(device.to_string()))
                        .child(SharedString::from(count.to_string())),
                );
            }
            root = root.child(steps);
        }

        root
    }
}

fn card_element(card: &Value) -> impl IntoElement {
    let label = card["label"].as_str().unwrap_or("");
    let value = display_value(&card["value"]);
    let unit = card["unit"].as_str().unwrap_or("");
    let display = total_display(&value, unit);
    let source = card["source_device"].as_str().unwrap_or("no source");
    let freshness = card["freshness"].as_str().unwrap_or("unknown");

    div()
        .flex()
        .flex_col()
        .gap_1()
        .w(px(340.0))
        .p_3()
        .rounded_lg()
        .bg(rgb(0x161b22))
        .child(
            div()
                .text_sm()
                .text_color(rgb(0x8b949e))
                .child(SharedString::from(label.to_string())),
        )
        .child(div().text_2xl().child(SharedString::from(display)))
        .child(
            div()
                .text_xs()
                .text_color(rgb(0x6e7681))
                .child(SharedString::from(format!("{source} · {freshness}"))),
        )
}

fn display_value(value: &Value) -> String {
    match value {
        Value::Null => "—".to_string(),
        Value::String(value) => value.clone(),
        Value::Number(value) => {
            if let Some(integer) = value.as_i64() {
                integer.to_string()
            } else if let Some(integer) = value.as_u64() {
                integer.to_string()
            } else {
                format!("{:.1}", value.as_f64().unwrap_or_default())
            }
        }
        other => other.to_string(),
    }
}

fn total_display(value: &str, unit: &str) -> String {
    match unit.trim() {
        "" => value.to_string(),
        "%" => format!("{value}%"),
        "C" => format!("{value} °C"),
        unit => format!("{value} {unit}"),
    }
}
