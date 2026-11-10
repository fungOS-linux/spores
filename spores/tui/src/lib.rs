//! A ratatui dashboard that renders to ANSI, not to a terminal.
//!
//! This is the shape a TUI takes as a Spore: it owns layout and state, and
//! returns one complete ANSI frame per call. The host decides where that frame
//! goes — a real terminal, `soil-vt` inside the terminal Spore, or Canvas — and
//! the Spore never knows which.

mod ansi;
#[cfg(target_arch = "wasm32")]
mod guest;

use ratatui::Frame;
use ratatui::Terminal;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Gauge, List, ListItem, ListState, Paragraph, Tabs, Wrap};

pub use ansi::AnsiBackend;

const TABS: [&str; 3] = ["Overview", "Log", "About"];
const ITEMS: [&str; 4] = ["Ingest", "Index", "Render", "Sync"];

pub struct App {
    cols: u16,
    rows: u16,
    tab: usize,
    selected: usize,
    ticks: u64,
    log: Vec<String>,
}

impl App {
    pub fn new(cols: u16, rows: u16) -> Self {
        Self {
            cols,
            rows,
            tab: 0,
            selected: 0,
            ticks: 0,
            log: vec!["session started".into()],
        }
    }

    /// A deterministic single frame: the same tick always draws the same bytes.
    pub fn preview(cols: u16, rows: u16, tick: u64) -> Self {
        let mut app = Self::new(cols, rows);
        app.ticks = tick;
        app.selected = tick as usize % ITEMS.len();
        app
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.cols = cols;
        self.rows = rows;
    }

    pub fn key(&mut self, key: &str) {
        match key {
            "tab" | "\t" => self.tab = (self.tab + 1) % TABS.len(),
            "j" | "down" => self.selected = (self.selected + 1) % ITEMS.len(),
            "k" | "up" => self.selected = self.selected.saturating_sub(1),
            " " | "space" | "enter" => {
                self.log.push(format!("activated {}", ITEMS[self.selected]));
                self.ticks += 1;
            }
            "r" => self.ticks += 1,
            "q" => self.log.push("quit requested".into()),
            _ => {}
        }
    }

    pub fn render(&self) -> Result<String, String> {
        let backend = AnsiBackend::new(self.cols, self.rows);
        let mut terminal = Terminal::new(backend).map_err(|error| error.to_string())?;
        // A frame is a complete screen, not a diff against whatever the host
        // drew last. Clearing resets ratatui's previous buffer so `draw` emits
        // every cell, including the spaces a diff would have skipped.
        terminal.clear().map_err(|error| error.to_string())?;
        terminal
            .draw(|frame| self.draw(frame))
            .map_err(|error| error.to_string())?;
        let bytes = terminal.backend().output().to_vec();
        String::from_utf8(bytes).map_err(|error| error.to_string())
    }

