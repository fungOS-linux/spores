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
    AnyElement, AppContext as _, Context, HeadlessAppContext, InteractiveElement as _, IntoElement,
    ParentElement as _, PlatformTextSystem as _, Render, ScrollHandle,
    StatefulInteractiveElement as _, Styled as _, Window, px, size,
};
use gpui_base::StyledExt as _;
use gpui_component::status_bar::StatusBar;
use gpui_component::{ActiveTheme as _, h_flex, v_flex};
use gpui_wgpu::{CosmicTextSystem, WgpuContext, WgpuHeadlessRenderer};
use soil::present::{cursor, input, surface};

mod comments;
mod data;
mod docx;

const LILEX: &[u8] = include_bytes!("../assets/Lilex-Regular.ttf");
const LILEX_BOLD: &[u8] = include_bytes!("../assets/Lilex-Bold.ttf");

/// One row in the vault listing.
#[derive(Clone)]
struct Row {
    name: String,
    path: String,
    directory: bool,
    size: u64,
}

/// Whether this is a file the reader can open. Everything else is listed, and
/// says what it is when it is picked.
fn is_word(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".docx")
}

/// A file's extension, so a row that is not a Word file can name what it is
/// rather than only what it is not.
fn extension(name: &str) -> Option<&str> {
    name.rsplit_once('.')
        .map(|(_, extension)| extension)
        .filter(|extension| {
            !extension.is_empty()
                && extension.len() <= 5
                && extension.chars().all(|c| c.is_ascii_alphanumeric())
        })
}

/// `1 block`, `3 blocks`.
fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
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
    /// A directory, an empty vault, or nothing picked yet.
    Empty,
    NotWord {
        name: String,
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

/// A pane with nothing to read: what is in the way, and the one key that gets
/// out of it. Every state that is not a document goes through here, so none of
/// them can dead-end.
struct Notice {
    title: String,
    detail: String,
    next: String,
    /// The reader's own words for a parse that failed.
    why: Option<String>,
    failed: bool,
}

/// Where a key goes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    Vault,
    Document,
}

/// What covers the pane.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Overlay {
    None,
    Help,
    Outline,
}

/// What the keyboard is composing. A comment and a filter are both text and
/// neither is a command, so both take every key the same way.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Typing {
    Filter,
    Comment,
}

/// The window's two side columns. Derived from the width in one place, so the
/// rail gives way before a line of prose does.
struct Columns {
    vault: f32,
    rail: f32,
}

impl Columns {
    fn of(width: f32) -> Self {
        Self {
            vault: (width * 0.20).clamp(220.0, 260.0),
            rail: (width * 0.23).clamp(240.0, 300.0),
        }
    }
}

/// The margin the container's own identities sit in, and the widest a line of
/// prose is allowed to run. `1A2B3C4D` at 12px is 54px; a 560px measure is
/// about 66 characters of Lilex.
const GUTTER: f32 = 64.0;
const MEASURE: f32 = 560.0;

/// One window of the reader.
struct Shell {
    /// Workspace-relative path of the directory being listed; empty is the root.
    path: String,
    /// Everything the directory holds.
    all: Vec<Row>,
    /// What the vault shows: `all`, narrowed by the filter.
    entries: Vec<Row>,
    selected: usize,
    /// The applied vault filter; also the buffer while one is being typed.
    filter: String,
    /// The path of the document in the pane, so a re-list does not re-read it.
    open: String,
    pane: Pane,
    note: Option<String>,
    /// Which pane the keyboard is talking to.
    focus: Focus,
    /// The block the cursor is on, in the open document.
    block: usize,
    /// The document's pending comments, if one is open.
    comments: Option<comments::Pending>,
    overlay: Overlay,
    typing: Option<Typing>,
    /// The comment being written.
    draft: String,
    /// Long documents, long vaults and long thread lists keep their cursor in
    /// view themselves; there is no pointer to scroll with.
    document_scroll: ScrollHandle,
    thread_scroll: ScrollHandle,
    vault_scroll: ScrollHandle,
}

impl Shell {
    fn open() -> Self {
        let mut shell = Self {
            path: String::new(),
            all: Vec::new(),
            entries: Vec::new(),
            selected: 0,
            filter: String::new(),
            open: String::new(),
            pane: Pane::Empty,
            note: None,
            focus: Focus::Vault,
            block: 0,
            comments: None,
            overlay: Overlay::None,
            typing: None,
            draft: String::new(),
            document_scroll: ScrollHandle::default(),
            thread_scroll: ScrollHandle::default(),
            vault_scroll: ScrollHandle::default(),
        };
        shell.reload();
        // The harness build opens on a Word file with the document focused, so a
        // captured frame shows the cursor, the threads and the composer hint
        // instead of an empty pane. The shipped build opens on the vault.
        #[cfg(feature = "demo")]
        if let Some(index) = shell.entries.iter().position(|row| row.name == "nda.docx") {
            shell.select(index);
            shell.focus = Focus::Document;
        }
        shell
    }

    fn reload(&mut self) {
        // A reload re-reads the open document as well as the listing: `r` is how
        // a file that changed under us gets picked up.
        self.open.clear();
        match data::ls(&self.path) {
            Ok(rows) => {
                self.all = sort(rows);
                self.note = None;
                self.apply_filter();
            }
            Err(error) => {
                self.all.clear();
                self.entries.clear();
                self.selected = 0;
                self.pane = Pane::Empty;
                self.comments = None;
                self.note = Some(format!("{}: {error}", describe(&self.path)));
            }
        }
    }

