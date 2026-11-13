//! xplorer — a file manager, rendered by GPUI inside a Spore.
//!
//! The window is three things: the path you are in, the entries the host reports
//! for it, and a preview of whatever is selected. Nothing here walks a disk. A
//! path comes from `soil:vfs`, the bytes behind it come from `soil:vfs` at a
//! bounded size, and a path the host does not grant simply does not exist — which
//! is why this Spore cannot show you somebody else's files even by accident.
//!
//! Keyboard first, on purpose: arrows move, `Enter` descends, `Backspace` goes up.
//! A pointer lands later; the hит-testing is the part that needs care and the
//! arrangement does not.

#[cfg(not(feature = "demo"))]
wit_bindgen::generate!({
    path: "wit",
    world: "runner",
    with: {
        "wasi:webgpu/webgpu@0.3.0-rc.2": wgpu_wasi_webgpu::wasi::webgpu::webgpu,
    },
    generate_all,
});

// The same window without the namespace.
//
// A host that has not linked `soil:vfs` cannot instantiate a component that
// imports it, so this build exists to render the arrangement against a fixed
// listing. It is a harness for looking at the UI, never the shipped Spore.
#[cfg(feature = "demo")]
wit_bindgen::generate!({
    path: "wit-demo",
    world: "runner",
    with: {
        "wasi:webgpu/webgpu@0.3.0-rc.2": wgpu_wasi_webgpu::wasi::webgpu::webgpu,
    },
    generate_all,
});

use std::{borrow::Cow, sync::Arc};

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AppContext as _, Context, HeadlessAppContext, IntoElement, ParentElement as _,
    PlatformTextSystem as _, Render, Styled as _, Window, px, size,
};
use gpui_component::{ActiveTheme as _, h_flex, v_flex};
use gpui_wgpu::{CosmicTextSystem, WgpuContext, WgpuHeadlessRenderer};
use soil::present::{cursor, input, surface};

mod data;

const LILEX: &[u8] = include_bytes!("../assets/Lilex-Regular.ttf");
const LILEX_BOLD: &[u8] = include_bytes!("../assets/Lilex-Bold.ttf");

/// How much of a file preview reads. A preview that streams a gigabyte is not a
/// preview; the status line says how much was shown rather than pretending the
/// file ended.
const PREVIEW_BYTES: u32 = 24 * 1024;

/// How many entries one page asks for.
#[cfg_attr(feature = "demo", allow(dead_code))]
const PAGE: u32 = 500;

/// One row in the list.
struct Row {
    name: String,
    path: String,
    directory: bool,
    size: u64,
    hydrated: bool,
}

/// What the pane on the right is showing.
struct Preview {
    title: String,
    body: String,
    footer: String,
    error: bool,
}

impl Preview {
    fn empty(why: &str) -> Self {
        Self {
            title: "nothing selected".to_owned(),
            body: why.to_owned(),
            footer: String::new(),
            error: false,
        }
    }
}

/// One window of the file manager.
struct Finder {
    /// Workspace-relative path of the directory being listed; empty is the root.
    path: String,
    entries: Vec<Row>,
    selected: usize,
    preview: Preview,
    note: Option<String>,
    /// How many tiles fit in one row, as of the last render. Arrow keys use it to
    /// move by a row; the arrangement is the only thing that knows the width.
    columns: usize,
}

impl Finder {
    fn open() -> Self {
        let mut finder = Self {
            path: String::new(),
            entries: Vec::new(),
            selected: 0,
            preview: Preview::empty("select a file to look inside it"),
            note: None,
            columns: 1,
        };
        finder.reload();
        // The harness build opens on a file, so a captured frame shows the
        // preview pane doing its job as well as the list.
        #[cfg(feature = "demo")]
        if let Some(index) = finder.entries.iter().position(|row| !row.directory) {
            finder.select(index);
        }
        finder
    }

    /// Ask the host for this directory again.
    fn reload(&mut self) {
        match data::ls(&self.path) {
            Ok(rows) => {
                self.entries = rows;
                self.entries.sort_by(|a, b| {
                    b.directory
                        .cmp(&a.directory)
                        .then_with(|| a.name.cmp(&b.name))
                });
                self.selected = 0;
                self.note = None;
                self.select(0);
            }
            Err(error) => {
                self.entries.clear();
                self.selected = 0;
                self.preview = Preview::empty("the host did not list this directory");
                self.note = Some(format!("{}: {error}", describe(&self.path)));
            }
        }
    }

