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
use soil::present::{cursor, input, surface};
use soil::terminal_screen::screen;
use std::{borrow::Cow, sync::Arc};
use terminal_core::{Cell, Grid, Style};

const LILEX: &[u8] = include_bytes!("../assets/Lilex-Regular.ttf");
const LILEX_BOLD: &[u8] = include_bytes!("../assets/Lilex-Bold.ttf");
/// The grid's face. Fira Code is monospaced with a 0.6 em advance, which is
/// what `CELL_W` measures at 13 px; the UI around it stays in Lilex.
const FIRA: &[u8] = include_bytes!("../assets/FiraCode-Regular.otf");
const FIRA_BOLD: &[u8] = include_bytes!("../assets/FiraCode-Bold.ttf");

const CELL_W: f32 = 7.8;
const CELL_H: f32 = 16.0;

/// The chrome around the grid. The renderer lays out with these and the pointer
/// hit-tests with them, so a click or a wheel tick lands on the cell the user
/// aimed at.
const SIDEBAR_W: f32 = 214.0;
const HEADER_H: f32 = 46.0;
const GRID_PAD: f32 = 10.0;
/// A cell's advance is not its ink: the last column needs room on its right or
/// the glyph is clipped by the pane.
const GRID_PAD_R: f32 = 18.0;
const FOOTER_H: f32 = 26.0;
const TOGGLE_W: f32 = 28.0;

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
    /// The sidebar can be folded away; the grid takes the width back.
    sidebar_open: bool,
    /// The last failure from the host, shown instead of a silent blank grid: a
    /// terminal that cannot open a shell has to say so.
    error: Option<SharedString>,
    targets: Vec<Target>,
    generation: u64,
}

fn grid_from_snapshot(snapshot: &screen::Grid) -> Grid {
    let cells = snapshot
        .cells
        .iter()
        .enumerate()
        .map(|(index, cell)| {
            let wide = !cell.wide_tail
                && snapshot
                    .cells
                    .get(index + 1)
                    .is_some_and(|next| next.wide_tail);
            Cell {
                text: cell.text.clone(),
                width: if wide { 2 } else { 1 },
                style: Style {
                    fg: cell.fg.unwrap_or(0),
                    bg: cell.bg.unwrap_or(0),
                    bold: cell.bold,
                    italic: cell.italic,
                    underline: cell.underline,
                    reverse: cell.inverse,
                    dim: cell.faint,
                    strikethrough: false,
                },
            }
        })
        .collect();
    let cursor = snapshot.cursor_row.zip(snapshot.cursor_col);
    Grid {
        cols: snapshot.cols,
        rows: snapshot.rows,
        cells,
        cursor_row: cursor.map_or(0, |cursor| cursor.0),
        cursor_col: cursor.map_or(0, |cursor| cursor.1),
        cursor_visible: cursor.is_some(),
        title: Some(snapshot.title.clone()).filter(|title| !title.is_empty()),
    }
}

