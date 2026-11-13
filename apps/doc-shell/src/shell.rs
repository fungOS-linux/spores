//! The window: three panes, one ordering, and the state a pane needs to draw.
//!
//! Every pane is rendered from the same function whether it is first, middle or
//! last: a pane does not know its position, and the layout is the only thing that
//! does. That is what makes flipping one action instead of two layouts.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use anyhow::Result;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _, StyledExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputEvent, Textarea, TextareaState},
    resizable::{ResizableState, h_resizable, resizable_panel},
    status_bar::StatusBar,
    text::TextView,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::{
    doclayer::{Store, ThreadView, TurnView},
    layout::{Layout, Pane},
    open_doc::{OpenDocument, View},
    persist,
    transcript::{SessionChoice, TranscriptSource, TranscriptView, VfsTranscripts},
    vault::{self, Document, Group},
};

actions!(
    doc_shell,
    [
        FlipPanes,
        MoveSidebar,
        ToggleView,
        SaveDocument,
        NextSession,
        PrevSession,
        Reload
    ]
);

/// What the document pane is showing.
enum Open {
    /// A markdown file, bound to its document.
    Modelled(Box<OpenDocument>),
    /// A file the document layer does not model. Named, not faked.
    Unmodelled { rel: String, ext: String },
}

/// Why the pane has no document, when that is the state.
const NO_DOCUMENT: &str = "No document open";

pub struct Shell {
    /// The vault's directory on disk and its records under `.doc-shell/`.
    root: PathBuf,
    title: String,
    store: Store,
    layouts: persist::Store,
    layout: Layout,
    groups: Vec<Group>,
    collapsed: BTreeSet<String>,
    open: Option<Open>,
    editor: Option<Entity<TextareaState>>,
    transcripts: Box<dyn TranscriptSource>,
    choices: Vec<SessionChoice>,
    session: usize,
    transcript: Option<Result<TranscriptView, String>>,
    /// Turns whose thinking, tools or prompt are shown rather than collapsed.
    expanded: BTreeSet<u32>,
    expanded_tools: BTreeSet<(u32, usize)>,
    /// The last thing an action did, shown in the status bar.
    status: String,
}

/// How the window should come up, from the command line.
#[derive(Clone, Copy, Debug)]
pub struct Start<'a> {
    /// A path inside the vault to open.
    pub open: Option<&'a str>,
    /// Start with the document and the transcript swapped.
    pub flipped: bool,
    /// Which view a markdown document opens in.
    pub view: View,
}

impl Shell {
    pub fn new(
        root: &Path,
        title: String,
        actor: &str,
        start: Start<'_>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let store = match Store::open(root, actor) {
            Ok(store) => store,
            Err(error) => {
                // A vault whose records cannot be opened is not a vault: say so
                // and exit rather than render three empty panes that look like an
                // empty vault.
                eprintln!("doc-shell: {error:#}");
                std::process::exit(1);
            }
        };
        let layouts = persist::Store::load();
        let mut shell = Self {
            root: root.to_path_buf(),
            title,
            transcripts: Box::new(VfsTranscripts::new(store.clone())),
            store,
            layout: layouts.layout(None),
            layouts,
            groups: Vec::new(),
            collapsed: BTreeSet::new(),
            open: None,
            editor: None,
            choices: Vec::new(),
            session: 0,
            transcript: None,
            expanded: BTreeSet::new(),
            expanded_tools: BTreeSet::new(),
            status: String::new(),
        };
        shell.status = match shell.layouts.note() {
            Some(note) => note.to_owned(),
            None => "ready".to_owned(),
        };
        shell.rescan();
        shell.load_transcripts();

