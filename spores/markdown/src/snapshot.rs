//! The data the Lua UI renders.
//!
//! This is the whole seam. Rust owns document semantics and editor state; Lua
//! owns every presentation decision. Nothing here names a colour, font, size,
//! coordinate, or widget. The snapshot is flat and string-keyed so it maps to a
//! Lua table without a bespoke binding generator:
//!
//! ```lua
//! {
//!   revision = 7, mode = "edit", dirty = true, line_count = 12,
//!   cursor = { line = 3, column = 4 },
//!   selection = { { line = 3, column = 0 }, { line = 3, column = 4 } } or nil,
//!   scroll = 0,
//!   blocks = {
//!     { index = 0, line = 0, kind = "heading", level = 1,
//!       text = "Field notes", spans = { { style = "text", text = "Field notes" } } },
//!     -- kind: heading | paragraph | quote | list_item | code | diagram | rule | spacer
//!     -- style: text | code | strong | emphasis | strong_emphasis | link | image
//!   },
//!   assets = { { kind = "image", target = "asset:sha256:...", line = 8, label = "map" } },
//! }
//! ```

use crate::content::{self, Request};
use crate::edit::{Editor, Mode, Position};
use crate::markdown::{self, Block, Inline};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub revision: u64,
    pub mode: Mode,
    pub cursor: Position,
    pub selection: Option<(Position, Position)>,
    pub scroll: usize,
    pub dirty: bool,
    pub line_count: usize,
    pub blocks: Vec<BlockData>,
    /// Everything the host must resolve before the UI can draw.
    pub assets: Vec<Request>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockData {
    pub index: usize,
    /// Zero-based source line.
    pub line: usize,
    /// One of `heading`, `paragraph`, `quote`, `list_item`, `code`, `diagram`,
    /// `rule`, `spacer`.
    pub kind: &'static str,
    /// Heading level, 0 when not a heading.
    pub level: u8,
    /// List ordering, false when not a list item.
    pub ordered: bool,
    /// Position within the list, 0 when not a list item.
    pub item: usize,
    /// Fence language for `code`, renderer name for `diagram`.
    pub language: Option<String>,
    /// Plain-text projection.
    pub text: String,
    pub spans: Vec<SpanData>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpanData {
    /// One of `text`, `code`, `strong`, `emphasis`, `strong_emphasis`, `link`,
    /// `image`.
    pub style: &'static str,
    pub text: String,
    /// Present for `link` and `image`.
    pub target: Option<String>,
}

/// Project the editor into the Lua-facing snapshot.
///
/// Takes no surface argument on purpose: a Spore that can tell which host is
/// drawing it will eventually behave differently on one of them.
pub fn project(editor: &Editor) -> Snapshot {
    let located = markdown::parse_blocks(editor.source());
    let assets = content::requests(&located);
    let mut blocks = Vec::with_capacity(located.len());

    for (index, entry) in located.iter().enumerate() {
        let line = entry.line;
        match &entry.block {
            Block::Heading { level, text } => {
                let (plain, spans) = flatten(text);
                blocks.push(BlockData {
                    index,
                    line,
                    kind: "heading",
                    level: *level,
                    ordered: false,
                    item: 0,
                    language: None,
                    text: plain,
                    spans,
                });
            }
            Block::Paragraph { text } => {
                let (plain, spans) = flatten(text);
                blocks.push(BlockData {
                    index,
                    line,
                    kind: "paragraph",
                    level: 0,
                    ordered: false,
                    item: 0,
                    language: None,
                    text: plain,
                    spans,
                });
            }
            Block::Quote { text } => {
                let (plain, spans) = flatten(text);
                blocks.push(BlockData {
                    index,
                    line,
                    kind: "quote",
                    level: 0,
                    ordered: false,
                    item: 0,
                    language: None,
                    text: plain,
                    spans,
                });
            }
            Block::List { ordered, items } => {
                for (item_index, item) in items.iter().enumerate() {
                    let (plain, spans) = flatten(item);
                    blocks.push(BlockData {
                        index: blocks.len(),
                        line,
                        kind: "list_item",
                        level: 0,
                        ordered: *ordered,
                        item: item_index,
                        language: None,
                        text: plain,
                        spans,
                    });
                }
            }
            Block::Fence { info, text } => match content::classify_fence(info) {
                content::FenceKind::Code => blocks.push(BlockData {
                    index,
                    line,
                    kind: "code",
                    level: 0,
                    ordered: false,
                    item: 0,
                    language: info.split_whitespace().next().map(str::to_owned),
                    text: text.clone(),
                    spans: vec![SpanData {
                        style: "code",
                        text: text.clone(),
                        target: None,
                    }],
                }),
                content::FenceKind::HostRendered(format) => blocks.push(BlockData {
                    index,
                    line,
                    kind: "diagram",
                    level: 0,
                    ordered: false,
                    item: 0,
                    language: Some(format.as_str().to_owned()),
                    text: text.clone(),
                    spans: vec![SpanData {
                        style: "code",
                        text: text.clone(),
                        target: None,
                    }],
                }),
            },
            Block::Rule => blocks.push(BlockData {
                index,
                line,
                kind: "rule",
                level: 0,
                ordered: false,
                item: 0,
                language: None,
                text: String::new(),
                spans: Vec::new(),
            }),
            Block::Blank => blocks.push(BlockData {
                index,
                line,
                kind: "spacer",
                level: 0,
                ordered: false,
                item: 0,
                language: None,
                text: String::new(),
                spans: Vec::new(),
            }),
        }
    }

    Snapshot {
        revision: editor.revision(),
        mode: editor.mode(),
        cursor: editor.cursor(),
        selection: editor.selection(),
        scroll: editor.scroll(),
        dirty: editor.is_dirty(),
        line_count: editor.line_count(),
        blocks,
        assets,
    }
}

fn flatten(items: &[Inline]) -> (String, Vec<SpanData>) {
    let mut plain = String::new();
    let mut spans = Vec::new();
    walk(items, Style::Text, &mut plain, &mut spans);
    (plain, spans)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Style {
    Text,
    Strong,
    Emphasis,
    StrongEmphasis,
}

impl Style {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Strong => "strong",
            Self::Emphasis => "emphasis",
            Self::StrongEmphasis => "strong_emphasis",
        }
    }
}

