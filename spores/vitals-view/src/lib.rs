//! Vitals view Spore: GPUI lays out the cards and activity rings,
//! `wasi:webgpu` renders them, and `soil:present` presents the texture to
//! Canvas.
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
use std::collections::BTreeMap;
use std::f32::consts::{FRAC_PI_2, TAU};
use std::sync::Arc;

use gpui::prelude::*;
use gpui::{
    canvas, div, point, px, relative, rgb, AnyWindowHandle, Bounds, Context, FontWeight,
    HeadlessAppContext, IntoElement, ParentElement, Path, PathBuilder, PathStyle, Pixels,
    PlatformTextSystem as _, Point, Render, Rgba, SharedString, StrokeOptions, Styled, Window,
};
use gpui_wgpu::{CosmicTextSystem, WgpuContext, WgpuHeadlessRenderer};
use lyon::tessellation::{LineCap, LineJoin};
use serde_json::{json, Value};
use soil::present::surface;
use soil::records::records;

/// IBM Plex Sans, SIL Open Font License 1.1. A wasm component has no system
/// fonts, so the Spore ships the one face its view names.
const FONT: &[u8] = include_bytes!("../assets/IBMPlexSans-Regular.ttf");
const FONT_FAMILY: &str = "IBM Plex Sans";

const BG: u32 = 0x070b12;
const PANEL: u32 = 0x0e1622;
const PANEL_RAISED: u32 = 0x111c2a;
const TRACK: u32 = 0x1b2938;
const TEXT: u32 = 0xe7eef7;
const MUTED: u32 = 0x8b9bb0;
const DIM: u32 = 0x5d7088;
const GREEN: u32 = 0x32e88b;
const ROSE: u32 = 0xff5d8f;
const CYAN: u32 = 0x54d6ff;
const AMBER: u32 = 0xf5b95b;
const PURPLE: u32 = 0xb59cff;
const ORANGE: u32 = 0xff9b5c;

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

fn load_view() -> Result<Value, String> {
    let key = records::Key {
        collection: "vitals.view".to_string(),
        id: "owner".to_string(),
    };
    let entry = match records::get(&key) {
        Ok(entry) => entry,
        Err(records::Error::NotFound) => return Ok(empty_view("owner")),
        Err(error) => return Err(format!("get vitals.view/owner: {error:?}")),
    };
    if entry.deleted {
        return Ok(empty_view("owner"));
    }
    let view: Value = serde_json::from_slice(&entry.body)
        .map_err(|error| format!("parse vitals.view/owner: {error}"))?;
    if view.get("schema").and_then(Value::as_str) != Some("vitals.view.v1") {
        return Err("vitals.view/owner does not declare vitals.view.v1".to_string());
    }
    Ok(view)
}