        if start.flipped {
            shell.layout.flip();
        }
        if let Some(rel) = start.open {
            shell.open_by_rel(rel, window, cx);
        }
        if let Some(Open::Modelled(open)) = shell.open.as_mut() {
            open.view = start.view;
        }
        shell
    }

    /// Re-read the tree. Nothing about the layout depends on it, so a vault that
    /// changed under us re-lists without moving a divider.
    fn rescan(&mut self) {
        self.groups = vault::scan(&self.root);
        self.choices = self.transcripts.sessions();
        if self.session >= self.choices.len() {
            self.session = self.choices.len().saturating_sub(1);
        }
        self.pick_session();
    }

    fn load_transcripts(&mut self) {
        self.choices = self.transcripts.sessions();
        self.session = self.choices.len().saturating_sub(1);
        self.pick_session();
    }

    fn pick_session(&mut self) {
        self.transcript = self.choices.get(self.session).map(|choice| {
            self.transcripts
                .transcript(choice)
                .inspect_err(|error| eprintln!("doc-shell: transcript: {error}"))
        });
    }

    fn note(&mut self, message: impl Into<String>) {
        self.status = message.into();
    }

    /// Open a document by its path inside the vault.
    fn open_by_rel(&mut self, rel: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(document) = self
            .groups
            .iter()
            .flat_map(|group| group.documents.iter())
            .find(|document| document.rel == rel)
            .cloned()
        else {
            self.note(format!("{rel} is not in this vault"));
            return;
        };
        self.open_document(&document, window, cx);
    }

    fn open_document(&mut self, document: &Document, window: &mut Window, cx: &mut Context<Self>) {
        if !document.modelled {
            let ext = document
                .path
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("unknown")
                .to_owned();
            self.open = Some(Open::Unmodelled {
                rel: document.rel.clone(),
                ext,
            });
            self.editor = None;
            self.layout = self.layouts.layout(Some(&document.id));
            self.note(format!("{}: not modelled yet", document.rel));
            cx.notify();
            return;
        }

        match OpenDocument::open(&self.store, &document.id, &document.rel, &document.path) {
            Ok(open) => {
                let buffer = open.buffer.clone();
                self.editor = Some(cx.new(|cx| {
                    TextareaState::new(window, cx)
                        .default_value(buffer)
                        .auto_grow(24, 400)
                }));
                // A keypress re-draws the toolbar so the unsaved marker is
                // honest. Nothing else listens to the editor: the buffer is read
                // when a save is asked for, so a view cannot push a half-typed
                // thought into the document.
                if let Some(editor) = self.editor.clone() {
                    cx.subscribe(&editor, |_, _, event: &InputEvent, cx| {
                        if matches!(event, InputEvent::Change) {
                            cx.notify();
                        }
                    })
                    .detach();
                }
                let report = open
                    .last
                    .as_ref()
                    .map(|report| report.line())
                    .unwrap_or_default();
                self.layout = self.layouts.layout(Some(&document.id));
                self.note(report);
                self.open = Some(Open::Modelled(Box::new(open)));
            }
            Err(error) => {
                self.note(format!("{}: {error}", document.rel));
                self.open = None;
                self.editor = None;
            }
        }
        cx.notify();
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.clone() else {
            return;
        };
        let Some(Open::Modelled(open)) = self.open.as_mut() else {
            return;
        };
        open.buffer = editor.read(cx).value().to_string();
        match open.save(&self.store) {
            Ok(report) => self.note(report.line()),
            Err(error) => self.note(format!("save failed: {error:#}")),
        }
        // The merged text stands: put it back in the view so it is not showing an
        // edit that lost the merge.
        if let Some(Open::Modelled(open)) = self.open.as_ref() {
            let merged = open.buffer.clone();
            editor.update(cx, |state, cx| state.set_value(merged, window, cx));
        }
        cx.notify();
    }

    /// Remember the arrangement for whatever is open. A failure is shown, not
    /// swallowed: a layout that silently does not persist is a bug report nobody
    /// files.
    fn remember_layout(&mut self) {
        let document = match self.open.as_ref() {
            Some(Open::Modelled(open)) => Some(open.id.clone()),
            Some(Open::Unmodelled { rel, .. }) => Some(vault::document_id(rel)),
            None => None,
        };
        if let Err(error) = self.layouts.remember(document.as_deref(), &self.layout) {
            self.note(format!("layout not saved: {error}"));
        }
    }

    /// Whether the source view holds text the document does not.
    ///
    /// Derived on every frame rather than kept as a flag: a flag can be set and
    /// forgotten, and the one thing an unsaved marker must never do is lie about
    /// which side of the merge the text is on.
    fn unsaved(&self, cx: &Context<Self>) -> bool {
        let Some(editor) = self.editor.as_ref() else {
            return false;
        };
        let Some(Open::Modelled(open)) = self.open.as_ref() else {
            return false;
        };
        editor.read(cx).value().as_ref() != open.buffer
    }

    fn flip(&mut self, cx: &mut Context<Self>) {
        self.layout.flip();
        self.remember_layout();
        let order: Vec<&str> = self.layout.panes().iter().map(|pane| pane.key()).collect();
        self.status = format!("panes: {}", order.join(" | "));
        cx.notify();
    }

    fn move_sidebar(&mut self, cx: &mut Context<Self>) {
        self.layout.move_sidebar();
        self.remember_layout();
        self.status = if self.layout.sidebar_at_end() {
            "sidebar on the right".to_owned()
        } else {
            "sidebar on the left".to_owned()
        };
        cx.notify();
    }

    fn toggle_view(&mut self, cx: &mut Context<Self>) {
        if let Some(Open::Modelled(open)) = self.open.as_mut() {
            open.view = open.view.toggled();
            let view = open.view.label();
            self.note(format!("view: {view}"));
            cx.notify();
        }
    }

    fn step_session(&mut self, step: i64, cx: &mut Context<Self>) {
        if self.choices.is_empty() {
            return;
        }
        let last = self.choices.len() - 1;
        let next = (self.session as i64 + step).clamp(0, last as i64) as usize;
        if next == self.session {
            return;
        }
        self.session = next;
        self.pick_session();
        self.expanded.clear();
        self.expanded_tools.clear();
        let label = self.choices[self.session].label();
        self.note(format!("watching {label}"));
        cx.notify();
    }

    // -- rendering -------------------------------------------------------

    fn toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let flipped = self.layout.panes().first() == Some(&Pane::Transcript);
        let sidebar_right = self.layout.sidebar_at_end();
        let view = match self.open.as_ref() {
            Some(Open::Modelled(open)) => Some(open.view),
            _ => None,
        };
        let dirty = self.unsaved(cx);

        h_flex()
            .flex_shrink_0()
            .w_full()
            .items_center()
            .justify_between()
            .gap_3()
            .px_3()
            .py_1p5()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(div().text_sm().font_semibold().child("doc-shell"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(self.title.clone()),
                    ),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_1()
                    .child(
                        Button::new("flip")
                            .label(if flipped {
                                "⇄ panes: agent | document"
                            } else {
                                "⇄ panes: document | agent"
                            })
                            .small()
                            .tooltip("Swap the document and the transcript (cmd-shift-p)")
                            .on_click(cx.listener(|this, _, _, cx| this.flip(cx))),
                    )
                    .child(
                        Button::new("sidebar-side")
                            .label(if sidebar_right {
                                "sidebar ⇠"
                            } else {
                                "sidebar ⇢"
                            })
                            .small()
                            .tooltip("Move the vault to the other end (cmd-shift-b)")
                            .on_click(cx.listener(|this, _, _, cx| this.move_sidebar(cx))),
                    )
                    .when(view.is_some(), |this| {
                        this.child(
                            Button::new("view")
                                .label(format!("view: {}", view.map_or("", View::label)))
                                .small()
                                .tooltip("Markdown source or rendered preview (cmd-shift-e)")
                                .on_click(cx.listener(|this, _, _, cx| this.toggle_view(cx))),
                        )
                        .child(
                            Button::new("save")
                                .label(if dirty { "save ●" } else { "save" })
                                .small()
                                .disabled(!dirty)
                                .tooltip("Write the buffer and reconcile it (cmd-s)")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.save(window, cx);
                                })),
                        )
                    }),
            )
    }

    fn status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let document = match self.open.as_ref() {
            Some(Open::Modelled(open)) => format!("{} · {} blocks", open.id, open.blocks()),
            Some(Open::Unmodelled { rel, .. }) => format!("{rel} · not modelled"),
            None => NO_DOCUMENT.to_lowercase(),
        };
        StatusBar::new()
            .left(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .text_xs()
                            .child(format!("root {}", self.root.display())),
                    )
                    .child(div().text_xs().child("·"))
                    .child(
                        div()
                            .text_xs()
                            .child(format!("actor {}", self.store.actor())),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(document),
            )
            .right(
                h_flex().gap_2().items_center().child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(self.status.clone()),
                ),
            )
    }

    fn panes(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let order = self.layout.panes().to_vec();
        // A distinct id per arrangement gives each one its own resizable state:
        // dragging a divider before a flip must not decide the widths after it.
        let mut group = h_resizable(format!("panes-{}", self.layout.key())).on_resize(cx.listener(
            |this, state: &Entity<ResizableState>, _, cx| {
                let sizes: Vec<f32> = state
                    .read(cx)
                    .sizes()
                    .iter()
                    .map(|pixels| f32::from(*pixels))
                    .collect();
                let panes = this.layout.panes().to_vec();
                let mut layout = this.layout.clone();
                for (index, pane) in panes.iter().enumerate() {
                    if let Some(width) = sizes.get(index) {
                        layout.set_width(*pane, *width);
                    }
                }
                if layout != this.layout {
                    this.layout = layout;
                    this.remember_layout();
                    cx.notify();
                }
            },
        ));
        let count = order.len();
        for (index, pane) in order.into_iter().enumerate() {
            let mut panel = resizable_panel();
            if let Some(width) = self.layout.width(pane) {
                panel = panel.size(px(width));
            }
            let edge = PaneEdge {
                first: index == 0,
                last: index == count - 1,
            };
            panel = panel.child(self.pane(pane, edge, window, cx));
            group = group.child(panel);
        }
        group
    }

    fn pane(
        &mut self,
        pane: Pane,
        edge: PaneEdge,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let mut element = match pane {
            Pane::Sidebar => self.sidebar(cx).into_any_element(),
            Pane::Document => self.document(window, cx).into_any_element(),
            Pane::Transcript => self.transcript_pane(cx).into_any_element(),
        };
        // A hairline belongs between panes, on whichever side the neighbour is.
        element = element.into_any_element();
        let _ = edge;
        element
    }

    fn sidebar(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let total: usize = self.groups.iter().map(|group| group.documents.len()).sum();
        let selected = match self.open.as_ref() {
            Some(Open::Modelled(open)) => Some(open.rel.clone()),
            Some(Open::Unmodelled { rel, .. }) => Some(rel.clone()),
            None => None,
        };
        let groups = self.groups.clone();
        let collapsed = self.collapsed.clone();

        v_flex()
            .size_full()
            .min_w_0()
            .bg(cx.theme().sidebar)
            .child(
                v_flex()
                    .flex_shrink_0()
                    .gap_0p5()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(div().text_sm().font_semibold().child(self.title.clone()))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{total} files · grouped by directory")),
                    ),
            )
            .child(
                v_flex()
                    .id("vault-tree")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .py_1()
                    .children(groups.into_iter().map(|group| {
                        let open = !collapsed.contains(&group.dir);
                        let dir = group.dir.clone();
                        v_flex()
                            .child(
                                h_flex()
                                    .id(SharedString::from(format!("group:{}", group.dir)))
                                    .items_center()
                                    .gap_1()
                                    .px_2()
                                    .py_1()
                                    .cursor_pointer()
                                    .hover(|style| style.bg(cx.theme().accent))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(if open { "▾" } else { "▸" }),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_semibold()
                                            .child(group.title().to_owned()),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(format!("{}", group.documents.len())),
                                    )
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if !this.collapsed.remove(&dir) {
                                            this.collapsed.insert(dir.clone());
                                        }
                                        cx.notify();
                                    })),
                            )
                            .when(open, |this| {
                                this.children(group.documents.into_iter().map(|document| {
                                    let is_selected =
                                        selected.as_deref() == Some(document.rel.as_str());
                                    let row_document = document.clone();
                                    h_flex()
                                        .id(SharedString::from(format!("doc:{}", document.id)))
                                        .items_center()
                                        .gap_1p5()
                                        .pl_5()
                                        .pr_2()
                                        .py_1()
                                        .cursor_pointer()
                                        .when(is_selected, |this| {
                                            this.bg(cx.theme().accent)
                                                .text_color(cx.theme().accent_foreground)
                                        })
                                        .when(!is_selected, |this| {
                                            this.hover(|style| style.bg(cx.theme().muted))
                                        })
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(if document.modelled { "¶" } else { "·" }),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .when(!document.modelled, |this| {
                                                    this.text_color(cx.theme().muted_foreground)
                                                })
                                                .child(document.name().to_owned()),
                                        )
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.open_document(&row_document, window, cx);
                                        }))
                                }))
                            })
                    })),
            )
    }

    fn document(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let header = self.document_header(cx);
        let threads = self.threads_section(cx);
        let footer = self.document_footer(cx);
        let (view, opened) = match self.open.as_ref() {
            Some(Open::Modelled(open)) => (Some(open.view), Some(open)),
            _ => (None, None),
        };
        let body: AnyElement = match self.open.as_ref() {
            None => empty_state(
                "No document open",
                "Pick a file in the vault. A markdown file is a document; anything else is a file this layer does not model yet.",
                cx,
            ),
            Some(Open::Unmodelled { rel, ext }) => empty_state(
                &format!("{rel} is not modelled yet"),
                &format!(
                    "The document layer has views for markdown. A .{ext} file has no blocks to show, and a stub of one would look like content. Converting it to markdown would flatten it, so this build does neither."
                ),
                cx,
            ),
            Some(Open::Modelled(_)) => match (view, opened) {
                (Some(View::Source), _) => self.source_view(cx),
                (Some(View::Preview), Some(open)) => preview_view(open, cx),
                _ => empty_state(NO_DOCUMENT, "nothing to show", cx),
            },
        };

        v_flex()
            .size_full()
            .min_w_0()
            .bg(cx.theme().background)
            .child(header)
            .child(div().flex_1().min_h_0().overflow_hidden().child(body))
            .child(threads)
            .child(footer)
            .into_any_element()
    }

    fn document_header(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let (title, detail) = match self.open.as_ref() {
            Some(Open::Modelled(open)) => (
                open.rel.clone(),
                format!("markdown · id {} · {}", open.id, open.view.label()),
            ),
            Some(Open::Unmodelled { rel, ext }) => {
                (rel.clone(), format!(".{ext} · not modelled yet"))
            }
            None => (NO_DOCUMENT.to_owned(), "nothing to show".to_owned()),
        };
        h_flex()
            .flex_shrink_0()
            .items_center()
            .justify_between()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                v_flex()
                    .gap_0p5()
                    .child(div().text_sm().font_semibold().child(title))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(detail),
                    ),
            )
            .when(self.unsaved(cx), |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().warning)
                        .child("unsaved"),
                )
            })
    }

    fn source_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(editor) = self.editor.clone() else {
            return empty_state("No editor", "The document has no bound text view.", cx)
                .into_any_element();
        };
        v_flex()
            .size_full()
            .p_2()
            .child(
                div()
                    .id("source-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(Textarea::new(&editor).h_full()),
            )
            .into_any_element()
    }

    fn threads_section(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let (threads, error) = match self.open.as_ref() {
            Some(Open::Modelled(open)) => (open.threads.clone(), open.threads_error.clone()),
            _ => (Vec::new(), None),
        };
        let visible = !threads.is_empty() || error.is_some();
        v_flex().flex_shrink_0().when(visible, |this| {
            this.max_h(px(240.0))
                .border_t_1()
                .border_color(cx.theme().border)
                .child(
                    h_flex()
                        .items_center()
                        .justify_between()
                        .px_3()
                        .py_1p5()
                        .child(
                            div()
                                .text_xs()
                                .font_semibold()
                                .child(format!("threads · {}", threads.len())),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("anchors, not positions · read-only this milestone"),
                        ),
                )
                .child(
                    v_flex()
                        .id("threads")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .gap_1()
                        .px_3()
                        .pb_2()
                        .when_some(error, |this, error| {
                            this.child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().danger)
                                    .child(format!("threads not read: {error}")),
                            )
                        })
                        .children(threads.into_iter().map(|thread| thread_card(thread, cx))),
                )
        })
    }

    fn document_footer(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let line = match self.open.as_ref() {
            Some(Open::Modelled(open)) => open
                .last
                .as_ref()
                .map(|report| report.line())
                .unwrap_or_else(|| "no reconcile yet".to_owned()),
            Some(Open::Unmodelled { .. }) => "nothing was reconciled".to_owned(),
            None => "block-level merge · last writer wins per block".to_owned(),
        };
        h_flex()
            .flex_shrink_0()
            .items_center()
            .justify_between()
            .px_3()
            .py_1()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        "block-level merge · last writer wins per block · no character CRDT yet",
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(line),
            )
    }

    fn transcript_pane(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let header = self.transcript_header(cx);
        let body: AnyElement = match self.transcript.clone() {
            None => empty_state(
                "No session records",
                "This pane is a client of the document layer's session records. Import one with `fungos-vfs session import --harness pi --path <pi store>` and it appears here; a live ACP stream is the next source behind the same trait.",
                cx,
            ),
            Some(Err(error)) => empty_state("Transcript not read", &error, cx),
            Some(Ok(view)) => self.turns(view, cx).into_any_element(),
        };
        v_flex()
            .size_full()
            .min_w_0()
            .bg(cx.theme().background)
            .child(header)
            .child(div().flex_1().min_h_0().child(body))
            .into_any_element()
    }

    fn transcript_header(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let (title, subtitle) = match &self.transcript {
            Some(Ok(view)) => (view.title.clone(), view.subtitle.clone()),
            Some(Err(error)) => ("not read".to_owned(), error.clone()),
            None => (
                "nothing to watch".to_owned(),
                "no session records in this root".to_owned(),
            ),
        };
        let position = if self.choices.is_empty() {
            String::new()
        } else {
            format!("{} / {}", self.session + 1, self.choices.len())
        };
        v_flex()
            .flex_shrink_0()
            .gap_1()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(div().text_sm().font_semibold().child("agent"))
                    .child(
                        h_flex()
                            .items_center()
                            .gap_1()
                            .child(
                                Button::new("prev-session")
                                    .label("‹")
                                    .small()
                                    .disabled(self.choices.is_empty() || self.session == 0)
                                    .tooltip("Previous session")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.step_session(-1, cx);
                                    })),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(position),
                            )
                            .child(
                                Button::new("next-session")
                                    .label("›")
                                    .small()
                                    .disabled(
                                        self.choices.is_empty()
                                            || self.session + 1 >= self.choices.len(),
                                    )
                                    .tooltip("Next session")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.step_session(1, cx);
                                    })),
                            ),
                    ),
            )
            .child(div().text_xs().font_semibold().child(title))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(subtitle),
            )
    }

    fn turns(&mut self, view: TranscriptView, cx: &mut Context<Self>) -> AnyElement {
        let unnormalized = view.unnormalized.clone();
        let cards: Vec<AnyElement> = view
            .turns
            .iter()
            .map(|turn| turn_card(turn, &self.expanded, &self.expanded_tools, cx))
            .collect();
        v_flex()
            .id("transcript")
            .size_full()
            .overflow_y_scroll()
            .gap_1()
            .p_2()
            .children(cards)
            .when(!unnormalized.is_empty(), |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!(
                            "not normalized: {}",
                            unnormalized
                                .iter()
                                .map(|(reason, count)| format!("{reason} ×{count}"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )),
                )
            })
            .into_any_element()
    }
}

