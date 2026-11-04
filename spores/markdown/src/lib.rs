//! Markdown reader/editor Spore.
//!
//! Rust owns document semantics and editor state. Lua owns every presentation
//! decision. The seam is one flat [`Snapshot`] passed to [`ui`], so an agent can
//! rewrite the UI live without touching the module.
//!
//! The crate has no filesystem, network, clock, or GPU authority: documents are
//! opaque `DocId`s resolved through [`Vfs`], and images/diagrams are opaque
//! [`Target`]s the host resolves. `MemoryVfs` backs the trait today; `fungos-vfs`
//! implements the same trait later without changing this API.

pub mod content;
pub mod doc;
pub mod edit;
pub mod markdown;
pub mod snapshot;
pub mod surface;
pub mod ui;

pub use content::{FenceKind, HostFormat, Request, RequestKind, classify_fence, requests};
pub use doc::{DocId, MemoryVfs, Target, Vfs};
pub use edit::{Editor, Mode, Position};
pub use markdown::{Block, Inline, Located, parse_blocks, parse_inline};
pub use snapshot::{BlockData, Snapshot, SpanData, project};
pub use surface::Surface;
pub use ui::Ui;

/// Largest document source the spore will accept.
pub const MAX_SOURCE_BYTES: usize = 2 * 1024 * 1024;

/// Largest number of distinct host-resolved references in one snapshot.
pub const MAX_REQUESTS: usize = 256;

/// Bounded undo depth. The spore never grows without a ceiling.
pub const MAX_UNDO: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Malformed identifier, target or argument.
    Invalid,
    /// A documented budget was exceeded.
    OverBudget,
    /// The editor is in read mode.
    ReadOnly,
    /// The host refused the storage or asset operation.
    Denied,
    /// The caller observed an older generation than the current state.
    Stale,
    /// The requested document does not exist.
    NotFound,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let message = match self {
            Self::Invalid => "invalid argument",
            Self::OverBudget => "budget exceeded",
            Self::ReadOnly => "document is in read mode",
            Self::Denied => "operation denied by the host",
            Self::Stale => "stale generation",
            Self::NotFound => "document not found",
        };
        f.write_str(message)
    }
}

impl std::error::Error for Error {}