fn empty_view(wearer: &str) -> Value {
    json!({
        "schema": "vitals.view.v1",
        "title": "Vitals",
        "wearer": wearer,
        "freshness": "unknown",
        "empty": true,
        "rings": [],
        "cards": [],
        "steps": {"rows": []},
    })
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

    let view = load_view()?;
    let viewport = (width as f32, height as f32);
    let window = app
        .open_window_scaled(
            gpui::size(px(viewport.0), px(viewport.1)),
            1.0,
            move |_, cx| {
                cx.new(|_| VitalsView {
                    view,
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
    surface::present(&texture).map_err(|error| format!("present: {error}"))
}

struct VitalsView {
    view: Value,
    width: f32,
    height: f32,
}

impl Render for VitalsView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        if self.width >= 1100.0 && self.height >= 880.0 {
            render_wide(&self.view, self.width).into_any_element()
        } else {
            render_compact(&self.view, self.width, self.height).into_any_element()
        }
    }
}

fn render_wide(view: &Value, width: f32) -> impl IntoElement {
    let title = view["title"].as_str().unwrap_or("Vitals");
    let wearer = view["wearer"].as_str().unwrap_or("unknown");
    let freshness = view["freshness"].as_str().unwrap_or("unknown");
    let device_count = view["devices"]
        .as_array()
        .map(|devices| devices.len())
        .unwrap_or(0);
    let cards = view["cards"].as_array().cloned().unwrap_or_default();
    let rows = view["steps"]["rows"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let rings = parse_rings(view);
    let display_rings = if rings.is_empty() {
        placeholder_rings()
    } else {
        rings
    };
    let sleep = parse_sleep(view);
    let estimate = Some(&view["steps"]["estimate"]);
    let empty = view["empty"].as_bool().unwrap_or(false);

    let mut root = div()
        .flex()
        .flex_col()
        .gap_5()
        .p_6()
        .size_full()
        .bg(rgb(BG))
        .text_color(rgb(TEXT))
        .child(header(title, wearer, freshness, device_count, false, true));

    if empty {
        root = root.child(empty_banner());
    }

    let content_width = (width - 48.0).max(320.0);
    let ring_width = (content_width * 0.30).clamp(320.0, 400.0);
    let grid_width = content_width - ring_width - 16.0;
    let card_width = ((grid_width - 12.0) / 2.0).max(260.0);

    let mut hero = div().flex().items_center().gap_4().child(ring_panel(
        &display_rings,
        wearer,
        freshness,
        false,
        ring_width,
        320.0,
    ));
    if cards.is_empty() {
        hero = hero.child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .p_5()
                .rounded_xl()
                .bg(rgb(PANEL))
                .text_color(rgb(MUTED))
                .child("Waiting for the first vitals view."),
        );
    } else {
        let mut grid = div()
            .flex()
            .flex_wrap()
            .gap_3()
            .w(px(grid_width))
            .max_w_full();
        for card in &cards {
            grid = grid.child(card_element(card, card_width, false));
        }
        hero = hero.child(grid);
    }
    root = root.child(hero);

    if sleep.is_some() && !rows.is_empty() {
        root = root.child(
            div()
                .flex()
                .items_stretch()
                .gap_4()
                .child(sleep_panel(sleep.as_ref().expect("sleep checked")))
                .child(steps_panel(&rows, false, estimate)),
        );
    } else if let Some(sleep) = &sleep {
        root = root.child(sleep_panel(sleep));
    } else if !rows.is_empty() {
        root = root.child(steps_panel(&rows, true, estimate));
    }

    root
}

fn render_compact(view: &Value, width: f32, height: f32) -> impl IntoElement {
    let title = view["title"].as_str().unwrap_or("Vitals");
    let wearer = view["wearer"].as_str().unwrap_or("unknown");
    let freshness = view["freshness"].as_str().unwrap_or("unknown");
    let device_count = view["devices"]
        .as_array()
        .map(|devices| devices.len())
        .unwrap_or(0);
    let cards = view["cards"].as_array().cloned().unwrap_or_default();
    let rings = parse_rings(view);
    let display_rings = if rings.is_empty() {
        placeholder_rings()
    } else {
        rings
    };
    let sleep = parse_sleep(view);
    let tiny = height < 560.0;
    let empty = view["empty"].as_bool().unwrap_or(false);
    let available = (width - 24.0).max(240.0);
    let content_width = available.min(560.0);
    let tiny_strip = tiny && sleep.is_some() && content_width < 400.0 && height >= 470.0;
    let canvas_cap = if tiny_strip { 180.0 } else { 240.0 };

    let mut stack = div()
        .flex()
        .flex_col()
        .flex_grow(1.0)
        .gap_2()
        .w(px(content_width))
        .max_w_full()
        .child(header(title, wearer, freshness, device_count, true, !tiny));

    if empty {
        stack = stack.child(empty_banner());
    }

    let rings = ring_panel(
        &display_rings,
        wearer,
        freshness,
        true,
        content_width,
        canvas_cap,
    );

    if tiny {
        if let (true, Some(sleep)) = (tiny_strip, sleep.as_ref()) {
            stack = stack.child(rings).child(sleep_strip(sleep, false));
        } else {
            stack = stack.child(
                div()
                    .flex()
                    .flex_grow(1.0)
                    .items_center()
                    .justify_center()
                    .w_full()
                    .child(rings),
            );
        }
    } else {
        stack = stack.child(rings).child({
            let wanted = if height >= 740.0 { 4 } else { 2 };
            let order = ["heart_rate", "sleep", "spo2", "temperature"];
            let selected: Vec<&Value> = order
                .iter()
                .filter_map(|id| cards.iter().find(|card| card["id"].as_str() == Some(*id)))
                .take(wanted)
                .collect();
            let card_width = (content_width - 8.0) / 2.0;
            let mut grid = div().flex().flex_wrap().gap_2().w_full();
            for card in selected {
                grid = grid.child(card_element(card, card_width, true));
            }
            grid
        });

        if let Some(sleep) = &sleep {
            if height >= 620.0 {
                stack = stack.child(sleep_strip(sleep, true));
            }
        }
    }

    div()
        .flex()
        .flex_col()
        .items_center()
        .p_3()
        .size_full()
        .bg(rgb(BG))
        .text_color(rgb(TEXT))
        .child(stack)
}

fn empty_banner() -> impl IntoElement {
    div()
        .p_4()
        .rounded_xl()
        .bg(rgb(0x122033))
        .text_sm()
        .text_color(rgb(0x9ec7ff))
        .child("No vitals view recorded yet. Run the vitals Spore seed/once first.")
}

fn header(
    title: &str,
    wearer: &str,
    freshness: &str,
    device_count: usize,
    compact: bool,
    show_subtitle: bool,
) -> impl IntoElement {
    let title_style = if compact {
        div().text_2xl()
    } else {
        div().text_3xl()
    };
    let subtitle = if compact {
        format!("{wearer} · {freshness} · {device_count} devices")
    } else {
        format!("Wearer: {wearer} · {freshness} · {device_count} devices")
    };
    let mut title_block = div().flex().flex_col().gap_1().child(
        title_style
            .font_weight(FontWeight::SEMIBOLD)
            .child(SharedString::from(title.to_string())),
    );
    if show_subtitle {
        title_block = title_block.child(
            div()
                .text_xs()
                .text_color(rgb(MUTED))
                .child(SharedString::from(subtitle)),
        );
    }
    let mut header = div()
        .flex()
        .justify_between()
        .items_center()
        .child(title_block);
    if !compact {
        header = header.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_1()
                .rounded_full()
                .bg(rgb(0x10251b))
                .child(dot(rgb(GREEN)))
                .child(div().text_xs().text_color(rgb(GREEN)).child("LIVE")),
        );
    }
    header
}

fn steps_panel(rows: &[Value], full: bool, estimate: Option<&Value>) -> impl IntoElement {
    let mut row_list = div().flex().flex_col().gap_2();
    for row in rows {
        row_list = row_list.child(step_row(row));
    }
    let mut panel = div()
        .flex()
        .flex_col()
        .gap_3()
        .p_4()
        .rounded_xl()
        .bg(rgb(PANEL))
        .max_w_full()
        .child(
            div()
                .flex()
                .flex_wrap()
                .justify_between()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(MUTED))
                        .child("Steps by device"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(DIM))
                        .child("per device · never summed"),
                ),
        )
        .child(row_list);
    if let Some(estimate) = estimate {
        if let Some(value) = estimate["value"].as_u64() {
            let covered = estimate["coverage"]["covered_buckets"]
                .as_u64()
                .unwrap_or(0);
            let total = estimate["coverage"]["total_buckets"].as_u64().unwrap_or(0);
            let fraction = (value as f64 / 10_000.0).clamp(0.0, 1.0) as f32;
            panel =
                panel.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child("Move estimate · fused"),
                        )
                        .child(
                            div()
                                .flex()
                                .justify_between()
                                .items_center()
                                .child(
                                    div()
                                        .text_2xl()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(SharedString::from(format_count(value as f64))),
                                )
                                .child(div().text_xs().text_color(rgb(DIM)).child(
                                    SharedString::from(format!("{covered}/{total} buckets")),
                                )),
                        )
                        .child(
                            div()
                                .h(px(4.0))
                                .w_full()
                                .rounded_full()
                                .bg(rgb(TRACK))
                                .child(
                                    div()
                                        .h_full()
                                        .w(relative(fraction))
                                        .rounded_full()
                                        .bg(rgb(GREEN)),
                                ),
                        ),
                );
        }
    }
    if full {
        panel.w_full()
    } else {
        panel.flex_basis(px(0.0)).flex_grow(1.0)
    }
}