    /// Move the selection and load its preview.
    fn select(&mut self, index: usize) {
        if self.entries.is_empty() {
            self.preview = Preview::empty("this directory is empty");
            return;
        }
        self.selected = index.min(self.entries.len() - 1);
        let row = &self.entries[self.selected];
        if row.directory {
            self.preview = Preview {
                title: row.name.clone(),
                body: "a directory — Enter opens it".to_owned(),
                footer: String::new(),
                error: false,
            };
            return;
        }
        self.preview = self.preview_of(row);
    }

    /// Read the selected file through the same capability the list came from.
    fn preview_of(&self, row: &Row) -> Preview {
        let footer = if row.hydrated {
            String::new()
        } else {
            "not on this machine yet: the namespace knows it, the bytes are elsewhere".to_owned()
        };
        match data::read(&row.path, PREVIEW_BYTES) {
            Ok((bytes, content_id, eof)) => {
                let shown = bytes.len();
                let text = String::from_utf8_lossy(&bytes);
                if text.contains('\u{0}') {
                    return Preview {
                        title: row.name.clone(),
                        body: format!(
                            "binary · {} bytes · {} shown\n{}",
                            row.size, shown, content_id
                        ),
                        footer: if eof { footer } else { String::new() },
                        error: false,
                    };
                }
                let mut body: String = text.lines().take(400).collect::<Vec<_>>().join("\n");
                if !eof {
                    body.push_str("\n\n… more than this preview shows");
                }
                Preview {
                    title: row.name.clone(),
                    body,
                    footer,
                    error: false,
                }
            }
            Err(error) => Preview {
                title: row.name.clone(),
                body: format!("could not read this file\n{error}"),
                footer,
                error: true,
            },
        }
    }

    fn move_selection(&mut self, delta: isize) {
        if self.entries.is_empty() {
            return;
        }
        let next = (self.selected as isize + delta).clamp(0, self.entries.len() as isize - 1);
        self.select(next as usize);
    }

    /// Descend into the selected directory.
    fn enter(&mut self) {
        let Some(row) = self.entries.get(self.selected) else {
            return;
        };
        if !row.directory {
            self.note = Some(format!("{} is a file; preview is on the right", row.name));
            return;
        }
        self.path = row.path.clone();
        self.reload();
    }

    /// Go up one directory, keeping the directory we came from selected.
    fn up(&mut self) {
        if self.path.is_empty() {
            return;
        }
        let leaving = self.path.clone();
        self.path = leaving
            .rsplit_once('/')
            .map_or_else(String::new, |(head, _)| head.to_owned());
        self.reload();
        if let Some(index) = self.entries.iter().position(|row| row.path == leaving) {
            self.select(index);
        }
    }

    fn handle_key(&mut self, key: &str) -> bool {
        match key {
            "arrow-right" | "l" => {
                self.move_selection(1);
                true
            }
            "arrow-left" | "h" => {
                self.move_selection(-1);
                true
            }
            "arrow-down" | "j" => {
                self.move_selection(self.columns.max(1) as isize);
                true
            }
            "arrow-up" | "k" => {
                self.move_selection(-(self.columns.max(1) as isize));
                true
            }
            "enter" => {
                self.enter();
                true
            }
            "backspace" => {
                self.up();
                true
            }
            "r" => {
                self.reload();
                true
            }
            _ => false,
        }
    }
}

/// An icon drawn from rectangles, because a Spore has no asset source and this
/// fork has no path transforms. Kind is read from the name: it is a hint, not a
/// claim about the bytes, and the preview is where the truth is.
///
/// Ink is the theme's foreground and the detail is punched in the theme's
/// background, so the same glyph reads in either mode without a second palette.
fn tile_icon(row: &Row, theme: &gpui_component::Theme) -> gpui::AnyElement {
    let name = row.name.to_ascii_lowercase();
    let folder = theme.primary;
    let paper = theme.foreground;
    let ink = theme.background;

    if row.directory {
        return v_flex()
            .items_start()
            .child(gpui::div().w(px(18.0)).h(px(6.0)).rounded_t_sm().bg(folder))
            .child(gpui::div().w(px(44.0)).h(px(30.0)).rounded_sm().bg(folder))
            .into_any_element();
    }

    // A page with a folded corner, and what is written on it: bars for text,
    // a chevron pair for code, a bar chart for a deck or a sheet.
    let (width, lines) = if name.ends_with(".md") || name.ends_with(".txt") {
        (34.0, 3)
    } else if name.ends_with(".lua") || name.ends_with(".rs") || name.ends_with(".toml") {
        (34.0, 2)
    } else if name.ends_with(".pptx") || name.ends_with(".docx") || name.ends_with(".xlsx") {
        (34.0, 4)
    } else {
        (34.0, 3)
    };

    let mut body = v_flex()
        .w(px(width + 8.0))
        .h(px(width + 12.0))
        .items_center()
        .justify_center()
        .gap_0p5()
        .rounded_sm()
        .bg(paper);

    for index in 0..lines {
        body = body.child(
            gpui::div()
                .w(px(if index == lines - 1 { 12.0 } else { 20.0 }))
                .h(px(2.0))
                .bg(ink),
        );
    }
    body.into_any_element()
}