/// One turn. Thinking, tool arguments and results are collapsed until asked for:
/// this pane is for watching an agent work, and most of a turn is noise.
fn turn_card(
    turn: &TurnView,
    expanded: &BTreeSet<u32>,
    expanded_tools: &BTreeSet<(u32, usize)>,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let index = turn.index;
    let open = expanded.contains(&index);
    let quiet = turn.role == "system";
    let role_color = match turn.role {
        "user" => cx.theme().primary,
        "assistant" => cx.theme().foreground,
        "tool" => cx.theme().warning,
        _ => cx.theme().muted_foreground,
    };
    v_flex()
        .id(SharedString::from(format!("turn:{index}")))
        .gap_1()
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(cx.theme().border)
        .p_2()
        .child(
            h_flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    h_flex()
                        .items_center()
                        .gap_1p5()
                        .child(
                            div()
                                .text_xs()
                                .font_semibold()
                                .text_color(role_color)
                                .child(turn.role.to_owned()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(format!("#{index}")),
                        )
                        .when_some(turn.model.clone(), |this, model| {
                            this.child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(model),
                            )
                        }),
                )
                .child(
                    h_flex()
                        .items_center()
                        .gap_1()
                        .when(quiet || turn.text.chars().count() > 400, |this| {
                            this.child(
                                Button::new(SharedString::from(format!("expand:{index}")))
                                    .label(if open { "collapse" } else { "expand" })
                                    .small()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if !this.expanded.remove(&index) {
                                            this.expanded.insert(index);
                                        }
                                        cx.notify();
                                    })),
                            )
                        })
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(crate::transcript::when(turn.at_ms)),
                        ),
                ),
        )
        .when(!quiet || open, |this| {
            this.child(div().text_sm().child(TextView::markdown(
                SharedString::from(format!("turn-text:{index}")),
                turn.text.clone(),
            )))
        })
        .when(!open && quiet, |this| {
            this.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(truncate(&turn.text, 140)),
            )
        })
        .when_some(turn.thinking.clone(), |this, thinking| {
            this.child(
                v_flex()
                    .gap_1()
                    .child(
                        Button::new(SharedString::from(format!("thinking:{index}")))
                            .label(if open { "▾ thinking" } else { "▸ thinking" })
                            .small()
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.expanded.remove(&index) {
                                    this.expanded.insert(index);
                                }
                                cx.notify();
                            })),
                    )
                    .when(open, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .italic()
                                .text_color(cx.theme().muted_foreground)
                                .child(truncate(&thinking, 2000)),
                        )
                    }),
            )
        })
        .when(!turn.tools.is_empty(), |this| {
            this.child(
                v_flex().gap_1().children(
                    turn.tools
                        .iter()
                        .enumerate()
                        .map(|(slot, tool)| {
                            let key = (index, slot);
                            let show = expanded_tools.contains(&key);
                            v_flex()
                                .gap_1()
                                .child(
                                    Button::new(SharedString::from(format!("tool:{index}:{slot}")))
                                        .label(format!(
                                            "{} {}{}",
                                            if show { "▾" } else { "▸" },
                                            tool.name,
                                            if tool.failed { " · failed" } else { "" }
                                        ))
                                        .small()
                                        .ghost()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if !this.expanded_tools.remove(&key) {
                                                this.expanded_tools.insert(key);
                                            }
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(truncate(
                                            &tool.arguments,
                                            if show { 600 } else { 90 },
                                        )),
                                )
                                .when(show, |this| {
                                    this.when_some(tool.result.clone(), |this, result| {
                                        this.child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(truncate(&result, 1200)),
                                        )
                                    })
                                })
                                .into_any_element()
                        })
                        .collect::<Vec<_>>(),
                ),
            )
        })
        .into_any_element()
}

