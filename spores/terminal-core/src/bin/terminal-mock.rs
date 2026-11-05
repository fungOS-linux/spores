//! Native mock: ANSI fixture -> VT grid -> plain text.
//!
//! This is the first vertical slice without Mycelium or Canvas. The same
//! `terminal_core::render` is linked into the Wasm Spore.

use terminal_core::{ANSI_FIXTURE, render};

fn main() {
    let grid = render(ANSI_FIXTURE.as_bytes(), 40, 8);
    println!(
        "title={:?} cursor=({}, {}) visible={}",
        grid.title, grid.cursor_row, grid.cursor_col, grid.cursor_visible
    );
    println!("{}", grid.plain_text());
}