#[derive(Clone)]
struct SleepSummary {
    start: u64,
    end: u64,
    time_in_bed_ms: u64,
    sleep_ms: u64,
    efficiency: f64,
    stage_ms: BTreeMap<String, u64>,
    stages: Vec<SleepStage>,
    source: String,
}

#[derive(Clone)]
struct SleepStage {
    stage: String,
    start: u64,
    end: u64,
}

fn parse_sleep(view: &Value) -> Option<SleepSummary> {
    let value = &view["sleep_session"];
    if !value.is_object() {
        return None;
    }
    let start = value["start"].as_u64()?;
    let end = value["end"].as_u64()?;
    if end <= start {
        return None;
    }
    let stages = value["stages"]
        .as_array()?
        .iter()
        .filter_map(|stage| {
            Some(SleepStage {
                stage: stage["stage"].as_str()?.to_string(),
                start: stage["start"].as_u64()?,
                end: stage["end"].as_u64()?,
            })
        })
        .collect();
    let mut stage_ms = BTreeMap::new();
    for stage in ["awake", "light", "deep", "rem"] {
        stage_ms.insert(stage.to_string(), 0);
    }
    if let Some(values) = value["stage_ms"].as_object() {
        for (stage, duration) in values {
            stage_ms.insert(stage.clone(), duration.as_u64().unwrap_or(0));
        }
    }
    Some(SleepSummary {
        start,
        end,
        time_in_bed_ms: value["time_in_bed_ms"].as_u64()?,
        sleep_ms: value["sleep_ms"].as_u64()?,
        efficiency: value["efficiency"].as_f64()?,
        stage_ms,
        stages,
        source: value["source_device"]
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
    })
}

