//! The installed-Spore launcher, rendered by GPUI inside a Spore.
//!
//! The host owns the catalogue and the launch; the Spore owns the list, the
//! selection, and every pixel. Arrow keys move, enter asks the host to open
//! the selected surface, escape closes the launcher.

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
use soil::spore::catalog;
use std::{borrow::Cow, sync::Arc};

const LILEX: &[u8] = include_bytes!("../assets/Lilex-Regular.ttf");
const LILEX_BOLD: &[u8] = include_bytes!("../assets/Lilex-Bold.ttf");

struct Row {
    id: SharedString,
    name: SharedString,
    version: SharedString,
    summary: SharedString,
    default_surface: SharedString,
}

struct LauncherView {
    rows: Vec<Row>,
    selected: usize,
    status: SharedString,
}

impl LauncherView {
    fn new() -> Self {
        match catalog::entries() {
            Ok(entries) => {
                let count = entries.len();
                let rows = entries
                    .into_iter()
                    .map(|entry| Row {
                        id: SharedString::from(entry.id),
                        name: SharedString::from(entry.name),
                        version: SharedString::from(entry.version),
                        summary: SharedString::from(entry.summary),
                        default_surface: SharedString::from(entry.default_surface),
                    })
                    .collect();
                Self {
                    rows,
                    selected: 0,
                    status: SharedString::from(format!("{count} installed")),
                }
            }
            Err(error) => Self {
                rows: Vec::new(),
                selected: 0,
                status: SharedString::from(format!("catalog: {error:?}")),
            },
        }
    }

    fn move_down(&mut self) -> bool {
        if self.selected + 1 < self.rows.len() {
            self.selected += 1;
            true
        } else {
            false
        }
    }

    fn move_up(&mut self) -> bool {
        if self.selected > 0 {
            self.selected -= 1;
            true
        } else {
            false
        }
    }

    fn open_selected(&mut self) -> bool {
        let Some(row) = self.rows.get(self.selected) else {
            return false;
        };
        self.status = match catalog::launch(&row.id, &row.default_surface) {
            Ok(launched) => SharedString::from(format!("opened {}", launched.target)),
            Err(error) => SharedString::from(format!("launch: {error:?}")),
        };
        true
    }

    fn header(&self) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .px(px(16.0))
            .h(px(52.0))
            .bg(rgb(0x0f141b))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(
                        div()
                            .text_size(px(15.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(0xe6edf5))
                            .child("Spores"),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(0x6d7b8c))
                            .child("installed on this machine"),
                    ),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(0x6d7b8c))
                    .child(self.status.clone()),
            )
    }

    fn rows(&self) -> Vec<AnyElement> {
        if self.rows.is_empty() {
            return vec![
                div()
                    .p(px(16.0))
                    .text_color(rgb(0x6d7b8c))
                    .child("No Spores found. Pass --spores DIR to canvas-winit.")
                    .into_any_element(),
            ];
        }
        self.rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let selected = index == self.selected;
                let mut element = div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .px(px(10.0))
                    .py(px(8.0))
                    .rounded(px(6.0));
                if selected {
                    element = element.bg(rgb(0x1b2430));
                }
                element
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(if selected {
                                        FontWeight::BOLD
                                    } else {
                                        FontWeight::NORMAL
                                    })
                                    .text_color(rgb(0xe6edf5))
                                    .child(row.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(rgb(0x6d7b8c))
                                    .child(row.version.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(rgb(0x4f6b82))
                                    .child(row.default_surface.clone()),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(0x8a99aa))
                            .child(row.summary.clone()),
                    )
                    .into_any_element()
            })
            .collect()
    }

    fn footer(&self) -> impl IntoElement {
        div()
            .h(px(28.0))
            .flex()
            .items_center()
            .justify_between()
            .px(px(16.0))
            .bg(rgb(0x0f141b))
            .text_size(px(10.0))
            .text_color(rgb(0x6d7b8c))
            .child("↑/↓ select · enter open · esc quit")
            .child(self.status.clone())
    }
}

impl Render for LauncherView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x0c1015))
            .font_family("Lilex")
            .child(self.header())
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .p(px(8.0))
                    .children(self.rows()),
            )
            .child(self.footer())
    }
}

fn present_frame(
    context: &WgpuContext,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> anyhow::Result<()> {
    let texture = context.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("launcher frame"),
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

fn run_launcher() -> anyhow::Result<()> {
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
        cx.new(|_| LauncherView::new())
    })?;
    let any: AnyWindowHandle = window.into();

    let mut dirty = true;
    loop {
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
        match input::poll(16) {
            Some(key) => {
                if key.key == "escape" {
                    break;
                }
                let changed = match key.key.as_str() {
                    "arrow-down" => window.update(&mut cx, |view, _, cx| {
                        let changed = view.move_down();
                        if changed {
                            cx.notify();
                        }
                        changed
                    })?,
                    "arrow-up" => window.update(&mut cx, |view, _, cx| {
                        let changed = view.move_up();
                        if changed {
                            cx.notify();
                        }
                        changed
                    })?,
                    "enter" => window.update(&mut cx, |view, _, cx| {
                        let changed = view.open_selected();
                        cx.notify();
                        changed
                    })?,
                    _ => false,
                };
                if changed {
                    dirty = true;
                }
            }
            None => {}
        }
    }
    Ok(())
}

struct Component;

impl Guest for Component {
    async fn run() -> u32 {
        match run_launcher() {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("launcher: {error:#}");
                1
            }
        }
    }
}

export!(Component);
