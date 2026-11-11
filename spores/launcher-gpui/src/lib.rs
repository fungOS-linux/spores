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
use soil::present::{cursor, input, surface};
use soil::spore::catalog;
use std::{borrow::Cow, sync::Arc};

/// One geometry, two readers: the grid lays tiles out with these numbers and the
/// pointer hit-tests with them. Keeping them shared is what makes a click land
/// on the tile the user aimed at.
const HEADER_H: f32 = 56.0;
const LIST_PAD: f32 = 24.0;
const TILE: f32 = 104.0;
const TILE_GAP: f32 = 28.0;
const LABEL_H: f32 = 26.0;


/// The colour an app gets when it does not ship one: a hue from its id, at a
/// saturation and lightness that read as a bright tile on a dark launcher.
/// Stable across runs, so an app always looks like itself.
fn tile_color(id: &str, lift: f32) -> u32 {
    let mut hash = 0u32;
    for byte in id.bytes() {
        hash = hash.wrapping_mul(16777619) ^ u32::from(byte);
    }
    let hue = (hash % 360) as f32;
    let saturation = 0.62;
    let lightness = (0.52 + lift).min(0.78);
    hsl_to_rgb(hue, saturation, lightness)
}

fn hsl_to_rgb(hue: f32, saturation: f32, lightness: f32) -> u32 {
    let c = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let m = lightness - c / 2.0;
    let (r, g, b) = match hue as u32 / 60 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let channel = |value: f32| ((value + m) * 255.0).round().clamp(0.0, 255.0) as u32;
    (channel(r) << 16) | (channel(g) << 8) | channel(b)
}

const LILEX: &[u8] = include_bytes!("../assets/Lilex-Regular.ttf");
const LILEX_BOLD: &[u8] = include_bytes!("../assets/Lilex-Bold.ttf");

struct Row {
    id: SharedString,
    /// What the row shows. The host resolves it from the manifest, so the list
    /// and the window title agree on what an app is called.
    title: SharedString,
    version: SharedString,
    summary: SharedString,
    default_surface: SharedString,
}