fn sleep_panel(sleep: &SleepSummary) -> impl IntoElement {
    let chart = sleep_chart(sleep, 74.0);

    let mut legend = div().flex().flex_wrap().gap_3();
    for stage in ["awake", "rem", "light", "deep"] {
        let duration = sleep.stage_ms.get(stage).copied().unwrap_or(0);
        legend = legend.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(dot(sleep_stage_color(stage)))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(SharedString::from(format!(
                            "{} {}",
                            sleep_stage_label(stage),
                            format_duration_ms(duration)
                        ))),
                ),
        );
    }

    div()
        .flex()
        .flex_col()
        .gap_3()
        .p_4()
        .rounded_xl()
        .bg(rgb(PANEL))
        .max_w_full()
        .flex_basis(px(0.0))
        .flex_grow(2.0)
        .child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().text_sm().text_color(rgb(MUTED)).child("Sleep"))
                        .child(
                            div()
                                .text_3xl()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(SharedString::from(format_duration_ms(sleep.sleep_ms))),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_end()
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(TEXT))
                                .child(SharedString::from(format!(
                                    "{:.0}% efficiency",
                                    sleep.efficiency * 100.0
                                ))),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(DIM))
                                .child(SharedString::from(format!(
                                    "{} in bed · {}",
                                    format_duration_ms(sleep.time_in_bed_ms),
                                    sleep.source
                                ))),
                        ),
                ),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(DIM))
                .child(SharedString::from(format!(
                    "{} → {} UTC",
                    clock_utc(sleep.start),
                    clock_utc(sleep.end)
                ))),
        )
        .child(chart)
        .child(legend)
}