impl TerminalView {
    fn new() -> Self {
        let (grid, generation) = match screen::snapshot() {
            Ok(snapshot) => (grid_from_snapshot(&snapshot), snapshot.generation),
            Err(_) => (
                Grid {
                    cols: 80,
                    rows: 24,
                    cells: vec![Cell::blank(); 80 * 24],
                    cursor_row: 0,
                    cursor_col: 0,
                    cursor_visible: false,
                    title: None,
                },
                0,
            ),
        };
        Self {
            grid,
            sidebar_open: true,
            error: None,
            generation,
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
                    .font_family("Fira Code")
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

    /// How many cells fit in the space the host gives us. The grid follows the
    /// window and the sidebar, so folding the sidebar widens the shell.
    fn cells_for(width: f32, height: f32, sidebar_open: bool) -> (u32, u32) {
        let sidebar = if sidebar_open { SIDEBAR_W } else { 0.0 };
        let cols = ((width - sidebar - GRID_PAD - GRID_PAD_R) / CELL_W)
            .floor()
            .max(20.0) as u32;
        let rows = ((height - HEADER_H - FOOTER_H - 2.0 * GRID_PAD) / CELL_H)
            .floor()
            .max(5.0) as u32;
        (cols, rows)
    }

    fn sidebar_width(&self) -> f32 {
        if self.sidebar_open { SIDEBAR_W } else { 0.0 }
    }

    /// The fold control sits at the head of the terminal pane.
    fn toggle_at(&self, x: f32, y: f32) -> bool {
        let left = self.sidebar_width();
        (left..left + TOGGLE_W).contains(&x) && (0.0..HEADER_H).contains(&y)
    }

    /// Which cell the pointer is over, if it is over the grid at all.
    fn cell_at(&self, x: f32, y: f32) -> Option<(u32, u32)> {
        let left = self.sidebar_width() + GRID_PAD;
        let top = HEADER_H + GRID_PAD;
        if x < left || y < top {
            return None;
        }
        let col = ((x - left) / CELL_W) as u32;
        let row = ((y - top) / CELL_H) as u32;
        (col < self.grid.cols && row < self.grid.rows).then_some((row, col))
    }

    /// The bar under the grid: what this session is, how big the grid is, and
    /// the keys that matter. Everything here is something the Spore knows; a
    /// field it cannot fill honestly (a working directory, a running process)
    /// waits for a contract that reports it rather than guessing.
    fn status_bar(&self) -> impl IntoElement {
        let title = self
            .grid
            .title
            .clone()
            .unwrap_or_else(|| "local shell".to_string());
        let (dot, title_color) = match &self.error {
            Some(_) => (0xd97757, 0xf0b8a4),
            None => (0x7ee08a, 0xc8d3e0),
        };
        let left = match &self.error {
            Some(error) => format!("terminal: {error}"),
            None => title,
        };
        div()
            .h(px(FOOTER_H))
            .flex()
            .items_center()
            .justify_between()
            .px(px(14.0))
            .bg(rgb(0x0f141b))
            .text_size(px(11.0))
            .text_color(rgb(0x6d7b8c))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(div().w(px(6.0)).h(px(6.0)).rounded(px(3.0)).bg(rgb(dot)))
                    .child(
                        div()
                            .text_color(rgb(title_color))
                            .child(SharedString::from(left)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(14.0))
                    .child(
                        div()
                            .text_color(rgb(0x8a99aa))
                            .child(SharedString::from(format!(
                                "{}×{}",
                                self.grid.cols, self.grid.rows
                            ))),
                    )
                    .child("⌃B sidebar")
                    .child("⌃Q quit"),
            )
    }

    fn sidebar(&self) -> impl IntoElement {
        div()
            .w(px(SIDEBAR_W))
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
            .h(px(HEADER_H))
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
                            .w(px(TOGGLE_W))
                            .text_size(px(13.0))
                            .text_color(rgb(0x8a99aa))
                            .child(if self.sidebar_open { "«" } else { "»" }),
                    )
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
            .children(self.sidebar_open.then(|| self.sidebar()))
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
                            .pl(px(GRID_PAD))
                            .pr(px(GRID_PAD_R))
                            .py(px(GRID_PAD))
                            .flex()
                            .flex_col()
                            .children(self.rows()),
                    )
                    .child(self.status_bar()),
            )
    }
}

