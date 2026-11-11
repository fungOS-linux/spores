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

use gpui::{AnyWindowHandle, AppContext as _, HeadlessAppContext, PlatformTextSystem, px, size};
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

pub mod view;

use view::{Board, HEADER_H, LIST_W, ROW_H, View, parse_graph, parse_runs};

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

fn fetch_runs(url: &str) -> Result<Vec<view::Run>, String> {
    parse_runs(&fetch_body(url)?)
}

fn fetch_graph(url: &str) -> Result<Vec<view::Workflow>, String> {
    parse_graph(&fetch_body(url)?)
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
                board.apply_runs(fetch_runs(RUNS_URL));
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
            board
                .fetched_at
                .map(|at| at.elapsed().as_secs() >= 5)
                .unwrap_or(true)
        })?;
        if stale {
            let runs = window.update(&mut cx, |board, _, cx| {
                board.apply_runs(fetch_runs(RUNS_URL));
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
                                    board.apply_graph(fetch_graph(GRAPH_URL));
                                }
                                cx.notify();
                            })?;
                            dirty = true;
                        }
                        "r" => {
                            window.update(&mut cx, |board, _, cx| {
                                match board.view {
                                    View::Runs => board.apply_runs(fetch_runs(RUNS_URL)),
                                    View::Brain => board.apply_graph(fetch_graph(GRAPH_URL)),
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