    /// Narrow the vault to the rows matching the filter and re-read what is
    /// under the cursor.
    fn apply_filter(&mut self) {
        let needle = self.filter.to_lowercase();
        self.entries = self
            .all
            .iter()
            .filter(|row| needle.is_empty() || row.name.to_lowercase().contains(&needle))
            .cloned()
            .collect();
        self.selected = 0;
        self.select(0);
    }

    /// Move the selection and read whatever is under it.
    fn select(&mut self, index: usize) {
        if self.entries.is_empty() {
            self.selected = 0;
            self.open.clear();
            self.pane = Pane::Empty;
            self.comments = None;
            return;
        }
        self.selected = index.min(self.entries.len() - 1);
        let row = self.entries[self.selected].clone();
        if row.directory || row.path == self.open {
            // A directory has nothing to read, and the open document must not be
            // re-parsed (or lose its block cursor) just because the filter
            // changed around it.
            if row.directory {
                self.open.clear();
                self.pane = Pane::Empty;
                self.comments = None;
            }
            return;
        }
        self.open = row.path.clone();
        self.comments = None;
        self.typing = None;
        self.draft.clear();
        self.block = 0;
        if !is_word(&row.name) {
            self.pane = Pane::NotWord { name: row.name };
            return;
        }
        match data::read(&row.path, data::DOC_BYTES) {
            Ok(bytes) => match docx::read(&bytes) {
                Ok(document) => {
                    self.comments = Some(comments::open(&row.path));
                    self.pane = Pane::Document {
                        name: row.name,
                        document,
                    };
                }
                Err(error) => {
                    self.pane = Pane::Failed {
                        name: row.name,
                        why: format!("{error:#}"),
                    }
                }
            },
            Err(error) => {
                self.pane = Pane::Failed {
                    name: row.name,
                    why: error,
                }
            }
        }
    }