    fn draw(&self, frame: &mut Frame) {
        let area = frame.area();
        if area.width < 24 || area.height < 8 {
            frame.render_widget(Paragraph::new("too small"), area);
            return;
        }
        let rows = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(3),
        ])
        .split(area);

        let tabs = Tabs::new(TABS)
            .select(self.tab)
            .block(Block::bordered().title(" soil tui "));
        frame.render_widget(tabs, rows[0]);

        match self.tab {
            0 => self.draw_overview(frame, rows[1]),
            1 => self.draw_log(frame, rows[1]),
            _ => self.draw_about(frame, rows[1]),
        }

        let spinner = ["|", "/", "-", "\\"][self.ticks as usize % 4];
        let footer = Line::from(vec![
            Span::styled("tab", Style::new().add_modifier(Modifier::BOLD)),
            Span::raw(" switch · "),
            Span::styled("j/k", Style::new().add_modifier(Modifier::BOLD)),
            Span::raw(" move · "),
            Span::styled("space", Style::new().add_modifier(Modifier::BOLD)),
            Span::raw(" act · "),
            Span::styled("q", Style::new().add_modifier(Modifier::BOLD)),
            Span::raw(format!(" quit   {spinner} tick {}", self.ticks)),
        ]);
        frame.render_widget(Paragraph::new(footer).block(Block::bordered()), rows[2]);
    }

    fn draw_overview(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let rows = Layout::vertical([Constraint::Min(3), Constraint::Length(3)]).split(area);
        let items: Vec<ListItem> = ITEMS.iter().map(|item| ListItem::new(*item)).collect();
        let mut state = ListState::default().with_selected(Some(self.selected));
        let list = List::new(items)
            .block(Block::bordered().title(" queue "))
            .highlight_symbol("> ")
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
        frame.render_stateful_widget(list, rows[0], &mut state);

        let ratio = (self.ticks % 100) as f64 / 100.0;
        let gauge = Gauge::default()
            .block(Block::bordered().title(" progress "))
            .gauge_style(Style::new().fg(Color::LightCyan))
            .ratio(ratio)
            .label(format!("{:.0}%", ratio * 100.0));
        frame.render_widget(gauge, rows[1]);
    }

    fn draw_log(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let lines: Vec<Line> = self
            .log
            .iter()
            .map(|line| Line::from(line.clone()))
            .collect();
        let paragraph = Paragraph::new(lines)
            .block(Block::bordered().title(" log "))
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
    }

    fn draw_about(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let lines = vec![
            Line::from("ratatui, no terminal backend"),
            Line::from("every frame is ANSI bytes"),
            Line::from(format!("screen: {}x{}", self.cols, self.rows)),
            Line::from("capabilities: none"),
        ];
        let paragraph = Paragraph::new(lines)
            .block(Block::bordered().title(" about "))
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The visible text of a frame: CSI sequences removed, glyphs in draw order.
    /// The backend fills blank runs, so this is the screen row by row.
    fn plain(frame: &str) -> String {
        let mut out = String::new();
        let mut chars = frame.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch != '\x1b' {
                out.push(ch);
                continue;
            }
            if chars.peek() == Some(&'[') {
                chars.next();
                for ch in chars.by_ref() {
                    if ('@'..='~').contains(&ch) {
                        break;
                    }
                }
            }
        }
        out
    }

    #[test]
    fn a_preview_contains_the_title_and_every_item() {
        let frame = plain(&App::preview(80, 24, 0).render().unwrap());
        assert!(frame.contains("soil tui"), "{frame}");
        for item in ITEMS {
            assert!(frame.contains(item), "missing {item}: {frame}");
        }
        assert!(frame.contains("queue"), "{frame}");
        assert!(frame.contains("progress"), "{frame}");
    }

    #[test]
    fn frames_are_ansi_and_utf8() {
        let frame = App::preview(40, 12, 3).render().unwrap();
        assert!(
            frame.contains('\x1b'),
            "a frame must carry escape sequences"
        );
        assert!(frame.contains("\x1b["), "{frame}");
    }

    #[test]
    fn keys_move_the_selection_and_activate_it() {
        let mut app = App::new(40, 12);
        app.key("j");
        app.key("j");
        app.key("space");
        assert!(app.log.iter().any(|line| line == "activated Render"));
        app.key("k");
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn tab_switches_the_body() {
        let mut app = App::new(60, 16);
        app.key("tab");
        let frame = plain(&app.render().unwrap());
        assert!(frame.contains("log"), "{frame}");
        assert!(frame.contains("session started"), "{frame}");
    }

    #[test]
    fn resizing_reflows_the_frame() {
        let narrow = App::preview(30, 10, 0).render().unwrap();
        let wide = App::preview(100, 30, 0).render().unwrap();
        assert_ne!(narrow, wide);
        assert!(plain(&narrow).contains("soil tui"));
        assert!(plain(&wide).contains("soil tui"));
    }

    #[test]
    fn a_tiny_screen_is_explained_rather_than_panicking() {
        let frame = plain(&App::preview(10, 3, 0).render().unwrap());
        assert!(frame.contains("too small"), "{frame}");
    }

    #[test]
    fn previews_are_deterministic() {
        assert_eq!(
            App::preview(50, 20, 7).render().unwrap(),
            App::preview(50, 20, 7).render().unwrap()
        );
    }
}
