//! VT parsing shared by the terminal Spore and its native mock.
//!
//! The Spore links this crate, feeds it PTY bytes, and submits the resulting
//! cell grid to `soil:terminal`. Keeping the conversion here lets the mock run
//! without a Wasm host.

use serde::{Deserialize, Serialize};
#[cfg(feature = "vt100")]
use unicode_width::UnicodeWidthStr;
#[cfg(feature = "vt100")]
use vt100::Color;

/// A deterministic ANSI fixture: title, colors, cursor movement, clear.
pub const ANSI_FIXTURE: &str = "\x1b]2;terminal fixture\x07\
\x1b[1;32mhello\x1b[0m from \x1b[1;34mmycelium\x1b[0m\r\n\
plain\r\n\
\x1b[7m reversed \x1b[0m \x1b[4munderline\x1b[0m\r\n\
\x1b[2J\x1b[H\
\x1b[33mstatus:\x1b[0m ready";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Style {
    pub fg: u32,
    pub bg: u32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub reverse: bool,
    pub dim: bool,
    pub strikethrough: bool,
}


#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cell {
    /// One grapheme cluster; empty for a wide-cell continuation.
    pub text: String,
    /// 1 or 2 display columns.
    pub width: u8,
    pub style: Style,
}

impl Cell {
    pub fn blank() -> Self {
        Self {
            text: " ".to_string(),
            width: 1,
            style: Style::default(),
        }
    }

    pub fn is_blank(&self) -> bool {
        self.text == " " && self.width == 1 && self.style == Style::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grid {
    pub cols: u32,
    pub rows: u32,
    /// Row-major, exactly `cols * rows` cells.
    pub cells: Vec<Cell>,
    pub cursor_row: u32,
    pub cursor_col: u32,
    pub cursor_visible: bool,
    pub title: Option<String>,
}

impl Grid {
    pub fn cell(&self, row: u32, col: u32) -> Option<&Cell> {
        if row >= self.rows || col >= self.cols {
            return None;
        }
        self.cells
            .get((row as usize) * (self.cols as usize) + (col as usize))
    }

    /// Plain-text view used by the native mock and tests.
    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        for row in 0..self.rows {
            for col in 0..self.cols {
                let cell = self.cell(row, col).expect("grid is rectangular");
                out.push_str(if cell.text.is_empty() {
                    " "
                } else {
                    &cell.text
                });
            }
            while out.ends_with(' ') {
                out.pop();
            }
            if row + 1 < self.rows {
                out.push('\n');
            }
        }
        out
    }
}

/// Parse bytes into a fixed-size grid. The caller owns the bounds.
#[cfg(feature = "vt100")]
pub fn render(bytes: &[u8], cols: u32, rows: u32) -> Grid {
    let mut parser = vt100::Parser::new(rows as u16, cols as u16, 0);
    parser.process(bytes);
    let screen = parser.screen();

    let mut cells = Vec::with_capacity((cols as usize) * (rows as usize));
    for row in 0..rows {
        for col in 0..cols {
            let cell = screen.cell(row as u16, col as u16);
            let Some(cell) = cell else {
                cells.push(Cell::blank());
                continue;
            };
            let text = cell.contents().to_string();
            if text.is_empty() || cell.is_wide_continuation() {
                cells.push(Cell {
                    text: String::new(),
                    width: 0,
                    style: style(cell),
                });
                continue;
            }
            let width = if cell.is_wide() {
                2
            } else {
                UnicodeWidthStr::width(text.as_str()).clamp(1, 2) as u8
            };
            cells.push(Cell {
                text,
                width,
                style: style(cell),
            });
        }
    }

    let (cursor_row, cursor_col) = screen.cursor_position();
    Grid {
        cols,
        rows,
        cells,
        cursor_row: u32::from(cursor_row),
        cursor_col: u32::from(cursor_col),
        cursor_visible: !screen.hide_cursor(),
        title: Some(screen.title().to_string()).filter(|title| !title.is_empty()),
    }
}

#[cfg(feature = "vt100")]
fn style(cell: &vt100::Cell) -> Style {
    Style {
        fg: color(cell.fgcolor()),
        bg: color(cell.bgcolor()),
        bold: cell.bold(),
        italic: cell.italic(),
        underline: cell.underline(),
        reverse: cell.inverse(),
        dim: false,
        strikethrough: false,
    }
}

#[cfg(feature = "vt100")]
fn color(color: Color) -> u32 {
    match color {
        Color::Default => 0,
        Color::Idx(index) => u32::from(index),
        Color::Rgb(r, g, b) => (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b),
    }
}

#[cfg(all(test, feature = "vt100"))]
mod tests {
    use super::*;

    #[test]
    fn fixture_clears_and_renders_status() {
        let grid = render(ANSI_FIXTURE.as_bytes(), 24, 4);
        assert_eq!(grid.cols, 24);
        assert_eq!(grid.rows, 4);
        assert_eq!(grid.title.as_deref(), Some("terminal fixture"));
        let text = grid.plain_text();
        assert!(text.contains("status: ready"), "got:\n{text}");
        assert!(!text.contains("hello"), "clear should remove earlier rows");
    }

    #[test]
    fn wide_cells_are_marked() {
        let grid = render("界".as_bytes(), 4, 1);
        assert_eq!(grid.cell(0, 0).unwrap().text, "界");
        assert_eq!(grid.cell(0, 0).unwrap().width, 2);
        assert!(grid.cell(0, 1).unwrap().text.is_empty());
    }

    #[test]
    fn plain_text_trims_trailing_blanks() {
        let grid = render(b"ok", 8, 1);
        assert_eq!(grid.plain_text(), "ok");
    }
}