fn describe(path: &str) -> String {
    if path.is_empty() {
        "/".to_owned()
    } else {
        path.to_owned()
    }
}

fn human_size(bytes: u64) -> String {
    match bytes {
        0..=1023 => format!("{bytes} B"),
        1024..=1_048_575 => format!("{:.1} kB", bytes as f64 / 1024.0),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.0),
    }
}

impl Render for Finder {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (width, height) = surface::size();
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let border = theme.border;

        // What the preview is looking at, said in the header rather than guessed at:
        // a size, and whether the bytes are actually on this machine.
        let subtitle = self.entries.get(self.selected).map(|row| {
            if row.directory {
                "directory".to_owned()
            } else {
                format!(
                    "{} · {}",
                    human_size(row.size),
                    if row.hydrated {
                        "on this machine"
                    } else {
                        "not on this machine"
                    }
                )
            }
        });

        // The path as breadcrumb segments, root first, the last one where we are.
        // One chip per segment beats one string: the eye lands on the leaf, and a
        // deep path does not read as a sentence.
        let segments: Vec<&str> = self
            .path
            .split('/')
            .filter(|part| !part.is_empty())
            .collect();
        let mut crumbs: Vec<gpui::AnyElement> = Vec::new();
        if segments.is_empty() {
            crumbs.push(
                gpui::div()
                    .text_sm()
                    .text_color(theme.foreground)
                    .child("/")
                    .into_any_element(),
            );
        } else {
            crumbs.push(
                gpui::div()
                    .text_sm()
                    .text_color(muted)
                    .child("/")
                    .into_any_element(),
            );
            for (index, segment) in segments.iter().enumerate() {
                if index > 0 {
                    crumbs.push(
                        gpui::div()
                            .text_sm()
                            .text_color(muted)
                            .child("/")
                            .into_any_element(),
                    );
                }
                let current = index + 1 == segments.len();
                crumbs.push(
                    gpui::div()
                        .text_sm()
                        .text_color(if current { theme.foreground } else { muted })
                        .child((*segment).to_owned())
                        .into_any_element(),
                );
            }
        }

        // A grid of tiles, the way a launcher or a Finder icon view reads: the
        // icon says what kind of thing it is and the name sits under it, so a
        // column of paths is not the only way to know what is here.
        const TILE: f32 = 112.0;
        const GAP: f32 = 8.0;
        let list_width = (width as f32) * 0.58;
        let columns = (((list_width - 24.0) / (TILE + GAP)).floor() as usize).max(1);
        self.columns = columns;

