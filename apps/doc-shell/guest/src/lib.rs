//! doc-shell as a Spore: a Word document, read from its own bytes.
//!
//! The window is a vault on the left and a document on the right, and the
//! document is drawn from the paragraphs the container already names — so a
//! block on screen has an identity that survives a re-read, without a store in
//! between. Nothing here walks a disk: a path comes from `soil:vfs`, the bytes
//! come from `soil:vfs` at a bounded size, and a path the host does not grant
//! simply does not exist.
//!
//! Keyboard first: arrows move, `Enter` descends a directory, `Backspace` goes
//! up, `d` toggles the theme, `q` quits.

#[cfg(not(feature = "demo"))]
wit_bindgen::generate!({
    path: "wit",
    world: "runner",
    with: {
        "wasi:webgpu/webgpu@0.3.0-rc.2": wgpu_wasi_webgpu::wasi::webgpu::webgpu,
    },
    generate_all,
});

// The same window without the namespace: a host that has not linked
// `soil:vfs` cannot instantiate a component that imports it, so this build
// renders against a vault `data.rs` writes in memory. Never the shipped Spore.
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

mod comments;
mod data;
mod docx;

const LILEX: &[u8] = include_bytes!("../assets/Lilex-Regular.ttf");
const LILEX_BOLD: &[u8] = include_bytes!("../assets/Lilex-Bold.ttf");

/// One row in the vault listing.
struct Row {
    name: String,
    path: String,
    directory: bool,
    size: u64,
}

fn sort(mut rows: Vec<Row>) -> Vec<Row> {
    rows.sort_by(|a, b| {
        b.directory
            .cmp(&a.directory)
            .then_with(|| a.name.cmp(&b.name))
    });
    rows
}

fn human_size(bytes: u64) -> String {
    match bytes {
        0..=1023 => format!("{bytes} B"),
        1024..=1_048_575 => format!("{:.1} kB", bytes as f64 / 1024.0),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.0),
    }
}

/// What the right pane is showing.
enum Pane {
    Empty,
    NotWord {
        name: String,
        who: String,
    },
    Document {
        name: String,
        document: docx::Document,
    },
    Failed {
        name: String,
        why: String,
    },
}

/// Where a key goes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    Vault,
    Document,
}

/// One window of the reader.
struct Shell {
    /// Workspace-relative path of the directory being listed; empty is the root.
    path: String,
    entries: Vec<Row>,
    selected: usize,
    pane: Pane,
    note: Option<String>,
    /// Which pane the keyboard is talking to.
    focus: Focus,
    /// The block the cursor is on, in the open document.
    block: usize,
    /// The document's pending comments, if one is open.
    comments: Option<comments::Pending>,
    /// A comment being written, if one is.
    compose: Option<String>,
}

impl Shell {
    fn open() -> Self {
        let mut shell = Self {
            path: String::new(),
            entries: Vec::new(),
            selected: 0,
            pane: Pane::Empty,
            note: None,
            focus: Focus::Vault,
            block: 0,
            comments: None,
            compose: None,
        };
        shell.reload();
        // The harness build opens on a Word file, so a captured frame shows the
        // reader doing its job instead of an empty pane.
        if let Some(index) = shell.entries.iter().position(|row| row.name == "nda.docx") {
            shell.select(index);
        }
        shell
    }

    fn reload(&mut self) {
        match data::ls(&self.path) {
            Ok(rows) => {
                self.entries = sort(rows);
                self.note = None;
                self.select(0);
            }
            Err(error) => {
                self.entries.clear();
                self.selected = 0;
                self.pane = Pane::Empty;
                self.note = Some(format!("{}: {error}", describe(&self.path)));
            }
        }
    }

