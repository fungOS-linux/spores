//! Sheets Spore.
//!
//! The workbook model and formula evaluation live in [`model`] and build for
//! both the host and the Spore target; the component below is only compiled for
//! wasm, where `soil:records` exists.
pub mod model;

#[cfg(target_family = "wasm")]
mod spore;