struct PaneEdge {
    #[allow(dead_code)]
    first: bool,
    #[allow(dead_code)]
    last: bool,
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.toolbar(cx))
            .child(div().flex_1().min_h_0().child(self.panes(window, cx)))
            .child(self.status_bar(cx))
            .on_action(cx.listener(|this, _: &FlipPanes, _, cx| this.flip(cx)))
            .on_action(cx.listener(|this, _: &MoveSidebar, _, cx| this.move_sidebar(cx)))
            .on_action(cx.listener(|this, _: &ToggleView, _, cx| this.toggle_view(cx)))
            .on_action(cx.listener(|this, _: &SaveDocument, window, cx| this.save(window, cx)))
            .on_action(cx.listener(|this, _: &NextSession, _, cx| this.step_session(1, cx)))
            .on_action(cx.listener(|this, _: &PrevSession, _, cx| this.step_session(-1, cx)))
            .on_action(cx.listener(|this, _: &Reload, _, cx| {
                this.rescan();
                cx.notify();
            }))
    }
}

fn preview_view(open: &OpenDocument, _cx: &mut Context<Shell>) -> AnyElement {
    v_flex()
        .id("preview")
        .size_full()
        .overflow_y_scroll()
        .p_4()
        .child(div().max_w_full().child(TextView::markdown(
            SharedString::from(format!("preview:{}", open.id)),
            open.rendered(),
        )))
        .into_any_element()
}