/// Fold or unfold the sidebar and re-lay the shell at the width that leaves.
fn toggle_sidebar(
    window: &gpui::WindowHandle<TerminalView>,
    cx: &mut HeadlessAppContext,
    _unused: Option<()>,
) -> anyhow::Result<()> {
    window.update(cx, |view, _, cx| {
        view.sidebar_open = !view.sidebar_open;
        cx.notify();
    })?;
    Ok(())
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

fn key_bytes(key: &input::Key) -> Vec<u8> {
    if key.control
        && let Some(text) = &key.text
        && text.len() == 1
    {
        let byte = text.as_bytes()[0];
        if byte.is_ascii_alphabetic() {
            return vec![byte & 0x1f];
        }
    }
    if let Some(text) = &key.text {
        return text.as_bytes().to_vec();
    }
    match key.key.as_str() {
        "enter" => b"\r".to_vec(),
        "tab" => b"\t".to_vec(),
        "backspace" => b"\x7f".to_vec(),
        "escape" => b"\x1b".to_vec(),
        "space" => b" ".to_vec(),
        "arrow-up" => b"\x1b[A".to_vec(),
        "arrow-down" => b"\x1b[B".to_vec(),
        "arrow-right" => b"\x1b[C".to_vec(),
        "arrow-left" => b"\x1b[D".to_vec(),
        "home" => b"\x1b[H".to_vec(),
        "end" => b"\x1b[F".to_vec(),
        "delete" => b"\x1b[3~".to_vec(),
        _ => Vec::new(),
    }
}

fn render_frame() -> anyhow::Result<()> {
    let (width, height) = surface::size();
    let instance = wgpu_wasi_webgpu::instance();
    let adapter = wit_bindgen::block_on(instance.request_adapter(&Default::default()))?;
    let (device, queue) = wit_bindgen::block_on(adapter.request_device(&Default::default()))?;
    let (context, format) = WgpuContext::from_offscreen(instance, adapter, device, queue)?;

    let text = Arc::new(CosmicTextSystem::new_without_system_fonts("Lilex"));
    text.add_fonts(vec![
        Cow::Borrowed(LILEX),
        Cow::Borrowed(LILEX_BOLD),
        Cow::Borrowed(FIRA),
        Cow::Borrowed(FIRA_BOLD),
    ])?;

    let factory_context = context.clone();
    let mut cx = HeadlessAppContext::with_platform(text, Arc::new(()), move || {
        Ok(Some(Box::new(WgpuHeadlessRenderer::with_context(
            factory_context.clone(),
            format,
        )?)))
    });

    // The host knows the display; asking for its scale means the frame we render
    // is the resolution the window blits, not a guess that gets resampled.
    let scale = surface::scale() as f32;
    let window =
        cx.open_window_scaled(size(px(width as f32), px(height as f32)), scale, |_, cx| {
            cx.new(|_| TerminalView::new())
        })?;
    let any: AnyWindowHandle = window.into();

    let mut generation = 0u64;
    let mut dirty = true;
    let mut window_size = (width, height);
    let mut cells = (0, 0);
    loop {
        // The window is the source of truth for how much space there is. Both
        // the emulator's grid and the GPUI window follow it, so a drag reaches
        // the shell instead of leaving a half-drawn screen behind.
        let size = surface::size();
        if size != window_size {
            window_size = size;
            window.update(&mut cx, |_, window, _| {
                window.resize(gpui::size(px(size.0 as f32), px(size.1 as f32)));
            })?;
            dirty = true;
        }
        let sidebar_open = window.update(&mut cx, |view, _, _| view.sidebar_open)?;
        let wanted = TerminalView::cells_for(size.0 as f32, size.1 as f32, sidebar_open);
        if wanted != cells {
            cells = wanted;
            let _ = screen::resize(wanted.0, wanted.1);
        }

        match screen::snapshot() {
            Ok(snapshot) if snapshot.generation != generation => {
                generation = snapshot.generation;
                let grid = grid_from_snapshot(&snapshot);
                window.update(&mut cx, |view, _, cx| {
                    view.grid = grid;
                    view.generation = generation;
                    view.error = None;
                    cx.notify();
                })?;
                dirty = true;
            }
            Ok(_) => {}
            Err(error) => {
                window.update(&mut cx, |view, _, cx| {
                    if view.error.as_deref() != Some(error.as_str()) {
                        view.error = Some(SharedString::from(error));
                        cx.notify();
                    }
                })?;
                dirty = true;
            }
        }

        if dirty {
            // `capture_screenshot` renders the last drawn frame; a notify only
            // marks the view dirty. Draw explicitly so a key update is visible.
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
        // Everything already queued goes to the pty on this turn: a key should
        // not wait for a poll timeout to reach the shell.
        let mut quit = false;
        loop {
            let Some(event) = input::poll(0) else {
                break;
            };
            match event {
                input::Event::KeyInput(key) => {
                    if key.control && key.key.eq_ignore_ascii_case("q") {
                        quit = true;
                        break;
                    }
                    if key.control && key.key.eq_ignore_ascii_case("b") {
                        toggle_sidebar(&window, &mut cx, None)?;
                        dirty = true;
                        continue;
                    }
                    let bytes = key_bytes(&key);
                    if !bytes.is_empty() {
                        let _ = screen::write(&bytes);
                    }
                    dirty = true;
                }
                input::Event::Moved(pointer) => {
                    // The grid is text: say so with the pointer itself, the way a
                    // native terminal does, instead of drawing our own caret.
                    let over_toggle = window
                        .update(&mut cx, |view, _, _| view.toggle_at(pointer.x, pointer.y))?;
                    if over_toggle {
                        let _ = cursor::set(cursor::CursorStyle::Pointer);
                        continue;
                    }
                    let over_grid = window
                        .update(&mut cx, |view, _, _| view.cell_at(pointer.x, pointer.y))?
                        .is_some();
                    let style = if over_grid {
                        cursor::CursorStyle::Text
                    } else {
                        cursor::CursorStyle::Default
                    };
                    let _ = cursor::set(style);
                }
                input::Event::Scrolled(scroll) => {
                    let over_grid = window
                        .update(&mut cx, |view, _, _| view.cell_at(scroll.x, scroll.y))?
                        .is_some();
                    if over_grid && scroll.lines != 0.0 {
                        // Three rows a notch feels like a wheel, not a nudge. The
                        // grid the host hands back already shows the moved viewport.
                        let _ = screen::scroll(-(scroll.lines * 3.0) as i32);
                    }
                }
                input::Event::Button(button) => {
                    if button.pressed && button.button == input::PointerButton::Left {
                        let target = window
                            .update(&mut cx, |view, _, _| view.toggle_at(button.x, button.y))?;
                        if target {
                            toggle_sidebar(&window, &mut cx, None)?;
                            dirty = true;
                        }
                    }
                }
            }
        }
        if quit {
            break;
        }
        // Then sleep until the emulator publishes or a tick passes, instead of
        // polling the grid: the echo of a keystroke is drawn the moment it
        // exists, and an idle terminal costs one wake-up every 8 ms at most.
        let _ = screen::wait(generation, 8);
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