fn walk(items: &[Inline], style: Style, plain: &mut String, spans: &mut Vec<SpanData>) {
    for item in items {
        match item {
            Inline::Text(text) => push(plain, spans, style, text, None),
            Inline::Code(code) => {
                plain.push_str(code);
                spans.push(SpanData {
                    style: "code",
                    text: code.clone(),
                    target: None,
                });
            }
            Inline::Strong(inner) => walk(inner, combine(style, Style::Strong), plain, spans),
            Inline::Emphasis(inner) => walk(inner, combine(style, Style::Emphasis), plain, spans),
            Inline::Link { text, target } => {
                let start = plain.len();
                walk(text, style, plain, spans);
                let body = plain[start..].to_owned();
                if !body.is_empty() {
                    spans.push(SpanData {
                        style: "link",
                        text: body,
                        target: Some(target.to_string()),
                    });
                }
            }
            Inline::Image { alt, target } => {
                plain.push_str(alt);
                spans.push(SpanData {
                    style: "image",
                    text: alt.clone(),
                    target: Some(target.to_string()),
                });
            }
        }
    }
}

fn combine(style: Style, addition: Style) -> Style {
    match (style, addition) {
        (Style::Text, other) => other,
        (_, Style::Strong | Style::Emphasis) => Style::StrongEmphasis,
        _ => style,
    }
}

fn push(
    plain: &mut String,
    spans: &mut Vec<SpanData>,
    style: Style,
    text: &str,
    target: Option<String>,
) {
    if text.is_empty() {
        return;
    }
    plain.push_str(text);
    spans.push(SpanData {
        style: style.as_str(),
        text: text.to_owned(),
        target,
    });
}