fn empty_state(title: &str, detail: &str, cx: &mut Context<Shell>) -> AnyElement {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_2()
        .p_8()
        .child(div().text_sm().font_semibold().child(title.to_owned()))
        .child(
            div()
                .text_xs()
                .max_w(px(520.0))
                .text_center()
                .text_color(cx.theme().muted_foreground)
                .child(detail.to_owned()),
        )
        .into_any_element()
}

fn thread_card(thread: ThreadView, cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    let state_color = match thread.state {
        "exact" => cx.theme().success,
        "clamped" => cx.theme().warning,
        "neighbour" => cx.theme().warning,
        _ => cx.theme().danger,
    };
    let resolution_note = match thread.state {
        "exact" => "the block is here, unchanged",
        "clamped" => "the words moved inside the block: this range no longer matches",
        "neighbour" => "the block is gone; this is the live block that took its place",
        _ => "nothing left to point at",
    };
    v_flex()
        .gap_1()
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(cx.theme().border)
        .p_2()
        .child(
            h_flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(thread.anchor.clone()),
                )
                .child(
                    h_flex()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .font_semibold()
                                .text_color(state_color)
                                .child(thread.state.to_owned()),
                        )
                        .when(thread.resolved, |this| {
                            this.child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("· resolved"),
                            )
                        }),
                ),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(resolution_note.to_owned()),
        )
        .child(
            div()
                .text_xs()
                .italic()
                .when(thread.tombstoned, |this| {
                    this.text_color(cx.theme().muted_foreground)
                })
                .child(if thread.block.is_empty() {
                    "(the anchored block has no text here)".to_owned()
                } else {
                    format!("“{}”", truncate(&thread.block, 160))
                }),
        )
        .children(thread.comments.into_iter().map(move |comment| {
            h_flex()
                .gap_1()
                .items_start()
                .child(
                    div()
                        .text_xs()
                        .font_semibold()
                        .text_color(cx.theme().muted_foreground)
                        .child(comment.author),
                )
                .child(div().text_xs().child(comment.body))
        }))
        .into_any_element()
}

/// A short excerpt, cut on a character boundary so a multi-byte character cannot
/// split into an invalid one.
fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(limit).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::truncate;

    #[test]
    fn an_excerpt_is_cut_on_a_character_boundary() {
        assert_eq!(truncate("abc", 5), "abc");
        assert_eq!(truncate("abcdef", 3), "abc…");
        assert_eq!(truncate("héllo wörld", 4), "héll…");
    }
}