    fn enter(&mut self) {
        let Some(row) = self.entries.get(self.selected).cloned() else {
            return;
        };
        if !row.directory {
            self.note = Some(format!(
                "{} is a file; it is already open on the right",
                row.name
            ));
            return;
        }
        self.path = row.path;
        self.filter.clear();
        self.typing = None;
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
        self.filter.clear();
        self.typing = None;
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

    fn block_at(&self, index: usize) -> Option<&docx::Block> {
        match &self.pane {
            Pane::Document { document, .. } => document.blocks.get(index),
            _ => None,
        }
    }

    /// The id of the block the cursor is on.
    fn block_id(&self) -> Option<&str> {
        self.block_at(self.block).map(|block| block.id.as_str())
    }

    /// Every heading in the open document, with the block it starts.
    fn outline(&self) -> Vec<(usize, &docx::Block)> {
        match &self.pane {
            Pane::Document { document, .. } => document
                .blocks
                .iter()
                .enumerate()
                .filter(|(_, block)| block.heading().is_some())
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Move the block cursor.
    fn cursor_to(&mut self, index: usize) {
        self.block = index.min(self.blocks().saturating_sub(1));
    }

    /// Where the thread on one block sits in the rail, if it has one.
    fn thread_group(&self, at: usize) -> Option<usize> {
        let pending = self.comments.as_ref()?;
        let Pane::Document { document, .. } = &self.pane else {
            return None;
        };
        let mut group = 0;
        for (index, paragraph) in document.blocks.iter().enumerate() {
            if pending.for_block(&paragraph.id).is_empty() {
                continue;
            }
            if index == at {
                return Some(group);
            }
            group += 1;
        }
        None
    }

    /// The next block past the cursor in `direction` that satisfies `wanted`.
    fn seek(&self, direction: i32, wanted: impl Fn(usize) -> bool) -> Option<usize> {
        let count = self.blocks();
        let mut index = self.block as i32 + direction;
        while index >= 0 && (index as usize) < count {
            if wanted(index as usize) {
                return Some(index as usize);
            }
            index += direction;
        }
        None
    }

    /// Jump to the next or previous block that has a thread on it.
    fn to_thread(&mut self, direction: i32) {
        let (found, note) = {
            let Some(pending) = self.comments.as_ref() else {
                return;
            };
            let pane = &self.pane;
            let found = self.seek(direction, |index| match pane {
                Pane::Document { document, .. } => document
                    .blocks
                    .get(index)
                    .is_some_and(|block| !pending.for_block(&block.id).is_empty()),
                _ => false,
            });
            let note = found.is_none().then(|| {
                match (direction, pending.threads().is_empty()) {
                    (_, true) => "no comments on this document",
                    (1, false) => "no thread after this block",
                    _ => "no thread before this block",
                }
                .to_owned()
            });
            (found, note)
        };
        self.focus = Focus::Document;
        match found {
            Some(index) => self.cursor_to(index),
            None => self.note = note,
        }
    }

    /// Why the pane has nothing to read, and what to press about it.
    fn notice(&self) -> Option<Notice> {
        let notice = |title: String, detail: String, next: String| Notice {
            title,
            detail,
            next,
            why: None,
            failed: false,
        };
        match &self.pane {
            Pane::Document { .. } => None,
            Pane::NotWord { name } => Some(notice(
                name.clone(),
                match extension(name) {
                    Some(extension) => {
                        format!("doc-shell reads Word files (.docx); this is .{extension}")
                    }
                    None => "doc-shell reads Word files (.docx); this has no extension".to_owned(),
                },
                "j / k picks another file".to_owned(),
            )),
            Pane::Failed { name, why } => Some(Notice {
                title: name.clone(),
                detail: "could not be read as a Word file".to_owned(),
                next: "r re-reads the vault · j / k picks another file".to_owned(),
                why: Some(why.clone()),
                failed: true,
            }),
            Pane::Empty => Some(match self.entries.get(self.selected) {
                Some(row) if row.directory => notice(
                    format!("{}/", row.name),
                    "a directory in the vault".to_owned(),
                    "enter opens it".to_owned(),
                ),
                Some(_) => notice(
                    "no document open".to_owned(),
                    "pick a file with j / k, and enter reads it".to_owned(),
                    "/ filters the vault".to_owned(),
                ),
                None if !self.filter.is_empty() => notice(
                    format!("no file matches {}", self.filter),
                    format!(
                        "{} in {}",
                        count(self.all.len(), "file", "files"),
                        describe(&self.path)
                    ),
                    "esc clears the filter".to_owned(),
                ),
                None if self.all.is_empty() => notice(
                    "this directory is empty".to_owned(),
                    format!("{} holds no files", describe(&self.path)),
                    "backspace goes up a directory".to_owned(),
                ),
                None => notice(
                    "no document open".to_owned(),
                    "pick a file with j / k, and enter reads it".to_owned(),
                    "/ filters the vault".to_owned(),
                ),
            }),
        }
    }

    fn typing(&self) -> bool {
        self.typing.is_some()
    }

    /// Leave whatever is covering the pane, then whatever is narrowing the
    /// vault, then whatever the last key had to say.
    fn escape(&mut self) -> bool {
        if self.overlay != Overlay::None {
            self.overlay = Overlay::None;
        } else if !self.filter.is_empty() {
            self.filter.clear();
            self.apply_filter();
        } else if self.note.is_some() {
            self.note = None;
        } else {
            return false;
        }
        true
    }

    /// Post the comment being written on the block under the cursor.
    fn post(&mut self) {
        let body = self.draft.trim().to_owned();
        self.typing = None;
        self.draft.clear();
        if body.is_empty() {
            return;
        }
        let (Some(block), Some(pending)) = (
            self.block_id().map(ToOwned::to_owned),
            self.comments.as_mut(),
        ) else {
            return;
        };
        pending.add(&block, &body);
        self.note = Some(format!("comment left on {block}"));
    }

    /// Resolve the thread on the block under the cursor, or reopen it.
    fn resolve(&mut self) {
        let Some(block) = self.block_id().map(ToOwned::to_owned) else {
            return;
        };
        let open = self.comments.as_ref().and_then(|pending| {
            let thread = pending.for_block(&block);
            (!thread.is_empty()).then(|| thread.iter().any(|comment| !comment.resolved))
        });
        let Some(open) = open else {
            self.note = Some(format!("no thread on {block} to resolve"));
            return;
        };
        if let Some(pending) = self.comments.as_mut() {
            pending.set_resolved(&block, open);
        }
        self.note = Some(if open {
            format!("resolved the thread on {block}")
        } else {
            format!("reopened the thread on {block}")
        });
    }

    /// Every key a Spore can receive, and what it means.
    fn handle_key(&mut self, key: &str, text: Option<&str>) -> bool {
        if let Some(typing) = self.typing {
            return self.type_key(typing, key, text);
        }
        match key {
            "tab" => {
                self.focus = match self.focus {
                    Focus::Vault => Focus::Document,
                    Focus::Document => Focus::Vault,
                };
                true
            }
            "escape" => self.escape(),
            "?" => {
                self.overlay = match self.overlay {
                    Overlay::Help => Overlay::None,
                    _ => Overlay::Help,
                };
                true
            }
            "o" if self.blocks() > 0 => {
                self.overlay = match self.overlay {
                    Overlay::Outline => Overlay::None,
                    _ => Overlay::Outline,
                };
                true
            }
            "/" => {
                self.focus = Focus::Vault;
                self.typing = Some(Typing::Filter);
                true
            }
            "c" if self.focus == Focus::Document && self.comments.is_some() => {
                self.typing = Some(Typing::Comment);
                true
            }
            "x" if self.focus == Focus::Document => {
                self.resolve();
                true
            }
            "n" if self.focus == Focus::Document => {
                self.to_thread(1);
                true
            }
            "p" if self.focus == Focus::Document => {
                self.to_thread(-1);
                true
            }
            "j" | "arrow-down" => {
                match (self.overlay, self.focus) {
                    // In the outline, the movers move by heading: the overlay is
                    // the document read as sections rather than paragraphs.
                    (Overlay::Outline, _) => {
                        let found = self.seek(1, |index| {
                            self.block_at(index)
                                .is_some_and(|block| block.heading().is_some())
                        });
                        if let Some(index) = found {
                            self.cursor_to(index);
                        }
                    }
                    (_, Focus::Vault) => self.select(self.selected + 1),
                    (_, Focus::Document) => self.cursor_to(self.block + 1),
                }
                true
            }
            "k" | "arrow-up" => {
                match (self.overlay, self.focus) {
                    (Overlay::Outline, _) => {
                        let found = self.seek(-1, |index| {
                            self.block_at(index)
                                .is_some_and(|block| block.heading().is_some())
                        });
                        if let Some(index) = found {
                            self.cursor_to(index);
                        }
                    }
                    (_, Focus::Vault) => self.select(self.selected.saturating_sub(1)),
                    (_, Focus::Document) => self.cursor_to(self.block.saturating_sub(1)),
                }
                true
            }
            "enter" if self.focus == Focus::Vault => {
                self.enter();
                true
            }
            "enter" if self.focus == Focus::Document && self.comments.is_some() => {
                // In the document, Enter is the gesture that starts writing on
                // the block under the cursor.
                self.typing = Some(Typing::Comment);
                true
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

    /// Typing takes every key: prose is not a command.
    fn type_key(&mut self, typing: Typing, key: &str, text: Option<&str>) -> bool {
        match typing {
            Typing::Filter => match key {
                "enter" => self.typing = None,
                "escape" => {
                    self.typing = None;
                    self.filter.clear();
                    self.apply_filter();
                }
                "backspace" => {
                    self.filter.pop();
                    self.apply_filter();
                }
                _ => match text {
                    Some(typed) if typed.chars().count() == 1 => {
                        self.filter.push_str(typed);
                        self.apply_filter();
                    }
                    _ => return false,
                },
            },
            Typing::Comment => match key {
                "enter" => self.post(),
                "escape" => {
                    self.typing = None;
                    self.draft.clear();
                }
                "backspace" => {
                    self.draft.pop();
                }
                "space" => self.draft.push(' '),
                _ => match text {
                    Some(typed) if typed.chars().count() == 1 => self.draft.push_str(typed),
                    _ => return false,
                },
            },
        }
        true
    }
}

fn describe(path: &str) -> String {
    if path.is_empty() {
        "/".to_owned()
    } else {
        path.to_owned()
    }
}

/// A row's mark: a folder drawn from two rectangles (the kit's icon bundle is a
/// network fetch on wasm), or the paragraph mark a Word file gets, or a plain
/// dot for a file this pane cannot read.
fn marker(directory: bool, word: bool, colour: gpui::Hsla) -> AnyElement {
    let cell = h_flex().w(px(14.0)).flex_shrink_0().justify_center();
    if directory {
        return cell
            .child(
                v_flex()
                    .items_start()
                    .child(gpui::div().w(px(6.0)).h(px(3.0)).rounded_t_sm().bg(colour))
                    .child(gpui::div().w(px(11.0)).h(px(8.0)).rounded_sm().bg(colour)),
            )
            .into_any_element();
    }
    cell.child(gpui::div().text_xs().text_color(colour).child(if word {
        "\u{b6}"
    } else {
        "\u{b7}"
    }))
    .into_any_element()
}

impl Shell {
    /// The vault: where the reader is, what it holds, and which row has the
    /// keyboard.
    fn vault_view(&self, columns: &Columns, theme: &gpui_component::Theme) -> AnyElement {
        let muted = theme.muted_foreground;
        let border = theme.border;
        let focused = self.focus == Focus::Vault;

        let rows: Vec<AnyElement> = self
            .entries
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let selected = index == self.selected;
                let lit = selected && focused;
                let word = is_word(&row.name);
                let name = if lit {
                    theme.sidebar_accent_foreground
                } else if word || row.directory {
                    theme.sidebar_foreground
                } else {
                    muted
                };
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1p5()
                    .rounded_md()
                    // A selection without the keyboard is a place you were, not a
                    // place the keys are: it stays, quietly.
                    .when(selected, |this| {
                        this.bg(if lit {
                            theme.sidebar_accent
                        } else {
                            theme.muted
                        })
                    })
                    .child(marker(row.directory, word, name))
                    .child(
                        gpui::div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .text_color(name)
                            .child(row.name.clone()),
                    )
                    .child(
                        gpui::div()
                            .text_xs()
                            .text_color(name)
                            .child(if row.directory {
                                String::new()
                            } else {
                                human_size(row.size)
                            }),
                    )
                    .into_any_element()
            })
            .collect();

        let line = if self.typing == Some(Typing::Filter) {
            h_flex()
                .gap_1()
                .items_center()
                .child(gpui::div().text_xs().text_color(muted).child("/"))
                .child(
                    gpui::div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_xs()
                        .text_color(theme.foreground)
                        .child(format!("{}_", self.filter)),
                )
                .into_any_element()
        } else if self.filter.is_empty() {
            gpui::div()
                .text_xs()
                .text_color(muted)
                .child(count(self.entries.len(), "file", "files"))
                .into_any_element()
        } else {
            h_flex()
                .justify_between()
                .gap_2()
                .child(
                    gpui::div()
                        .text_xs()
                        .text_color(theme.foreground)
                        .child(format!("{} of {}", self.entries.len(), self.all.len())),
                )
                .child(
                    gpui::div()
                        .text_xs()
                        .text_color(muted)
                        .child(format!("/{} · esc clears", self.filter)),
                )
                .into_any_element()
        };

        v_flex()
            .w(px(columns.vault))
            .flex_shrink_0()
            .h_full()
            .bg(theme.sidebar)
            .border_r_1()
            .border_color(border)
            .child(
                v_flex()
                    .flex_shrink_0()
                    .gap_1()
                    .px_3()
                    .pt_3()
                    .pb_2()
                    .border_b_1()
                    .border_color(border)
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                gpui::div()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(if focused { theme.foreground } else { muted })
                                    .child("vault"),
                            )
                            .child(
                                gpui::div()
                                    .min_w_0()
                                    .truncate()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(describe(&self.path)),
                            ),
                    )
                    .child(line),
            )
            .child(
                v_flex()
                    .id("vault-rows")
                    .flex_1()
                    .min_h_0()
                    .gap_0p5()
                    .px_2()
                    .py_1()
                    .overflow_y_scroll()
                    // No pointer, so no drag target: the pane keeps its own
                    // selection in view and the count says how much is there.
                    .scrollbar_width(px(0.0))
                    .track_scroll(&self.vault_scroll)
                    .children(rows),
            )
            .into_any_element()
    }

    /// What the pane can say about the document above it: how much there is,
    /// and how much of it Word named.
    fn document_header(&self, theme: &gpui_component::Theme) -> Option<AnyElement> {
        let Pane::Document { document, .. } = &self.pane else {
            return None;
        };
        let derived = document.synthetic;
        let named = if derived == 0 {
            "every identity from Word".to_owned()
        } else {
            format!(
                "{} of {} identities from Word",
                document.identified,
                document.blocks.len()
            )
        };
        Some(
            h_flex()
                .flex_shrink_0()
                .w_full()
                .justify_center()
                .pt_3()
                .pb_2()
                .child(
                    // The same column the blocks are laid out in, so the
                    // document's summary sits over its own measure.
                    h_flex()
                        .w(px(GUTTER + 12.0 + MEASURE))
                        .max_w(gpui::relative(1.0))
                        .px_4()
                        .child(
                            gpui::div()
                                .min_w_0()
                                .truncate()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(format!(
                                    "{} · {} · {named}",
                                    count(document.blocks.len(), "paragraph", "paragraphs"),
                                    count(document.words(), "word", "words"),
                                )),
                        ),
                )
                .into_any_element(),
        )
    }

    /// The document, one row per paragraph: its margin identity and its text at
    /// a readable measure. Comments are not in this column — a thread taller
    /// than its paragraph would push the next paragraph down and break the
    /// document's own rhythm.
    fn blocks_view(&self, theme: &gpui_component::Theme) -> AnyElement {
        let muted = theme.muted_foreground;
        let border = theme.border;
        let Pane::Document { document, .. } = &self.pane else {
            return v_flex().into_any_element();
        };

        let mut rows: Vec<AnyElement> = Vec::with_capacity(document.blocks.len());
        for (index, block) in document.blocks.iter().enumerate() {
            let level = block.heading();
            let cursor = index == self.block;
            let lit = cursor && self.focus == Focus::Document;

            // Section rhythm: a heading owns the text under it, so it takes the
            // space above and gives it back below.
            let (top, bottom) = match level {
                Some(1) if index == 0 => (0.0, 6.0),
                Some(1) => (26.0, 6.0),
                Some(_) => (18.0, 2.0),
                None => (5.0, 5.0),
            };
            let line = if level == Some(1) { 28.0 } else { 22.0 };

            rows.push(
                h_flex()
                    .w_full()
                    .justify_center()
                    .pt(px(top))
                    .pb(px(bottom))
                    .child(
                        // The measure is centred in whatever room the rail
                        // leaves, so a wide window does not turn prose into a
                        // single long line.
                        h_flex()
                            .w(px(GUTTER + 12.0 + MEASURE))
                            .max_w(gpui::relative(1.0))
                            .items_stretch()
                            .gap_3()
                            .child(
                                // The cursor is a tick beside the first line:
                                // it says which block the keys are on, and
                                // nothing more.
                                gpui::div()
                                    .w(px(2.0))
                                    .h(px(18.0))
                                    .flex_shrink_0()
                                    .rounded_full()
                                    .when(cursor, |this| {
                                        this.bg(if lit { theme.primary } else { border })
                                    }),
                            )
                            .child(
                                // The container's own identity, in the margin
                                // where a margin note goes. An identity derived
                                // from position is marked by a dot and by the
                                // word, not by colour alone.
                                v_flex()
                                    .w(px(GUTTER))
                                    .flex_shrink_0()
                                    .items_end()
                                    .child(
                                        h_flex()
                                            .items_center()
                                            .gap_1()
                                            .when(!block.identified, |this| {
                                                this.child(
                                                    gpui::div()
                                                        .w(px(4.0))
                                                        .h(px(4.0))
                                                        .rounded_full()
                                                        .bg(theme.warning),
                                                )
                                            })
                                            .child(
                                                gpui::div()
                                                    .text_xs()
                                                    .line_height(px(line))
                                                    .text_color(if block.identified {
                                                        muted
                                                    } else {
                                                        theme.foreground
                                                    })
                                                    .child(block.id.clone()),
                                            ),
                                    )
                                    .when(!block.identified, |this| {
                                        this.child(
                                            gpui::div()
                                                .text_xs()
                                                .line_height(px(15.0))
                                                .text_color(muted)
                                                .child("position"),
                                        )
                                    }),
                            )
                            .child(
                                gpui::div()
                                    .flex_1()
                                    .min_w_0()
                                    .when(level == Some(1), |this| {
                                        this.text_xl().font_semibold().line_height(px(28.0))
                                    })
                                    .when(level == Some(2), |this| {
                                        this.text_base().font_semibold().line_height(px(23.0))
                                    })
                                    .when(level.is_some_and(|level| level > 2), |this| {
                                        this.text_sm().font_semibold().line_height(px(22.0))
                                    })
                                    .when(level.is_none(), |this| {
                                        this.text_sm().line_height(px(22.0))
                                    })
                                    .child(
                                        gpui::div()
                                            .text_color(if block.text.is_empty() {
                                                muted
                                            } else {
                                                theme.foreground
                                            })
                                            .child(if block.text.is_empty() {
                                                "\u{2014}".to_owned()
                                            } else {
                                                block.text.clone()
                                            }),
                                    ),
                            ),
                    )
                    .into_any_element(),
            );
        }

        v_flex()
            .id("blocks")
            .flex_1()
            .min_h_0()
            .pb_6()
            .overflow_y_scroll()
            .scrollbar_width(px(0.0))
            .track_scroll(&self.document_scroll)
            .children(rows)
            .into_any_element()
    }

    /// The rail: the threads pending on this document, in the document's own
    /// order, with the comment being written at the top where the keys are.
    ///
    /// A thread is the unit here. A resolved thread stays and dims, because a
    /// reply to a resolved thread still has to render.
    fn threads_view(&self, columns: &Columns, theme: &gpui_component::Theme) -> AnyElement {
        let muted = theme.muted_foreground;
        let border = theme.border;
        let Pane::Document { document, .. } = &self.pane else {
            return v_flex().into_any_element();
        };
        let pending = self.comments.as_ref();
        let cursor_thread = pending.is_some_and(|pending| {
            self.block_id()
                .is_some_and(|block| !pending.for_block(block).is_empty())
        });
        let composing = self.typing == Some(Typing::Comment);
        // What the rail can say about itself: what it drew, and whether the book
        // holds anything anchored to a block this file no longer has.
        let summary = |drawn: usize, open: usize| {
            let orphans = pending.map_or(0, |pending| pending.threads().len() - drawn);
            let mut line = match (drawn, open) {
                (0, _) => "none yet".to_owned(),
                (drawn, 0) => format!("{drawn} resolved"),
                (drawn, open) => format!("{open} open \u{b7} {} resolved", drawn - open),
            };
            if orphans > 0 {
                line.push_str(&format!(" \u{b7} {orphans} off this document"));
            }
            line
        };

        // Counted from the groups this rail draws, not from the book: the two
        // have to agree, and a comment anchored to a block the file no longer
        // has is the one case where they would not.
        let (mut drawn, mut open) = (0, 0);
        let mut groups: Vec<AnyElement> = Vec::new();
        if let Some(pending) = pending {
            for (index, block) in document.blocks.iter().enumerate() {
                let thread = pending.for_block(&block.id);
                if thread.is_empty() {
                    continue;
                }
                drawn += 1;
                let resolved = thread.iter().all(|comment| comment.resolved);
                if !resolved {
                    open += 1;
                }
                let here = index == self.block && self.focus == Focus::Document;
                let colour = if resolved { muted } else { theme.foreground };
                let cards = thread.iter().enumerate().map(|(n, comment)| {
                    v_flex()
                        .gap_0p5()
                        .px_2()
                        .py_1p5()
                        .rounded_md()
                        .bg(theme.muted)
                        .child(
                            h_flex()
                                .items_center()
                                .gap_1()
                                // A reply hangs off the comment above it rather
                                // than reading as a second opinion.
                                .when(n > 0, |this| {
                                    this.child(
                                        gpui::div().text_xs().text_color(muted).child("\u{21b3}"),
                                    )
                                })
                                .child(
                                    gpui::div()
                                        .text_xs()
                                        .font_semibold()
                                        .text_color(colour)
                                        .child(comment.author.clone()),
                                ),
                        )
                        .child(
                            gpui::div()
                                .text_xs()
                                .line_height(px(17.0))
                                .text_color(colour)
                                .child(comment.body.clone()),
                        )
                        .into_any_element()
                });
                groups.push(
                    v_flex()
                        .gap_1p5()
                        .px_3()
                        .py_2p5()
                        // Which thread the keys are on: a rule down its side,
                        // the way the document marks the block the same way.
                        .when(here, |this| this.border_l_2().border_color(theme.primary))
                        .child(
                            h_flex()
                                .items_center()
                                .gap_1p5()
                                .child(
                                    gpui::div()
                                        .text_xs()
                                        .text_color(if here { theme.foreground } else { muted })
                                        .child(format!("\u{b6} {}", block.id)),
                                )
                                .when(!block.identified, |this| {
                                    this.child(
                                        gpui::div().text_xs().text_color(muted).child("position"),
                                    )
                                })
                                .when(resolved, |this| {
                                    this.child(
                                        gpui::div()
                                            .text_xs()
                                            .text_color(muted)
                                            .child("\u{2713} resolved"),
                                    )
                                }),
                        )
                        .children(cards)
                        .into_any_element(),
                );
            }
        }

        let hint = if composing {
            gpui::div().into_any_element()
        } else if self.focus == Focus::Document {
            gpui::div()
                .text_xs()
                .text_color(muted)
                .child(if cursor_thread {
                    "+ reply \u{b7} c"
                } else {
                    "+ comment \u{b7} c"
                })
                .into_any_element()
        } else {
            gpui::div().into_any_element()
        };

        v_flex()
            .w(px(columns.rail))
            .flex_shrink_0()
            .h_full()
            .border_l_1()
            .border_color(border)
            .child(
                v_flex()
                    .flex_shrink_0()
                    .gap_1()
                    .px_3()
                    .pt_3()
                    .pb_2()
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                gpui::div()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(if self.focus == Focus::Document {
                                        theme.foreground
                                    } else {
                                        muted
                                    })
                                    .child("comments"),
                            )
                            .child(
                                gpui::div()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(summary(drawn, open)),
                            ),
                    )
                    .child(hint)
                    .when(composing, |this| {
                        this.child(
                            v_flex()
                                .gap_1()
                                .px_2()
                                .py_1p5()
                                .rounded_md()
                                .bg(theme.background)
                                .border_1()
                                .border_color(theme.ring)
                                .child(gpui::div().text_xs().text_color(muted).child(format!(
                                    "new comment on {}",
                                    self.block_id().unwrap_or("\u{2014}")
                                )))
                                .child(
                                    gpui::div()
                                        .text_sm()
                                        .text_color(theme.foreground)
                                        .child(format!("{}_", self.draft)),
                                )
                                .child(
                                    gpui::div()
                                        .text_xs()
                                        .text_color(muted)
                                        .child("enter posts \u{b7} esc cancels"),
                                ),
                        )
                    }),
            )
            .child(
                v_flex()
                    .id("threads")
                    .flex_1()
                    .min_h_0()
                    .pt_1()
                    .overflow_y_scroll()
                    .scrollbar_width(px(0.0))
                    .track_scroll(&self.thread_scroll)
                    .when(groups.is_empty(), |this| {
                        this.child(
                            gpui::div()
                                .px_3()
                                .py_2()
                                .text_xs()
                                .text_color(muted)
                                .child("no comments on this document yet"),
                        )
                    })
                    .children(groups),
            )
            .into_any_element()
    }

