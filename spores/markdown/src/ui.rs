//! The Lua UI source, shipped beside the module.
//!
//! The UI is not compiled into the Spore. It is a Lua file Canvas loads, JITs,
//! and can hot-reload on `digest()` change. Editing this file restyles or
//! restructures the app without rebuilding the wasm module, which is what makes
//! live agent tweaks to a running app possible.

use crate::surface::Surface;

/// The UI shipped with this Spore. Canvas may substitute a user or agent
/// override at the same entry point; the Spore never assumes its own copy won.
pub const ENTRY: &str = "ui/editor.lua";

pub struct Ui;

impl Ui {
    /// The reviewed default UI.
    pub fn source() -> &'static str {
        include_str!("../ui/editor.lua")
    }

    /// Identity of the default UI. Canvas recompiles when this changes and
    /// records it so a view can say which UI produced it.
    pub fn digest() -> String {
        digest(Self::source())
    }

    /// Surfaces this UI is declared to work on. Canvas renders all of them; this
    /// list only tells Soil what it may launch.
    pub fn surfaces() -> [Surface; 3] {
        Surface::ALL
    }
}

pub fn digest(source: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in source.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}
