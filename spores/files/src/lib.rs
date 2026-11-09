//! files: a VFS browser and storage census Spore.
//!
//! Rust owns the namespace semantics and the census; Lua owns every pixel. The
//! seam is one flat [`Snapshot`](snapshot::Snapshot) passed to the view, so an
//! agent can restyle a running app without rebuilding the module.
//!
//! The crate has no filesystem, network, clock, or GPU authority. It reads a
//! [`Vfs`](vfs::Vfs): a host-supplied, workspace-relative, paged namespace that
//! is the same shape as `soil:vfs@1.0.0`. [`StdFs`](vfs::StdFs) is the host-side
//! adapter `files-serve` supplies until Soil links the module to the real grant;
//! [`MemoryVfs`](vfs::MemoryVfs) is what the tests drive.
//!
//! What this Spore deliberately does not do yet: move bytes (backup, projection,
//! sync). A transport would be a second grant with a different threat model, and
//! the census has to be right before it can be trusted to describe what moved.

pub mod scan;
pub mod snapshot;
pub mod vfs;
#[cfg(not(target_arch = "wasm32"))]
pub mod vfs_scan;

pub use scan::{KindClass, KindStat, Limits, Scan, Totals, scan};
pub use snapshot::{Crumb, ListingRow, Root, Snapshot};
pub use vfs::{Entry, Error, Kind, MemoryVfs, Page, StdFs, Vfs};