    /// A pane with nothing in it, and the key that gets out.
    fn notice_view(&self, notice: &Notice, theme: &gpui_component::Theme) -> AnyElement {
        let muted = theme.muted_foreground;
        let border = theme.border;
        v_flex()
            .flex_1()
            .min_h_0()
            .items_center()
            .justify_center()
            .px_5()
            .pb_12()
            .child(
                v_flex()
                    .max_w(px(520.0))
                    .gap_2()
                    .child(
                        gpui::div()
                            .w(px(28.0))
                            .h(px(2.0))
                            .rounded_full()
                            .bg(if notice.failed { theme.danger } else { border }),
                    )
                    .child(
                        gpui::div()
                            .text_lg()
                            .font_semibold()
                            .text_color(theme.foreground)
                            .child(notice.title.clone()),
                    )
                    .child(
                        gpui::div()
                            .text_sm()
                            .line_height(px(21.0))
                            .text_color(muted)
                            .child(notice.detail.clone()),
                    )
                    .when_some(notice.why.clone(), |this, why| {
                        this.child(
                            gpui::div()
                                .text_xs()
                                .line_height(px(17.0))
                                .text_color(muted)
                                .child(why),
                        )
                    })
                    .child(
                        // The one thing to do next, in the one colour on the
                        // card that is not secondary.
                        gpui::div()
                            .text_xs()
                            .text_color(theme.foreground)
                            .child(notice.next.clone()),
                    ),
            )
            .into_any_element()
    }

