//! An ANSI backend for ratatui.
//!
//! ratatui normally renders through a host terminal backend (crossterm,
//! termion, termwiz). A Spore has no terminal: it has a byte stream. This is the
//! missing adapter — `Terminal<AnsiBackend>` draws into a `Vec<u8>` that any VT
//! host can parse, whether that host is a real terminal, `soil-vt`, or Canvas.

use std::io;

use ratatui::backend::{Backend, ClearType, WindowSize};
use ratatui::buffer::Cell;
use ratatui::layout::{Position, Size};
use ratatui::style::{Color, Modifier};

pub struct AnsiBackend {
    cols: u16,
    rows: u16,
    cursor: Position,
    out: Vec<u8>,
}

impl AnsiBackend {
    pub fn new(cols: u16, rows: u16) -> Self {
        Self {
            cols,
            rows,
            cursor: Position { x: 0, y: 0 },
            out: Vec::new(),
        }
    }

    /// The bytes drawn so far. One complete frame after `Terminal::draw`.
    pub fn output(&self) -> &[u8] {
        &self.out
    }

    fn move_to(&mut self, x: u16, y: u16) {
        self.cursor = Position { x, y };
        self.out
            .extend_from_slice(format!("\x1b[{};{}H", y + 1, x + 1).as_bytes());
    }

    fn set_style(&mut self, cell: &Cell) {
        let mut params = vec!["0".to_string()];
        for (flag, code) in [
            (Modifier::BOLD, "1"),
            (Modifier::DIM, "2"),
            (Modifier::ITALIC, "3"),
            (Modifier::UNDERLINED, "4"),
            (Modifier::REVERSED, "7"),
            (Modifier::CROSSED_OUT, "9"),
        ] {
            if cell.modifier.contains(flag) {
                params.push(code.to_string());
            }
        }
        params.push(foreground(cell.fg));
        params.push(background(cell.bg));
        self.out
            .extend_from_slice(format!("\x1b[{}m", params.join(";")).as_bytes());
    }
}

