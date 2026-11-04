//! Content classification and host-resolution requests.
//!
//! A fenced block whose language names a renderer is *not* code. The Spore
//! ships the source and the host draws it. The same rule governs images and
//! attachments: the Spore names a [`Target`], the host owns the bytes.

use crate::doc::Target;
use crate::markdown::{Block, Inline, Located};
use crate::{MAX_REQUESTS, markdown};

/// A language the host renders rather than the Spore showing as code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostFormat {
    Mermaid,
    /// Reserved for the project's own diagram language (see `docs/host-contract.md`).
    Pele,
    Graphviz,
    Plantuml,
    Math,
}

impl HostFormat {
    pub fn from_info(info: &str) -> Option<Self> {
        let language = info.split_whitespace().next().unwrap_or("");
        match language.to_ascii_lowercase().as_str() {
            "mermaid" => Some(Self::Mermaid),
            "pele" => Some(Self::Pele),
            "dot" | "graphviz" => Some(Self::Graphviz),
            "plantuml" | "puml" => Some(Self::Plantuml),
            "math" | "katex" | "latex" => Some(Self::Math),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mermaid => "mermaid",
            Self::Pele => "pele",
            Self::Graphviz => "graphviz",
            Self::Plantuml => "plantuml",
            Self::Math => "math",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FenceKind {
    Code,
    HostRendered(HostFormat),
}

pub fn classify_fence(info: &str) -> FenceKind {
    match HostFormat::from_info(info) {
        Some(format) => FenceKind::HostRendered(format),
        None => FenceKind::Code,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestKind {
    Image,
    Diagram(HostFormat),
}

/// One thing the host must resolve before the view is complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub kind: RequestKind,
    pub target: Target,
    /// Zero-based source line, so a host can correlate pixels with the editor.
    pub line: usize,
    /// Alt text or fence language, for accessibility and error messages.
    pub label: String,
}

impl Request {
    /// Stable identity for caching. The host resolves by this, not by line.
    pub fn key(&self) -> String {
        match self.kind {
            RequestKind::Image => format!("image:{}", self.target),
            RequestKind::Diagram(format) => {
                format!("diagram:{}:{}", format.as_str(), self.label)
            }
        }
    }
}

/// Collect every host-resolved reference, deduplicated and bounded.
pub fn requests(blocks: &[Located]) -> Vec<Request> {
    let mut out: Vec<Request> = Vec::new();

    let mut push = |request: Request| {
        if out.len() >= MAX_REQUESTS {
            return;
        }
        if out.iter().any(|existing| existing.key() == request.key()) {
            return;
        }
        out.push(request);
    };

    for located in blocks {
        match &located.block {
            Block::Heading { text, .. } | Block::Paragraph { text } | Block::Quote { text } => {
                collect_inline(text, located.line, &mut push);
            }
            Block::List { items, .. } => {
                for item in items {
                    collect_inline(item, located.line, &mut push);
                }
            }
            Block::Fence { info, text } => {
                if let FenceKind::HostRendered(format) = classify_fence(info) {
                    // Diagram identity is its source, not its line, so editing
                    // above it does not invalidate a host cache entry.
                    push(Request {
                        kind: RequestKind::Diagram(format),
                        target: Target::Asset {
                            algorithm: "inline".to_owned(),
                            digest: inline_digest(info, text),
                        },
                        line: located.line,
                        label: format.as_str().to_owned(),
                    });
                }
            }
            Block::Rule | Block::Blank => {}
        }
    }

    out
}

fn collect_inline<F: FnMut(Request)>(items: &[Inline], line: usize, push: &mut F) {
    for item in items {
        match item {
            Inline::Text(_) | Inline::Code(_) => {}
            Inline::Strong(inner) | Inline::Emphasis(inner) => collect_inline(inner, line, push),
            Inline::Link { text, .. } => collect_inline(text, line, push),
            Inline::Image { alt, target } => push(Request {
                kind: RequestKind::Image,
                target: target.clone(),
                line,
                label: alt.clone(),
            }),
        }
    }
}

/// Deterministic, dependency-free 64-bit FNV-1a. This is a cache key, not a
/// content address: the host owns real CIDs for stored assets.
fn inline_digest(info: &str, text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in info
        .bytes()
        .chain(std::iter::once(b'\n'))
        .chain(text.bytes())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Convenience for callers that already have a source string.
pub fn requests_of(source: &str) -> Vec<Request> {
    requests(&markdown::parse_blocks(source))
}
