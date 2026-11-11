//! term.myc as a GPUI Spore.
//!
//! The Spore owns the terminal view and its VT grid; the host owns only the
//! WebGPU device and the compositor surface. Nothing here sees a window, a
//! swapchain, or a native display handle.

wit_bindgen::generate!({
    path: "wit",
    world: "runner",
    with: {
        "wasi:webgpu/webgpu@0.3.0-rc.2": wgpu_wasi_webgpu::wasi::webgpu::webgpu,
    },
    generate_all,
});

use gpui::{
    AnyElement, AnyWindowHandle, AppContext as _, Context, FontWeight, HeadlessAppContext,
    IntoElement, PlatformTextSystem, Render, SharedString, Window, div, prelude::*, px, rgb, size,
};
use gpui_wgpu::{CosmicTextSystem, WgpuContext, WgpuHeadlessRenderer};
use soil::present::{input, surface};
use std::{borrow::Cow, sync::Arc};
use terminal_core::{ANSI_FIXTURE, Grid, render};

const LILEX: &[u8] = include_bytes!("../assets/Lilex-Regular.ttf");
const LILEX_BOLD: &[u8] = include_bytes!("../assets/Lilex-Bold.ttf");

const CELL_W: f32 = 7.8;
const CELL_H: f32 = 16.0;
const DEFAULT_FG: u32 = 0xc8d3e0;

/// xterm's first sixteen colours; indices above that are the 6x6x6 cube.
const ANSI: [u32; 16] = [
    0x1b1f27, 0xd14b4b, 0x4f9e5a, 0xc9a227, 0x4b7fd1, 0x9a6ac9, 0x3fa7a0, 0xc8d3e0, 0x5c6773,
    0xff6b6b, 0x7ee08a, 0xffd166, 0x7aa2ff, 0xc792ea, 0x64d8d0, 0xffffff,
];

fn ansi_color(value: u32, default: u32) -> u32 {
    if value == 0 {
        default
    } else if value < 16 {
        ANSI[value as usize]
    } else if value < 256 {
        let index = value - 16;
        let level = |n: u32| if n == 0 { 0 } else { 55 + 40 * n };
        level(index / 36) << 16 | level((index % 36) / 6) << 8 | level(index % 6)
    } else {
        value & 0x00ff_ffff
    }
}

struct Target {
    name: &'static str,
    kind: &'static str,
    up: bool,
    selected: bool,
}

struct TerminalView {
    grid: Grid,
    targets: Vec<Target>,
    last_key: SharedString,
}

impl TerminalView {
    fn new() -> Self {
        Self {
            // The same ANSI fixture the native mock and the cell-grid spore use.
            grid: render(ANSI_FIXTURE.as_bytes(), 80, 24),
            last_key: SharedString::from("none"),
            targets: vec![
                Target {
                    name: "studio",
                    kind: "macOS · this machine",
                    up: true,
                    selected: true,
                },
                Target {
                    name: "agora-one",
                    kind: "Linux · RTX 4090",
                    up: true,
                    selected: false,
                },
                Target {
                    name: "home-pi",
                    kind: "Linux · aarch64",
                    up: false,
                    selected: false,
                },
            ],
        }
    }

    fn rows(&self) -> Vec<AnyElement> {
        let mut rows = Vec::with_capacity(self.grid.rows as usize);
        for row in 0..self.grid.rows {
            let mut cells = Vec::new();
            let mut col = 0;
            while col < self.grid.cols {
                let cell = self
                    .grid
                    .cell(row, col)
                    .expect("grid is row-major and complete");
                let width = cell.width.max(1);
                let mut fg = ansi_color(cell.style.fg, DEFAULT_FG);
                let mut bg = ansi_color(cell.style.bg, 0);
                if cell.style.reverse {
                    std::mem::swap(&mut fg, &mut bg);
                    if bg == 0 {
                        bg = 0x2a3442;
                    }
                }
                let cursor = self.grid.cursor_visible
                    && row == self.grid.cursor_row
                    && col == self.grid.cursor_col;

                let mut element = div()
                    .flex_none()
                    .w(px(CELL_W * f32::from(width)))
                    .h(px(CELL_H))
                    .font_family("Lilex")
                    .text_size(px(13.0))
                    .line_height(px(CELL_H))
                    .text_color(rgb(fg));
                if cell.style.bold {
                    element = element.font_weight(FontWeight::BOLD);
                }
                if bg != 0 {
                    element = element.bg(rgb(bg));
                }
                if cursor {
                    element = element.bg(rgb(0x3a5f8a)).text_color(rgb(0xffffff));
                }
                let text = if cell.text.is_empty() {
                    SharedString::from(" ")
                } else {
                    SharedString::from(cell.text.clone())
                };
                cells.push(element.child(text).into_any_element());
                col += u32::from(width);
            }
            rows.push(
                div()
                    .flex()
                    .flex_row()
                    .h(px(CELL_H))
                    .children(cells)
                    .into_any_element(),
            );
        }
        rows
    }