fn sleep_strip(sleep: &SleepSummary, full: bool) -> impl IntoElement {
    let summary = ["awake", "rem", "light", "deep"]
        .iter()
        .map(|stage| {
            format!(
                "{} {}",
                sleep_stage_label(stage),
                format_duration_ms(sleep.stage_ms.get(*stage).copied().unwrap_or(0))
            )
        })
        .collect::<Vec<_>>()
        .join(" · ");
    let mut strip = div()
        .flex()
        .flex_col()
        .gap_1()
        .p_2()
        .rounded_xl()
        .bg(rgb(PANEL))
        .w_full()
        .child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child("Sleep timeline"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(DIM))
                        .child(SharedString::from(format!(
                            "{} → {} UTC",
                            clock_utc(sleep.start),
                            clock_utc(sleep.end)
                        ))),
                ),
        )
        .child(sleep_chart(sleep, if full { 40.0 } else { 32.0 }));
    if full {
        strip = strip.child(
            div()
                .text_xs()
                .text_color(rgb(DIM))
                .child(SharedString::from(summary)),
        );
    }
    strip
}

fn sleep_chart(sleep: &SleepSummary, height: f32) -> impl IntoElement {
    let total = sleep.end.saturating_sub(sleep.start).max(1);
    let inner_height = (height - 16.0).max(20.0);
    let row_height = inner_height / 4.0;
    let bar_height = (row_height * 0.58).max(2.0);
    let mut inner = div().relative().h(px(inner_height)).w_full();
    for stage in &sleep.stages {
        let start = stage.start.max(sleep.start).min(sleep.end);
        let end = stage.end.max(sleep.start).min(sleep.end);
        if end <= start {
            continue;
        }
        let left = (start - sleep.start) as f64 / total as f64;
        let width = (end - start) as f64 / total as f64;
        let row = sleep_stage_index(&stage.stage) as f32;
        inner = inner.child(
            div()
                .absolute()
                .top(px(row * row_height + (row_height - bar_height) / 2.0))
                .left(relative(left as f32))
                .w(relative(width as f32))
                .h(px(bar_height))
                .rounded_xs()
                .bg(sleep_stage_color(&stage.stage)),
        );
    }
    div()
        .relative()
        .h(px(height))
        .w_full()
        .rounded_lg()
        .bg(rgb(TRACK))
        .px(px(12.0))
        .py(px(8.0))
        .overflow_hidden()
        .child(inner)
}

fn sleep_stage_color(stage: &str) -> Rgba {
    match stage {
        "awake" => rgb(AMBER),
        "rem" => rgb(CYAN),
        "light" => rgb(0x6ea8ff),
        "deep" => rgb(PURPLE),
        _ => rgb(DIM),
    }
}

fn sleep_stage_index(stage: &str) -> usize {
    match stage {
        "awake" => 0,
        "rem" => 1,
        "light" => 2,
        "deep" => 3,
        _ => 0,
    }
}

fn sleep_stage_label(stage: &str) -> &'static str {
    match stage {
        "awake" => "Awake",
        "rem" => "REM",
        "light" => "Light",
        "deep" => "Deep",
        _ => "Unknown",
    }
}

fn format_duration_ms(ms: u64) -> String {
    let minutes = ms / 60_000;
    let hours = minutes / 60;
    let remainder = minutes % 60;
    if hours > 0 {
        format!("{hours}h {remainder}m")
    } else {
        format!("{remainder}m")
    }
}

fn clock_utc(ms: u64) -> String {
    let day = ms % 86_400_000;
    format!("{:02}:{:02}", day / 3_600_000, (day / 60_000) % 60)
}