    /// Move the selection and read whatever is under it.
    fn select(&mut self, index: usize) {
        if self.entries.is_empty() {
            self.pane = Pane::Empty;
            return;
        }
        self.selected = index.min(self.entries.len() - 1);
        let row = &self.entries[self.selected];
        if row.directory {
            self.pane = Pane::Empty;
            return;
        }
        let name = row.name.clone();
        let path = row.path.clone();
        if !name.to_ascii_lowercase().ends_with(".docx") {
            self.pane = Pane::NotWord {
                name,
                who: "a Word file (.docx) is what this pane reads".to_owned(),
            };
            return;
        }
        self.comments = None;
        self.compose = None;
        self.block = 0;
        match data::read(&path, data::DOC_BYTES) {
            Ok(bytes) => match docx::read(&bytes) {
                Ok(document) => {
                    self.comments = Some(comments::open(&path));
                    self.pane = Pane::Document { name, document };
                }
                Err(error) => {
                    self.pane = Pane::Failed {
                        name,
                        why: format!("{error:#}"),
                    }
                }
            },
            Err(error) => self.pane = Pane::Failed { name, why: error },
        }
    }

    fn enter(&mut self) {
        let Some(row) = self.entries.get(self.selected) else {
            return;
        };
        if !row.directory {
            self.note = Some(format!(
                "{} is a file; it is already open on the right",
                row.name
            ));
            return;
        }
        self.path = row.path.clone();
        self.reload();
    }

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

    /// How many blocks the open document has.
    fn blocks(&self) -> usize {
        match &self.pane {
            Pane::Document { document, .. } => document.blocks.len(),
            _ => 0,
        }
    }

    /// The id of the block the cursor is on.
    fn block_id(&self) -> Option<&str> {
        match &self.pane {
            Pane::Document { document, .. } => document
                .blocks
                .get(self.block)
                .map(|block| block.id.as_str()),
            _ => None,
        }
    }

