//! doc-shell: a native window onto the document layer.
//!
//! The crate is a library plus a thin `main` so the demo fixture and the tests can
//! use the same adapters the window does. Nothing here is a second copy of the
//! document layer: [`doclayer`] is the one place that knows its API, and every
//! pane is a view of what that module returns.

pub mod demo;
pub mod doclayer;
pub mod layout;
pub mod open_doc;
pub mod persist;
pub mod shell;
pub mod theme;
pub mod transcript;
pub mod vault;