fn ring_panel(
    rings: &[Ring],
    wearer: &str,
    freshness: &str,
    compact: bool,
    available_width: f32,
    canvas_cap: f32,
) -> impl IntoElement {
    let panel_width = available_width;
    let canvas_size = if compact {
        ((panel_width - 24.0) * 0.7).clamp(180.0, canvas_cap)
    } else {
        320.0
    };
    let center = rings
        .iter()
        .find(|ring| ring.id == "heart")
        .or_else(|| rings.first());
    let center_value = center
        .map(Ring::display_value)
        .unwrap_or_else(|| "—".to_string());
    let center_unit = center.map(|ring| ring.unit.clone()).unwrap_or_default();
    let center_label = center.map(|ring| ring.label.clone()).unwrap_or_default();

    let paint_specs = rings.to_vec();
    let legend =
        if compact {
            let mut legend = div().flex().items_start().justify_between().gap_2();
            for ring in rings {
                legend =
                    legend.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .flex_grow(1.0)
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(dot(ring.color()))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(MUTED))
                                            .child(SharedString::from(ring.label.clone())),
                                    ),
                            )
                            .child(div().text_xs().text_color(rgb(TEXT)).child(
                                SharedString::from(value_with_unit(
                                    &ring.display_value(),
                                    &ring.unit,
                                )),
                            )),
                    );
            }
            legend
        } else {
            let mut legend = div().flex().flex_col().gap(px(8.0));
            for ring in rings {
                legend =
                    legend.child(
                        div()
                            .flex()
                            .justify_between()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(dot(ring.color()))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(TEXT))
                                            .child(SharedString::from(ring.label.clone())),
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(DIM))
                                    .child(SharedString::from(format!(
                                        "{} · {}",
                                        ring.source, ring.freshness
                                    ))),
                            )
                            .child(div().text_xs().text_color(rgb(TEXT)).child(
                                SharedString::from(value_with_unit(
                                    &ring.display_value(),
                                    &ring.unit,
                                )),
                            )),
                    );
            }
            legend
        };

    div()
        .flex()
        .flex_col()
        .gap(px(if compact { 8.0 } else { 12.0 }))
        .w(px(panel_width))
        .p(px(if compact { 12.0 } else { 20.0 }))
        .rounded_2xl()
        .bg(rgb(PANEL))
        .child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child("Activity rings"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(DIM))
                        .child(SharedString::from(format!("{wearer} · {freshness}"))),
                ),
        )
        .child(
            div().flex().justify_center().child(
                div()
                    .relative()
                    .w(px(canvas_size))
                    .h(px(canvas_size))
                    .child(
                        canvas(
                            move |_, _, _| {},
                            move |bounds, _, window, _| paint_rings(bounds, &paint_specs, window),
                        )
                        .absolute()
                        .top(px(0.0))
                        .left(px(0.0))
                        .w(px(canvas_size))
                        .h(px(canvas_size)),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(0.0))
                            .left(px(0.0))
                            .w(px(canvas_size))
                            .h(px(canvas_size))
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .child(
                                div()
                                    .text_2xl()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(SharedString::from(center_value)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(MUTED))
                                    .child(SharedString::from(center_unit)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(DIM))
                                    .child(SharedString::from(center_label)),
                            ),
                    ),
            ),
        )
        .child(legend)
}