struct LauncherView {
    rows: Vec<Row>,
    /// Keyboard selection, drawn as a ring.
    selected: usize,
    /// Pointer hover, drawn as a brighter tile. Held apart from `selected` so
    /// pointing at an app always shows something, even when it is also the
    /// selected one.
    hovered: Option<usize>,
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
                        title: SharedString::from(entry.title),
                        version: SharedString::from(entry.version),
                        summary: SharedString::from(entry.summary),
                        default_surface: SharedString::from(entry.default_surface),
                    })
                    .collect();
                Self {
                    rows,
                    selected: 0,
                    hovered: None,
                    status: SharedString::from(format!("{count} installed")),
                }
            }
            Err(error) => Self {
                rows: Vec::new(),
                selected: 0,
                hovered: None,
                status: SharedString::from(format!("catalog: {error:?}")),
            },
        }
    }

    /// How many tiles fit across the window. The grid reflows with the window
    /// instead of running off the edge.
    fn columns(width: f32) -> usize {
        let usable = (width - 2.0 * LIST_PAD + TILE_GAP).max(TILE);
        ((usable / (TILE + TILE_GAP)).floor() as usize).max(1)
    }

    /// The tile under a point, if the pointer is over one.
    fn tile_at(&self, x: f32, y: f32, width: f32) -> Option<usize> {
        let columns = Self::columns(width);
        let top = HEADER_H + LIST_PAD;
        if y < top || x < LIST_PAD {
            return None;
        }
        let stride_x = TILE + TILE_GAP;
        let stride_y = TILE + LABEL_H + TILE_GAP;
        let column = ((x - LIST_PAD) / stride_x).floor();
        let row = ((y - top) / stride_y).floor();
        if column < 0.0 || row < 0.0 {
            return None;
        }
        let column = column as usize;
        if column >= columns {
            return None;
        }
        // A tile's label belongs to the tile: pointing at the name should open
        // the app, not fall between targets.
        if (y - top) % stride_y > TILE + LABEL_H {
            return None;
        }
        let index = row as usize * columns + column;
        (index < self.rows.len()).then_some(index)
    }

    /// Follow the pointer. The hover ring is its own state: with one app
    /// installed, a highlight that only moved the selection would look inert.
    fn hover(&mut self, index: Option<usize>) -> bool {
        if index == self.hovered {
            return false;
        }
        self.hovered = index;
        true
    }

    fn open(&mut self, index: usize) -> bool {
        let Some(row) = self.rows.get(index) else {
            return false;
        };
        self.status = match catalog::launch(&row.id, &row.default_surface) {
            Ok(launched) => SharedString::from(format!("opened {}", launched.target)),
            Err(error) => SharedString::from(format!("launch: {error:?}")),
        };
        true
    }

    fn click(&mut self, index: Option<usize>) -> bool {
        let Some(index) = index else {
            return false;
        };
        self.selected = index;
        self.open(index)
    }

    fn move_by(&mut self, dx: i32, dy: i32, width: f32) -> bool {
        if self.rows.is_empty() {
            return false;
        }
        let columns = Self::columns(width) as i32;
        let count = self.rows.len() as i32;
        let column = self.selected as i32 % columns;
        let row = self.selected as i32 / columns;
        let (column, row) = if dx != 0 {
            let next = column + dx;
            if next < 0 || next >= columns {
                (column, row)
            } else {
                (next, row)
            }
        } else if dy != 0 {
            let next = (row + dy).max(0);
            (column, next)
        } else {
            (column, row)
        };
        let index = row * columns + column;
        let index = index.clamp(0, count - 1) as usize;
        if index == self.selected {
            return false;
        }
        self.selected = index;
        true
    }

    fn header(&self) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .px(px(22.0))
            .h(px(HEADER_H))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(
                        div()
                            .text_size(px(16.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(0xf2f6fb))
                            .child("Spores"),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(0x76828f))
                            .child("installed on this machine"),
                    ),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(0x76828f))
                    .child(self.status.clone()),
            )
    }

    fn tiles(&self, width: f32) -> Vec<AnyElement> {
        if self.rows.is_empty() {
            return vec![
                div()
                    .text_color(rgb(0x76828f))
                    .child("No Spores found. Pass --spores DIR to canvas-winit.")
                    .into_any_element(),
            ];
        }
        // Rows are chunked here rather than wrapped by the layout, so the
        // pointer hit test and the drawn grid share one idea of where a tile is.
        let columns = Self::columns(width).max(1);
        let mut grid = Vec::with_capacity(self.rows.len().div_ceil(columns));
        for start in (0..self.rows.len()).step_by(columns) {
            let mut cells = Vec::with_capacity(columns);
            for index in start..(start + columns).min(self.rows.len()) {
                let row = &self.rows[index];
                let hovered = self.hovered == Some(index);
                let selected = self.selected == index;
                let background = tile_color(&row.id, if hovered { 0.14 } else { 0.0 });
                let glyph: String = row
                    .title
                    .chars()
                    .next()
                    .map(|first| first.to_uppercase().collect())
                    .unwrap_or_else(|| "?".to_owned());
                let mut tile = div()
                    .w(px(TILE))
                    .h(px(TILE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(24.0))
                    .bg(rgb(background))
                    .text_size(px(40.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(0xffff_ffff));
                if selected {
                    tile = tile.border_2().border_color(rgb(0x7d_8b_99));
                }
                if hovered {
                    tile = tile.border_2().border_color(rgb(0xffff_ffff));
                }
                let cell = div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(6.0))
                    .w(px(TILE))
                    .child(tile.child(SharedString::from(glyph)))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(if hovered || selected {
                                rgb(0xf2f6fb)
                            } else {
                                rgb(0xb9c4cf)
                            })
                            .child(row.title.clone()),
                    )
                    .into_any_element();
                cells.push(cell);
            }
            grid.push(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(TILE_GAP))
                    .children(cells)
                    .into_any_element(),
            );
        }
        grid
    }

    fn footer(&self) -> impl IntoElement {
        div()
            .h(px(28.0))
            .flex()
            .items_center()
            .justify_between()
            .px(px(22.0))
            .bg(rgb(0x0f141b))
            .text_size(px(10.0))
            .text_color(rgb(0x76828f))
            .child("←↑↓→ select · enter open · esc quit")
            .child(
                // What the pointer (or the keyboard) is on, so a tile can stay
                // a name and a colour without hiding what the app is.
                self.hovered
                    .or(Some(self.selected))
                    .and_then(|index| self.rows.get(index))
                    .map(|row| {
                        SharedString::from(format!(
                            "{} {} · {}",
                            row.title, row.version, row.summary
                        ))
                    })
                    .unwrap_or_else(|| self.status.clone()),
            )
    }
}