    fn handle_key(&mut self, key: &str) -> bool {
        // Composing takes every key: a comment is prose, not a command.
        if let Some(text) = self.compose.as_mut() {
            match key {
                "enter" => {
                    let body = text.trim().to_owned();
                    let block = self.block_id().map(ToOwned::to_owned);
                    self.compose = None;
                    if body.is_empty() {
                        return true;
                    }
                    if let (Some(block), Some(pending)) = (block, self.comments.as_mut()) {
                        pending.add(&block, &body);
                        self.note = Some(format!("comment left on {block}"));
                    }
                    return true;
                }
                "escape" => {
                    self.compose = None;
                    return true;
                }
                "backspace" => {
                    text.pop();
                    return true;
                }
                "space" => {
                    text.push(' ');
                    return true;
                }
                other if other.chars().count() == 1 => {
                    text.push_str(other);
                    return true;
                }
                _ => return false,
            }
        }

        match key {
            "tab" => {
                self.focus = match self.focus {
                    Focus::Vault => Focus::Document,
                    Focus::Document => Focus::Vault,
                };
                true
            }
            "c" if self.focus == Focus::Document && self.blocks() > 0 => {
                if self.comments.is_none()
                    && let Pane::Document { .. } = self.pane
                {
                    return false;
                }
                self.compose = Some(String::new());
                true
            }
            "x" if self.focus == Focus::Document => {
                let Some(block) = self.block_id().map(ToOwned::to_owned) else {
                    return false;
                };
                let Some(pending) = self.comments.as_mut() else {
                    return false;
                };
                let open = pending
                    .for_block(&block)
                    .iter()
                    .any(|comment| !comment.resolved);
                pending.set_resolved(&block, open);
                self.note = Some(if open {
                    format!("resolved the thread on {block}")
                } else {
                    format!("reopened the thread on {block}")
                });
                true
            }
            "arrow-down" | "j" => {
                match self.focus {
                    Focus::Vault => self.select(self.selected + 1),
                    Focus::Document => {
                        self.block = (self.block + 1).min(self.blocks().saturating_sub(1));
                    }
                }
                true
            }
            "arrow-up" | "k" => {
                match self.focus {
                    Focus::Vault => {
                        self.selected = self.selected.saturating_sub(1);
                        self.select(self.selected);
                    }
                    Focus::Document => self.block = self.block.saturating_sub(1),
                }
                true
            }
            "enter" if self.focus == Focus::Vault => {
                self.enter();
                true
            }
            "enter" => {
                // In the document, Enter is the gesture that starts writing on
                // the block under the cursor.
                if self.blocks() > 0 {
                    self.compose = Some(String::new());
                    return true;
                }
                false
            }
            "backspace" if self.focus == Focus::Vault => {
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

fn describe(path: &str) -> String {
    if path.is_empty() {
        "/".to_owned()
    } else {
        path.to_owned()
    }
}

impl Render for Shell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let border = theme.border;

        let segments: Vec<&str> = self
            .path
            .split('/')
            .filter(|part| !part.is_empty())
            .collect();
        let mut crumbs: Vec<gpui::AnyElement> = Vec::new();
        crumbs.push(
            gpui::div()
                .text_sm()
                .text_color(if segments.is_empty() {
                    theme.foreground
                } else {
                    muted
                })
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

        let rows: Vec<gpui::AnyElement> = self
            .entries
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let selected = index == self.selected;
                h_flex()
                    .w_full()
                    .justify_between()
                    .items_center()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .when(selected, |this| this.bg(theme.accent))
                    .child(gpui::div().text_sm().text_color(theme.foreground).child(
                        if row.directory {
                            format!("{}/", row.name)
                        } else {
                            row.name.clone()
                        },
                    ))
                    .child(
                        gpui::div()
                            .text_xs()
                            .text_color(muted)
                            .child(if row.directory {
                                String::new()
                            } else {
                                human_size(row.size)
                            }),
                    )
                    .into_any_element()
            })
            .collect();

        let (title, meta, blocks, failed) = match &self.pane {
            Pane::Document { name, document } => (
                name.clone(),
                format!(
                    "{} blocks · {} named by Word · {} synthetic · {} headings{}",
                    document.blocks.len(),
                    document.identified,
                    document.synthetic,
                    document.headings(),
                    match self.comments.as_ref().map(comments::Pending::unresolved) {
                        Some(0) | None => String::new(),
                        Some(pending) => format!(" · {pending} pending comments"),
                    }
                ),
                Some(document),
                false,
            ),
            Pane::NotWord { name, who } => (name.clone(), who.clone(), None, false),
            Pane::Failed { name, why } => (name.clone(), why.clone(), None, true),
            Pane::Empty => (
                "no document open".to_owned(),
                "select a Word file in the vault".to_owned(),
                None,
                false,
            ),
        };

        // Each block: its identity in the gutter, its text where the eye
        // expects it, and the thread still pending on it to the right. A heading
        // is a heading because the container said so.
        let mut rendered: Vec<gpui::AnyElement> = Vec::new();
        for (index, block) in blocks
            .into_iter()
            .flat_map(|document| &document.blocks)
            .enumerate()
        {
            let heading = block
                .style
                .as_deref()
                .is_some_and(|style| style.starts_with("Heading"));
            let thread = self
                .comments
                .as_ref()
                .map(|pending| pending.for_block(&block.id))
                .unwrap_or_default();
            let focused = self.focus == Focus::Document && index == self.block;
            rendered.push(
                h_flex()
                    .items_start()
                    .gap_3()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .when(heading, |this| this.mt_2())
                    .when(focused, |this| this.bg(theme.accent))
                    .child(
                        gpui::div()
                            .w(px(88.0))
                            .text_xs()
                            .text_color(if block.identified {
                                muted
                            } else {
                                theme.warning
                            })
                            .child(if block.identified {
                                block.id.clone()
                            } else {
                                format!("{} · position", block.id)
                            }),
                    )
                    .child(
                        gpui::div()
                            .flex_1()
                            .when(heading, |this| this.text_lg())
                            .when(!heading, |this| this.text_sm())
                            .text_color(theme.foreground)
                            .child(if block.text.is_empty() {
                                "\u{2014}".to_owned()
                            } else {
                                block.text.clone()
                            }),
                    )
                    .child(
                        v_flex()
                            .w(px(260.0))
                            .gap_1()
                            .children(thread.into_iter().map(|comment| {
                                v_flex()
                                    .gap_0p5()
                                    .child(
                                        h_flex()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                gpui::div()
                                                    .text_xs()
                                                    .text_color(if comment.resolved {
                                                        muted
                                                    } else {
                                                        theme.foreground
                                                    })
                                                    .child(comment.author.clone()),
                                            )
                                            .when(comment.resolved, |this| {
                                                this.child(
                                                    gpui::div()
                                                        .text_xs()
                                                        .text_color(muted)
                                                        .child("· resolved"),
                                                )
                                            }),
                                    )
                                    .child(
                                        gpui::div()
                                            .text_xs()
                                            .text_color(if comment.resolved {
                                                muted
                                            } else {
                                                theme.foreground
                                            })
                                            .child(comment.body.clone()),
                                    )
                                    .into_any_element()
                            })),
                    )
                    .into_any_element(),
            );
        }

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
                        v_flex()
                            .w(px(320.0))
                            .h_full()
                            .p_2()
                            .gap_0p5()
                            .border_r_1()
                            .border_color(border)
                            .when(self.entries.is_empty(), |this| {
                                this.child(
                                    gpui::div()
                                        .p_2()
                                        .text_xs()
                                        .text_color(muted)
                                        .child("this directory is empty"),
                                )
                            })
                            .children(rows),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .p_4()
                            .gap_2()
                            .child(
                                v_flex()
                                    .gap_0p5()
                                    .child(
                                        gpui::div()
                                            .text_sm()
                                            .text_color(theme.foreground)
                                            .child(title),
                                    )
                                    .child(
                                        gpui::div()
                                            .text_xs()
                                            .text_color(if failed { theme.danger } else { muted })
                                            .child(meta),
                                    ),
                            )
                            .children(rendered)
                            .when_some(self.compose.clone(), |this, text| {
                                this.child(
                                    h_flex()
                                        .gap_2()
                                        .mt_3()
                                        .px_2()
                                        .py_2()
                                        .rounded_md()
                                        .border_1()
                                        .border_color(border)
                                        .child(gpui::div().text_xs().text_color(muted).child(
                                            format!(
                                                "comment on {}:",
                                                self.block_id().unwrap_or("—")
                                            ),
                                        ))
                                        .child(
                                            gpui::div()
                                                .flex_1()
                                                .text_sm()
                                                .text_color(theme.foreground)
                                                .child(format!("{text}_")),
                                        )
                                        .child(
                                            gpui::div()
                                                .text_xs()
                                                .text_color(muted)
                                                .child("enter leaves it · esc cancels"),
                                        ),
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
                                None => "—".to_owned(),
                            },
                        ),
                    ))
                    .child(
                        gpui::div()
                            .text_xs()
                            .text_color(muted)
                            .child(match self.focus {
                                Focus::Vault => "tab document · d theme · q quits",
                                Focus::Document => "c comment · x resolve · tab vault",
                            }),
                    ),
            )
    }
}

fn run_shell() -> anyhow::Result<()> {
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
    // Dark by default, and `d` toggles it: the palette is the kit's, so one call
    // moves every colour.
    cx.update(|cx| {
        gpui_component::Theme::change(gpui_component::ThemeMode::Dark, None, cx);
    });

    let scale = surface::scale() as f32;
    let window =
        cx.open_window_scaled(size(px(width as f32), px(height as f32)), scale, |_, cx| {
            cx.new(|_| Shell::open())
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
                let changed = window.update(&mut cx, |shell, _, cx| {
                    // The theme is app state, not view state, so the toggle
                    // happens here where the App is in hand.
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
                    let changed = shell.handle_key(&key.key);
                    if changed {
                        cx.notify();
                    }
                    changed
                })?;
                dirty |= changed;
            }
            Some(input::Event::Moved(_)) => {
                // No hit-testing yet: a hover that silently moves the selection
                // would be worse than no hover.
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
        label: Some("doc-shell frame"),
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
        match run_shell() {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("doc-shell: {error:#}");
                1
            }
        }
    }
}

export!(Component);