    fn document_view(&self, columns: &Columns, theme: &gpui_component::Theme) -> AnyElement {
        let notice = self.notice();
        let body = match &notice {
            Some(notice) => self.notice_view(notice, theme),
            None => self.blocks_view(theme),
        };
        h_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .children(self.document_header(theme))
                    .child(body),
            )
            // The rail is its own column and its own scroll, so a document stays
            // a document however much is said about it.
            .when(notice.is_none(), |this| {
                this.child(self.threads_view(columns, theme))
            })
            .into_any_element()
    }

    fn toolbar_view(&self, theme: &gpui_component::Theme) -> AnyElement {
        let muted = theme.muted_foreground;
        let title = match &self.pane {
            Pane::Document { name, .. } | Pane::Failed { name, .. } | Pane::NotWord { name } => {
                name.clone()
            }
            Pane::Empty => "no document open".to_owned(),
        };
        h_flex()
            .flex_shrink_0()
            .w_full()
            .items_center()
            .justify_between()
            .gap_3()
            .px_3()
            .py_1p5()
            .border_b_1()
            .border_color(theme.border)
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .min_w_0()
                    .child(gpui::div().text_sm().font_semibold().child("doc-shell"))
                    .child(
                        gpui::div()
                            .text_xs()
                            .min_w_0()
                            .truncate()
                            .text_color(muted)
                            .child(title),
                    ),
            )
            // Symbols and key names, the way the native shell's toolbar reads:
            // the action is named once, in the place the eye already goes.
            .child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap_3()
                    .child(
                        gpui::div()
                            .text_xs()
                            .text_color(muted)
                            .child("c comment · x resolve · ? keys"),
                    )
                    .child(gpui::div().text_xs().text_color(muted).child("·"))
                    .child(
                        gpui::div()
                            .text_xs()
                            .text_color(if self.focus == Focus::Document {
                                theme.foreground
                            } else {
                                muted
                            })
                            .child(match self.focus {
                                Focus::Vault => "focus: vault",
                                Focus::Document => "focus: document",
                            }),
                    ),
            )
            .into_any_element()
    }

    fn status_view(&self, theme: &gpui_component::Theme) -> AnyElement {
        let muted = theme.muted_foreground;
        let left = self
            .note
            .clone()
            .unwrap_or_else(|| match self.entries.get(self.selected) {
                Some(row) => format!(
                    "{} of {} · {}",
                    self.selected + 1,
                    self.entries.len(),
                    row.name
                ),
                None => "\u{2014}".to_owned(),
            });
        let block = match (self.block_id(), self.blocks()) {
            (Some(id), total) => format!("block {} of {total} · {id}", self.block + 1),
            (None, _) => "no block".to_owned(),
        };
        StatusBar::new()
            .left(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(gpui::div().text_xs().child(left))
                    .child(gpui::div().text_xs().child("·"))
                    .child(gpui::div().text_xs().text_color(muted).child(block)),
            )
            .right(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(gpui::div().text_xs().child("d theme"))
                    .child(gpui::div().text_xs().child("q quits")),
            )
            .into_any_element()
    }

    /// Help, or the document read as headings. One panel, one table.
    fn overlay_view(&self, theme: &gpui_component::Theme) -> Option<AnyElement> {
        let muted = theme.muted_foreground;
        let (title, rows): (String, Vec<AnyElement>) = match self.overlay {
            Overlay::None => return None,
            Overlay::Help => (
                "keys".to_owned(),
                [
                    ("tab", "switch between the vault and the document"),
                    ("j k", "move: a row, a block, or a heading"),
                    (
                        "enter",
                        "open a directory · comment on the block · jump to a heading",
                    ),
                    ("c", "comment on the block under the cursor"),
                    ("x", "resolve the block's thread, or reopen it"),
                    ("n p", "next and previous thread"),
                    ("o", "outline: headings, and j k jump between them"),
                    ("/", "filter the vault as you type"),
                    ("r", "re-read the vault and the open document"),
                    ("d", "light or dark"),
                    ("esc", "close what is open"),
                    ("q", "quit"),
                ]
                .iter()
                .map(|(keys, what)| {
                    h_flex()
                        .items_start()
                        .gap_3()
                        .child(
                            gpui::div()
                                .w(px(52.0))
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(theme.foreground)
                                .child(*keys),
                        )
                        .child(
                            gpui::div()
                                .flex_1()
                                .min_w_0()
                                .text_xs()
                                .line_height(px(17.0))
                                .text_color(muted)
                                .child(*what),
                        )
                        .into_any_element()
                })
                .collect(),
            ),
            Overlay::Outline => {
                let headings = self.outline();
                let current = headings
                    .iter()
                    .rev()
                    .find(|(index, _)| *index <= self.block)
                    .map(|(index, _)| *index);
                let rows: Vec<AnyElement> = if headings.is_empty() {
                    vec![
                        gpui::div()
                            .text_xs()
                            .text_color(muted)
                            .child("this document has no headings")
                            .into_any_element(),
                    ]
                } else {
                    headings
                        .iter()
                        .map(|(index, block)| {
                            let level = block.heading().unwrap_or(1);
                            let here = Some(*index) == current;
                            h_flex()
                                .w_full()
                                .items_center()
                                .gap_2()
                                .py_1()
                                .pr_2()
                                .rounded_sm()
                                .pl(px(4.0 + f32::from(level.saturating_sub(1)) * 14.0))
                                .when(here, |this| this.bg(theme.accent))
                                .child(
                                    gpui::div()
                                        .w(px(24.0))
                                        .flex_shrink_0()
                                        .text_xs()
                                        .text_color(muted)
                                        .child(format!("{}", index + 1)),
                                )
                                .child(
                                    gpui::div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .text_xs()
                                        .text_color(if here { theme.foreground } else { muted })
                                        .child(block.text.clone()),
                                )
                                .into_any_element()
                        })
                        .collect()
                };
                (
                    format!("outline · {}", count(headings.len(), "heading", "headings")),
                    rows,
                )
            }
        };

        Some(
            h_flex()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .items_center()
                .justify_center()
                .bg(theme.overlay)
                .child(
                    v_flex()
                        .w(px(560.0))
                        .gap_3()
                        .p_5()
                        .rounded_lg()
                        // A panel has to sit above the scrim in both themes, so
                        // it takes the raised surface rather than the page's.
                        .bg(theme.muted)
                        .border_1()
                        .border_color(theme.ring)
                        .child(
                            h_flex()
                                .items_center()
                                .justify_between()
                                .gap_3()
                                .child(gpui::div().text_sm().font_semibold().child(title))
                                .child(gpui::div().text_xs().text_color(muted).child("esc closes")),
                        )
                        .children(rows)
                        .child(
                            gpui::div()
                                .text_xs()
                                .text_color(muted)
                                .child("nothing here needs a click: this window has no pointer"),
                        ),
                )
                .into_any_element(),
        )
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let columns = Columns::of(f32::from(window.viewport_size().width));

        // Keeping the cursor in view is the only scrolling this window does:
        // there is no pointer and no scrollbar to drag. It is asked here, every
        // frame, because it is a no-op when the cursor is already visible and
        // because the request has to be in hand before this frame is laid out
        // (the very first one has no scroll state to apply it to).
        self.vault_scroll.scroll_to_item(self.selected);
        self.document_scroll.scroll_to_item(self.block);
        if let Some(group) = self.thread_group(self.block) {
            self.thread_scroll.scroll_to_item(group);
        }

        v_flex()
            .relative()
            .size_full()
            .bg(theme.background)
            .text_sm()
            .child(self.toolbar_view(theme))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.vault_view(&columns, theme))
                    .child(self.document_view(&columns, theme)),
            )
            .child(self.status_view(theme))
            .children(self.overlay_view(theme))
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
                let (changed, quit) = window.update(&mut cx, |shell, _, cx| {
                    // `d` and `q` belong to the window, but only when nothing is
                    // being typed: a comment may well contain either letter.
                    if !shell.typing() {
                        match key.key.as_str() {
                            "q" => return (false, true),
                            // The theme is app state, not view state, so the
                            // toggle happens here where the App is in hand.
                            "d" => {
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
                                return (true, false);
                            }
                            _ => {}
                        }
                    }
                    let changed = shell.handle_key(&key.key, key.text.as_deref());
                    if changed {
                        cx.notify();
                    }
                    (changed, false)
                })?;
                dirty |= changed;
                if quit {
                    break;
                }
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