fn card_element(card: &Value, width: f32, compact: bool) -> impl IntoElement {
    let id = card["id"].as_str().unwrap_or("metric");
    let label = card["label"].as_str().unwrap_or("");
    let value = display_value(&card["value"]);
    let unit = card["unit"].as_str().unwrap_or("");
    let display = value_with_unit(&value, unit);
    let source = card["source_device"].as_str().unwrap_or("no source");
    let freshness = card["freshness"].as_str().unwrap_or("unknown");
    let quality = card["quality"].as_f64().unwrap_or(0.0).clamp(0.0, 1.0);
    let accent = card_color(id);
    let fraction = quality as f32;
    let value_style = if compact {
        div().text_2xl()
    } else {
        div().text_3xl()
    };

    div()
        .flex()
        .flex_col()
        .gap(px(if compact { 6.0 } else { 8.0 }))
        .w(px(width))
        .h(px(if compact { 100.0 } else { 150.0 }))
        .p(px(if compact { 12.0 } else { 16.0 }))
        .rounded_xl()
        .bg(rgb(PANEL_RAISED))
        .child(
            div()
                .h(px(3.0))
                .w_full()
                .rounded_full()
                .bg(accent)
                .opacity(0.9),
        )
        .child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(SharedString::from(label.to_string())),
                )
                .child(dot(freshness_color(freshness))),
        )
        .child(
            value_style
                .font_weight(FontWeight::SEMIBOLD)
                .child(SharedString::from(display)),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(DIM))
                .child(SharedString::from(source.to_string())),
        )
        .child(
            div()
                .h(px(3.0))
                .w_full()
                .rounded_full()
                .bg(rgb(TRACK))
                .child(
                    div()
                        .h_full()
                        .w(relative(fraction))
                        .rounded_full()
                        .bg(accent),
                ),
        )
}

fn step_row(row: &Value) -> impl IntoElement {
    let device = row["device"].as_str().unwrap_or("device");
    let count = row["count"].as_u64().unwrap_or(0);
    let fraction = (count as f64 / 10_000.0).clamp(0.0, 1.0) as f32;
    let accent = device_color(device);

    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(dot(accent))
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(TEXT))
                                .child(SharedString::from(device.to_string())),
                        ),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(TEXT))
                        .child(SharedString::from(format_count(count as f64))),
                ),
        )
        .child(
            div()
                .h(px(6.0))
                .w_full()
                .rounded_full()
                .bg(rgb(TRACK))
                .child(
                    div()
                        .h_full()
                        .w(relative(fraction))
                        .rounded_full()
                        .bg(accent),
                ),
        )
}

#[derive(Clone)]
struct Ring {
    id: String,
    label: String,
    value: Option<f64>,
    max: f64,
    unit: String,
    source: String,
    freshness: String,
}

impl Ring {
    fn fraction(&self) -> f32 {
        let Some(value) = self.value else {
            return 0.0;
        };
        if self.max <= 0.0 {
            return 0.0;
        }
        (value / self.max).clamp(0.0, 1.0) as f32
    }

    fn display_value(&self) -> String {
        match self.value {
            Some(value) if self.unit == "steps" => format_count(value),
            Some(value) => format_number(value),
            None => "—".to_string(),
        }
    }

    fn color(&self) -> Rgba {
        ring_color(&self.id)
    }
}

fn parse_rings(view: &Value) -> Vec<Ring> {
    view["rings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|ring| {
            Some(Ring {
                id: ring["id"].as_str()?.to_string(),
                label: ring["label"].as_str()?.to_string(),
                value: ring["value"].as_f64(),
                max: ring["max"].as_f64().unwrap_or(1.0),
                unit: ring["unit"].as_str().unwrap_or("").to_string(),
                source: ring["source_device"]
                    .as_str()
                    .unwrap_or("no source")
                    .to_string(),
                freshness: ring["freshness"].as_str().unwrap_or("unknown").to_string(),
            })
        })
        .collect()
}

fn placeholder_rings() -> Vec<Ring> {
    vec![
        Ring {
            id: "move".to_string(),
            label: "Move".to_string(),
            value: None,
            max: 10_000.0,
            unit: "steps".to_string(),
            source: "no source".to_string(),
            freshness: "unknown".to_string(),
        },
        Ring {
            id: "heart".to_string(),
            label: "Heart".to_string(),
            value: None,
            max: 180.0,
            unit: "bpm".to_string(),
            source: "no source".to_string(),
            freshness: "unknown".to_string(),
        },
        Ring {
            id: "oxygen".to_string(),
            label: "Oxygen".to_string(),
            value: None,
            max: 100.0,
            unit: "%".to_string(),
            source: "no source".to_string(),
            freshness: "unknown".to_string(),
        },
    ]
}