        let tiles: Vec<gpui::AnyElement> = self
            .entries
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let selected = index == self.selected;
                v_flex()
                    .w(px(TILE))
                    .h(px(TILE + 8.0))
                    .items_center()
                    .justify_start()
                    .gap_1p5()
                    .pt_2()
                    .rounded_lg()
                    .when(selected, |this| this.bg(theme.accent))
                    // A fixed well for the icon, so names sit on one line across
                    // the grid no matter how tall the glyph is. A row flex, so
                    // `justify_center` is what centres the icon horizontally.
                    .child(
                        h_flex()
                            .h(px(52.0))
                            .w_full()
                            .items_center()
                            .justify_center()
                            .child(tile_icon(row, theme)),
                    )
                    .child(
                        gpui::div()
                            .px_1()
                            .max_w_full()
                            .text_xs()
                            .truncate()
                            .text_color(theme.foreground)
                            .child(row.name.clone()),
                    )
                    .when(!row.hydrated, |this| {
                        this.child(gpui::div().text_xs().text_color(muted).child("not here"))
                    })
                    .into_any_element()
            })
            .collect();

        v_flex()
            .size_full()
            .bg(theme.background)
            .text_sm()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(border)
                    .child(h_flex().items_center().gap_1().children(crumbs))
                    .child(gpui::div().text_xs().text_color(muted).child(format!(
                        "{} entr{} · enter opens · backspace up",
                        self.entries.len(),
                        if self.entries.len() == 1 { "y" } else { "ies" }
                    ))),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        h_flex()
                            .w(px(list_width))
                            .h_full()
                            .items_start()
                            .content_start()
                            .flex_wrap()
                            .gap(px(GAP))
                            .p_3()
                            .border_r_1()
                            .border_color(border)
                            .children(if self.entries.is_empty() {
                                vec![
                                    gpui::div()
                                        .p_1()
                                        .text_xs()
                                        .text_color(muted)
                                        .child("this directory is empty")
                                        .into_any_element(),
                                ]
                            } else {
                                tiles
                            }),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .p_3()
                            .gap_2()
                            .child(
                                v_flex()
                                    .gap_0p5()
                                    .child(
                                        gpui::div()
                                            .text_sm()
                                            .text_color(theme.foreground)
                                            .child(self.preview.title.clone()),
                                    )
                                    .when_some(subtitle, |this, text| {
                                        this.child(
                                            gpui::div().text_xs().text_color(muted).child(text),
                                        )
                                    }),
                            )
                            .child(
                                gpui::div()
                                    .text_xs()
                                    .text_color(if self.preview.error {
                                        theme.danger
                                    } else {
                                        muted
                                    })
                                    .child(self.preview.body.clone()),
                            )
                            .when(!self.preview.footer.is_empty(), |this| {
                                this.child(
                                    gpui::div()
                                        .text_xs()
                                        .text_color(theme.warning)
                                        .child(self.preview.footer.clone()),
                                )
                            }),
                    ),
            )
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .px_3()
                    .py_1()
                    .border_t_1()
                    .border_color(border)
                    .child(gpui::div().text_xs().text_color(muted).child(
                        self.note.clone().unwrap_or_else(
                            || match self.entries.get(self.selected) {
                                Some(row) => format!(
                                    "{} of {} · {}",
                                    self.selected + 1,
                                    self.entries.len(),
                                    row.name
                                ),
                                None => format!("{} × {}", width, height),
                            },
                        ),
                    ))
                    .child(
                        gpui::div()
                            .text_xs()
                            .text_color(muted)
                            .child("d theme · q quits"),
                    ),
            )
    }
}

fn run_xplorer() -> anyhow::Result<()> {
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

    cx.update(gpui_component::init);
    // Dark by default: this is a surface for looking at files at night as often as
    // in daylight, and the palette is the kit's, so one call moves everything.
    cx.update(|cx| {
        gpui_component::Theme::change(gpui_component::ThemeMode::Dark, None, cx);
    });

    let scale = surface::scale() as f32;
    let window =
        cx.open_window_scaled(size(px(width as f32), px(height as f32)), scale, |_, cx| {
            cx.new(|_| Finder::open())
        })?;
    let any: gpui::AnyWindowHandle = window.into();

    let mut dirty = true;
    let mut window_size = (width, height);
    loop {
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
                if !key.pressed {
                    continue;
                }
                if key.key == "q" {
                    break;
                }
                let changed = window.update(&mut cx, |finder, _, cx| {
                    // The theme is app state, not view state, so the toggle happens
                    // here where the App is in hand; the view only asks for a redraw.
                    if key.key == "d" {
                        let dark = !cx.theme().mode.is_dark();
                        gpui_component::Theme::change(
                            if dark {
                                gpui_component::ThemeMode::Dark
                            } else {
                                gpui_component::ThemeMode::Light
                            },
                            None,
                            cx,
                        );
                        cx.notify();
                        return true;
                    }
                    let changed = finder.handle_key(&key.key);
                    if changed {
                        cx.notify();
                    }
                    changed
                })?;
                dirty |= changed;
            }
            Some(input::Event::Moved(pointer)) => {
                // The list is not hit-tested yet, so the pointer reports nothing it
                // can act on rather than pretending a hover is a selection.
                let _ = pointer;
                let _ = cursor::set(cursor::CursorStyle::Default);
            }
            Some(input::Event::Button(_)) | Some(input::Event::Scrolled(_)) | None => {}
        }
    }
    Ok(())
}

fn present_frame(
    context: &WgpuContext,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> anyhow::Result<()> {
    let texture = context.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("xplorer frame"),
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

struct Component;

impl Guest for Component {
    async fn run() -> u32 {
        match run_xplorer() {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("xplorer: {error:#}");
                1
            }
        }
    }
}

export!(Component);