impl Backend for AnsiBackend {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        // Adjacent cells with the same style are one run: emit the glyphs back
        // to back and only move or restyle when the run breaks. A per-cell
        // cursor move would triple the bytes and make a frame unreadable.
        let mut previous: Option<(u16, u16)> = None;
        let mut style: Option<(Color, Color, Modifier)> = None;
        for (x, y, cell) in content {
            // ratatui uses an empty symbol for the second half of a wide cell.
            // The leading cell already advanced the cursor.
            if cell.symbol().is_empty() {
                continue;
            }
            let cell_style = (cell.fg, cell.bg, cell.modifier);
            let adjacent = previous.is_some_and(|(px, py)| py == y && px + 1 == x);
            // `draw` receives a diff: cells equal to the previous frame are
            // omitted. A frame here is a complete repaint of a screen the host
            // just cleared, so an omitted run is blank and has to be written
            // as spaces — otherwise the text would be split by invisible gaps.
            let gap = previous
                .and_then(|(px, py)| (py == y && px + 1 < x).then(|| usize::from(x - px - 1)));
            if let Some(width) = gap {
                self.out.extend_from_slice(b"\x1b[0m");
                self.out.extend(std::iter::repeat_n(b' ', width));
                self.cursor = Position { x, y };
                style = Some((Color::Reset, Color::Reset, Modifier::empty()));
            } else if !adjacent {
                self.move_to(x, y);
            }
            if style != Some(cell_style) {
                self.set_style(cell);
                style = Some(cell_style);
            }
            self.out.extend_from_slice(cell.symbol().as_bytes());
            previous = Some((x, y));
        }
        Ok(())
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        self.out.extend_from_slice(b"\x1b[?25l");
        Ok(())
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        self.out.extend_from_slice(b"\x1b[?25h");
        Ok(())
    }

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        Ok(self.cursor)
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        let position = position.into();
        self.move_to(position.x, position.y);
        Ok(())
    }

    fn clear(&mut self) -> io::Result<()> {
        self.out.extend_from_slice(b"\x1b[2J");
        self.cursor = Position { x: 0, y: 0 };
        Ok(())
    }

    fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
        let sequence: &[u8] = match clear_type {
            ClearType::All => b"\x1b[2J",
            ClearType::AfterCursor => b"\x1b[0J",
            ClearType::BeforeCursor => b"\x1b[1J",
            ClearType::CurrentLine => b"\x1b[2K",
            ClearType::UntilNewLine => b"\x1b[0K",
        };
        self.out.extend_from_slice(sequence);
        Ok(())
    }

    fn size(&self) -> io::Result<Size> {
        Ok(Size::new(self.cols, self.rows))
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        Ok(WindowSize {
            columns_rows: Size::new(self.cols, self.rows),
            // A VT grid has no pixels. A host that does (Canvas) scales it.
            pixels: Size::new(0, 0),
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn foreground(color: Color) -> String {
    match color {
        Color::Reset => "39".into(),
        Color::Black => "30".into(),
        Color::Red => "31".into(),
        Color::Green => "32".into(),
        Color::Yellow => "33".into(),
        Color::Blue => "34".into(),
        Color::Magenta => "35".into(),
        Color::Cyan => "36".into(),
        Color::Gray => "37".into(),
        Color::DarkGray => "90".into(),
        Color::LightRed => "91".into(),
        Color::LightGreen => "92".into(),
        Color::LightYellow => "93".into(),
        Color::LightBlue => "94".into(),
        Color::LightMagenta => "95".into(),
        Color::LightCyan => "96".into(),
        Color::White => "97".into(),
        Color::Rgb(r, g, b) => format!("38;2;{r};{g};{b}"),
        Color::Indexed(index) => format!("38;5;{index}"),
    }
}

fn background(color: Color) -> String {
    match color {
        Color::Reset => "49".into(),
        Color::Black => "40".into(),
        Color::Red => "41".into(),
        Color::Green => "42".into(),
        Color::Yellow => "43".into(),
        Color::Blue => "44".into(),
        Color::Magenta => "45".into(),
        Color::Cyan => "46".into(),
        Color::Gray => "47".into(),
        Color::DarkGray => "100".into(),
        Color::LightRed => "101".into(),
        Color::LightGreen => "102".into(),
        Color::LightYellow => "103".into(),
        Color::LightBlue => "104".into(),
        Color::LightMagenta => "105".into(),
        Color::LightCyan => "106".into(),
        Color::White => "107".into(),
        Color::Rgb(r, g, b) => format!("48;2;{r};{g};{b}"),
        Color::Indexed(index) => format!("48;5;{index}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(symbol: &'static str) -> Cell {
        Cell::new(symbol)
    }

    #[test]
    fn truecolor_and_reset_are_emitted() {
        let mut backend = AnsiBackend::new(4, 1);
        let mut c = cell("x");
        c.fg = Color::Rgb(1, 2, 3);
        c.bg = Color::Indexed(21);
        c.modifier = Modifier::BOLD | Modifier::ITALIC;
        backend.set_style(&c);
        let out = String::from_utf8_lossy(backend.output());
        assert!(out.contains("38;2;1;2;3"), "{out}");
        assert!(out.contains("48;5;21"), "{out}");
        assert!(out.contains(";1;") && out.contains(";3;"), "{out}");
        assert!(out.starts_with("\x1b[0;"), "{out}");
    }

    #[test]
    fn empty_symbols_do_not_advance_the_cursor() {
        let mut backend = AnsiBackend::new(4, 1);
        let empty = cell("");
        let glyph = cell("a");
        backend
            .draw([(0, 0, &empty), (1, 0, &glyph)].into_iter())
            .unwrap();
        let out = String::from_utf8_lossy(backend.output());
        assert!(!out.contains("\x1b[1;1H"), "{out}");
        assert!(out.contains("\x1b[1;2H"), "{out}");
    }

    #[test]
    fn clear_regions_map_to_the_expected_sequences() {
        let mut backend = AnsiBackend::new(4, 1);
        backend.clear_region(ClearType::CurrentLine).unwrap();
        assert_eq!(backend.output(), b"\x1b[2K");
        backend.clear_region(ClearType::All).unwrap();
        assert_eq!(backend.output(), b"\x1b[2K\x1b[2J");
    }
}