    fn sidebar(&self) -> impl IntoElement {
        div()
            .w(px(214.0))
            .h_full()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .p(px(14.0))
            .bg(rgb(0x11161d))
            .child(
                div()
                    .mb(px(10.0))
                    .text_size(px(10.0))
                    .text_color(rgb(0x6d7b8c))
                    .child("MYCELIUM"),
            )
            .children(self.targets.iter().map(|target| {
                let mut row = div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(8.0))
                    .py(px(6.0))
                    .rounded(px(6.0));
                if target.selected {
                    row = row.bg(rgb(0x1b2430));
                }
                row.child(
                    div()
                        .w(px(6.0))
                        .h(px(6.0))
                        .rounded(px(3.0))
                        .bg(rgb(if target.up { 0x4f9e5a } else { 0xc9a227 })),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgb(0xdbe4ee))
                                .child(target.name),
                        )
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(rgb(0x6d7b8c))
                                .child(target.kind),
                        ),
                )
            }))
    }

    fn header(&self) -> impl IntoElement {
        let title = self
            .grid
            .title
            .clone()
            .unwrap_or_else(|| "terminal".to_string());
        div()
            .h(px(46.0))
            .flex()
            .items_center()
            .justify_between()
            .px(px(14.0))
            .bg(rgb(0x0f141b))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgb(0xe6edf5))
                            .child("term.myc"),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(0x6d7b8c))
                            .child(SharedString::from(title)),
                    ),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(0x6d7b8c))
                    .child(SharedString::from(format!(
                        "{}x{}",
                        self.grid.cols, self.grid.rows
                    ))),
            )
    }
}

impl Render for TerminalView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_row()
            .bg(rgb(0x0b0f14))
            .font_family("Lilex")
            .child(self.sidebar())
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .child(self.header())
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .p(px(10.0))
                            .flex()
                            .flex_col()
                            .children(self.rows()),
                    )
                    .child(
                        div()
                            .h(px(26.0))
                            .flex()
                            .items_center()
                            .px(px(14.0))
                            .bg(rgb(0x0f141b))
                            .text_size(px(10.0))
                            .text_color(rgb(0x6d7b8c))
                            .child(SharedString::from(format!(
                                "connected · mycelium ssh studio · last key: {}",
                                self.last_key
                            ))),
                    ),
            )
    }
}

fn present_frame(
    context: &WgpuContext,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> anyhow::Result<()> {
    let texture = context.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("term.myc frame"),
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

    let window = cx.open_window(size(px(width as f32), px(height as f32)), |_, cx| {
        cx.new(|_| TerminalView::new())
    })?;
    let any: AnyWindowHandle = window.into();

    loop {
        // `capture_screenshot` renders the last drawn frame; a notify only
        // marks the view dirty. Draw explicitly so a key update is visible.
        cx.update_window(any, |_, window, app| {
            let _ = window.draw(app);
        })?;
        let image = cx.capture_screenshot(any)?;
        present_frame(&context, image.width(), image.height(), image.as_raw())?;

        if !input::interactive() {
            break;
        }
        match input::poll(16) {
            Some(key) => {
                if key.key == "escape" {
                    break;
                }
                let label = key.text.clone().unwrap_or_else(|| key.key.clone());
                window.update(&mut cx, |view, _, cx| {
                    view.last_key = SharedString::from(label);
                    cx.notify();
                })?;
            }
            None => {}
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
                eprintln!("term.myc: {error:#}");
                1
            }
        }
    }
}

export!(Component);
