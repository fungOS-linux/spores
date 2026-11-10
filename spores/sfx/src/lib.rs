//! SFX: a soundboard for the events a desktop already produces.
//!
//! The Spore owns the mapping (which event sounds like what), the samples it
//! can name (`soil:vfs`, verified by content id), and the PCM it writes
//! (`soil:audio`). The host owns the bus subscription, the room table, the
//! output device and the clock.

pub mod actions;
pub mod decode;
pub mod lua;
pub mod mapping;
pub mod snapshot;
pub mod synth;

#[cfg(all(target_arch = "wasm32", feature = "guest"))]
mod guest;