fn paint_rings(bounds: Bounds<Pixels>, rings: &[Ring], window: &mut Window) {
    let center = bounds.center();
    let extent = f32::from(bounds.size.width).min(f32::from(bounds.size.height));
    let width = 12.0;
    let gap = 18.0;
    let outer = extent / 2.0 - width / 2.0 - 2.0;

    for (index, ring) in rings.iter().take(3).enumerate() {
        let radius = outer - index as f32 * gap;
        if radius <= width {
            break;
        }
        let color = ring.color();
        if let Some(track) = arc_path(center, radius, width, 1.0) {
            window.paint_path(track, color.alpha(0.10));
        }
        let fraction = ring.fraction();
        if fraction > 0.0 {
            if let Some(glow) = arc_path(center, radius, width + 9.0, fraction) {
                window.paint_path(glow, color.alpha(0.12));
            }
            if let Some(arc) = arc_path(center, radius, width, fraction) {
                window.paint_path(arc, color);
            }
        }
    }
}

fn arc_path(center: Point<Pixels>, radius: f32, width: f32, fraction: f32) -> Option<Path<Pixels>> {
    let fraction = fraction.clamp(0.0, 1.0);
    if fraction <= 0.0 {
        return None;
    }

    let start = -FRAC_PI_2;
    let sweep = TAU * fraction;
    let steps = ((sweep / TAU) * 128.0).ceil().max(8.0) as usize;
    let mut builder = PathBuilder::stroke(px(width)).with_style(PathStyle::Stroke(
        StrokeOptions::default()
            .with_line_width(width)
            .with_line_cap(LineCap::Round)
            .with_line_join(LineJoin::Round),
    ));
    builder.move_to(point(
        center.x + px(radius * start.cos()),
        center.y + px(radius * start.sin()),
    ));
    for step in 1..=steps {
        let angle = start + sweep * (step as f32 / steps as f32);
        builder.line_to(point(
            center.x + px(radius * angle.cos()),
            center.y + px(radius * angle.sin()),
        ));
    }
    builder.build().ok()
}

fn dot(color: Rgba) -> impl IntoElement {
    div().w(px(7.0)).h(px(7.0)).rounded_full().bg(color)
}

fn ring_color(id: &str) -> Rgba {
    match id {
        "move" => rgb(GREEN),
        "heart" => rgb(ROSE),
        "oxygen" => rgb(CYAN),
        _ => rgb(AMBER),
    }
}

fn card_color(id: &str) -> Rgba {
    match id {
        "heart_rate" => rgb(ROSE),
        "hrv" => rgb(AMBER),
        "spo2" => rgb(CYAN),
        "temperature" => rgb(ORANGE),
        "sleep" => rgb(PURPLE),
        "motion" => rgb(GREEN),
        _ => rgb(AMBER),
    }
}

fn device_color(device: &str) -> Rgba {
    if device.contains("pinetime") {
        rgb(GREEN)
    } else if device.contains("ring") {
        rgb(CYAN)
    } else {
        rgb(AMBER)
    }
}

fn freshness_color(freshness: &str) -> Rgba {
    match freshness {
        "fresh" => rgb(GREEN),
        "stale" => rgb(AMBER),
        "expired" => rgb(ROSE),
        _ => rgb(DIM),
    }
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
                format_number(value.as_f64().unwrap_or_default())
            }
        }
        other => other.to_string(),
    }
}

fn value_with_unit(value: &str, unit: &str) -> String {
    if value == "—" {
        return value.to_string();
    }
    match unit.trim() {
        "" => value.to_string(),
        "%" => format!("{value}%"),
        "C" => format!("{value} °C"),
        unit => format!("{value} {unit}"),
    }
}

fn format_number(value: f64) -> String {
    if (value - value.round()).abs() < 0.05 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

fn format_count(value: f64) -> String {
    let digits = (value.round() as u64).to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(character);
    }
    out
}