impl Render for LauncherView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let width = surface::size().0 as f32;
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x14171c))
            .font_family("Lilex")
            .child(self.header())
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(TILE_GAP))
                    .p(px(LIST_PAD))
                    .children(self.tiles(width)),
            )
            .child(self.footer())
    }
}

/// Run one view change and mark the frame dirty if it did anything.
fn step(
    window: &gpui::WindowHandle<LauncherView>,
    cx: &mut HeadlessAppContext,
    change: impl FnOnce(&mut LauncherView) -> bool,
) -> anyhow::Result<bool> {
    window.update(cx, |view, _, cx| {
        let changed = change(view);
        if changed {
            cx.notify();
        }
        changed
    })
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

    let scale = surface::scale() as f32;
    let window = cx.open_window_scaled(
        size(px(width as f32), px(height as f32)),
        scale,
        |_, cx| cx.new(|_| LauncherView::new()),
    )?;
    let any: AnyWindowHandle = window.into();

    let mut dirty = true;
    let mut window_size = (width, height);
    loop {
        // Follow the window: a launcher that keeps its first size looks broken
        // the moment someone drags a corner.
        let size_now = surface::size();
        if size_now != window_size {
            window_size = size_now;
            window.update(&mut cx, |_, window, _| {
                window.resize(gpui::size(px(size_now.0 as f32), px(size_now.1 as f32)));
            })?;
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
        match input::poll(16) {
            Some(input::Event::KeyInput(key)) => {
                if key.key == "escape" {
                    break;
                }
                let width = surface::size().0 as f32;
                let changed = match key.key.as_str() {
                    "arrow-right" => step(&window, &mut cx, |view| view.move_by(1, 0, width))?,
                    "arrow-left" => step(&window, &mut cx, |view| view.move_by(-1, 0, width))?,
                    "arrow-down" => step(&window, &mut cx, |view| view.move_by(0, 1, width))?,
                    "arrow-up" => step(&window, &mut cx, |view| view.move_by(0, -1, width))?,
                    "enter" => step(&window, &mut cx, |view| view.open(view.selected))?,
                    _ => false,
                };
                if changed {
                    dirty = true;
                }
            }
            Some(input::Event::Moved(pointer)) => {
                let width = surface::size().0 as f32;
                let changed = window.update(&mut cx, |view, _, cx| {
                    let target = view.tile_at(pointer.x, pointer.y, width);
                    let changed = view.hover(target);
                    if changed {
                        cx.notify();
                    }
                    changed
                })?;
                // A tile is a target, so the pointer says so before it is clicked.
                let target = window
                    .update(&mut cx, |view, _, _| view.tile_at(pointer.x, pointer.y, width))?;
                let _ = cursor::set(match target {
                    Some(_) => cursor::CursorStyle::Pointer,
                    None => cursor::CursorStyle::Default,
                });
                if changed {
                    dirty = true;
                }
            }
            Some(input::Event::Button(button)) => {
                if button.pressed && button.button == input::PointerButton::Left {
                    let width = surface::size().0 as f32;
                    let changed = window.update(&mut cx, |view, _, cx| {
                        let target = view.tile_at(button.x, button.y, width);
                        let changed = view.click(target);
                        cx.notify();
                        changed
                    })?;
                    if changed {
                        dirty = true;
                    }
                }
            }
            Some(input::Event::Scrolled(_)) | None => {}
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
