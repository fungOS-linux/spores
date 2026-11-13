//! Which pane is where.
//!
//! There is one layout, not two. A layout is an ordering of the three panes plus
//! a width for each side pane; the document pane takes what is left. "Flipped"
//! is a permutation of that ordering, so every pane renders from the same code in
//! every position and nothing anywhere has to know which side it is on.

use serde::{Deserialize, Serialize};

/// The three regions of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pane {
    Sidebar,
    Document,
    Transcript,
}

impl Pane {
    pub const ALL: [Pane; 3] = [Pane::Sidebar, Pane::Document, Pane::Transcript];

    /// The word the pane's own header shows.
    pub fn label(self) -> &'static str {
        match self {
            Pane::Sidebar => "vault",
            Pane::Document => "document",
            Pane::Transcript => "agent",
        }
    }

    /// Stable identity for element ids and the persisted layout.
    pub fn key(self) -> &'static str {
        match self {
            Pane::Sidebar => "sidebar",
            Pane::Document => "document",
            Pane::Transcript => "transcript",
        }
    }
}

/// The panes that do not flex, and how wide they are.
///
/// The document pane has no entry on purpose: it is the one that takes the
/// remaining width, which is what keeps the layout readable at any window size.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Widths {
    pub sidebar: f32,
    pub transcript: f32,
}

pub const SIDEBAR_MIN: f32 = 180.0;
pub const SIDEBAR_MAX: f32 = 520.0;
pub const TRANSCRIPT_MIN: f32 = 220.0;
pub const TRANSCRIPT_MAX: f32 = 720.0;

impl Default for Widths {
    fn default() -> Self {
        // Delta-shaped: a slim tree, a wide document, a deliberately slim
        // transcript. The transcript is for watching, not for writing.
        Self {
            sidebar: 260.0,
            transcript: 340.0,
        }
    }
}

impl Widths {
    fn get(&self, pane: Pane) -> Option<f32> {
        match pane {
            Pane::Sidebar => Some(self.sidebar),
            Pane::Transcript => Some(self.transcript),
            Pane::Document => None,
        }
    }

    fn set(&mut self, pane: Pane, width: f32) {
        match pane {
            Pane::Sidebar => self.sidebar = width.clamp(SIDEBAR_MIN, SIDEBAR_MAX),
            Pane::Transcript => self.transcript = width.clamp(TRANSCRIPT_MIN, TRANSCRIPT_MAX),
            Pane::Document => {}
        }
    }
}

/// One window layout: the panes left to right, and two widths.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    order: Vec<Pane>,
    widths: Widths,
}

impl Default for Layout {
    fn default() -> Self {
        Self::delta()
    }
}

impl Layout {
    /// `vault | document | agent`.
    pub fn delta() -> Self {
        Self {
            order: vec![Pane::Sidebar, Pane::Document, Pane::Transcript],
            widths: Widths::default(),
        }
    }

    /// Every pane, left to right.
    pub fn panes(&self) -> &[Pane] {
        &self.order
    }

    /// The width a pane was given, or `None` when it flexes.
    pub fn width(&self, pane: Pane) -> Option<f32> {
        self.widths.get(pane)
    }

    pub fn set_width(&mut self, pane: Pane, width: f32) {
        self.widths.set(pane, width);
    }

    /// Swap the document and the transcript.
    ///
    /// This is the whole flip: two positions in one ordering change, and every
    /// other pane — the sidebar included — stays where it was.
    pub fn flip(&mut self) {
        let document = self.index(Pane::Document);
        let transcript = self.index(Pane::Transcript);
        self.order.swap(document, transcript);
    }

    /// Send the sidebar to the other end, keeping the other two panes in order.
    pub fn move_sidebar(&mut self) {
        let index = self.index(Pane::Sidebar);
        let sidebar = self.order.remove(index);
        if index == 0 {
            self.order.push(sidebar);
        } else {
            self.order.insert(0, sidebar);
        }
    }

    /// Whether the sidebar is the last pane, i.e. on the right in a left-to-right
    /// reading order.
    pub fn sidebar_at_end(&self) -> bool {
        self.order.last() == Some(&Pane::Sidebar)
    }

    /// A layout is a permutation of the three panes and nothing else. Anything
    /// else — a hand-edited file, a pane removed in a later version — is replaced
    /// rather than rendered half-built.
    pub fn is_valid(&self) -> bool {
        let mut seen: Vec<Pane> = self.order.clone();
        seen.sort();
        seen == Pane::ALL.to_vec()
            && self.widths.sidebar.is_finite()
            && self.widths.transcript.is_finite()
            && self.widths.sidebar > 0.0
            && self.widths.transcript > 0.0
    }

    fn index(&self, pane: Pane) -> usize {
        self.order
            .iter()
            .position(|candidate| *candidate == pane)
            .expect("a layout holds every pane once")
    }

    /// The ordering as one string, used to give each arrangement its own
    /// resizable state: dragging a divider in one arrangement must not decide the
    /// widths of another.
    pub fn key(&self) -> String {
        self.order
            .iter()
            .map(|pane| pane.key())
            .collect::<Vec<_>>()
            .join("+")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flipping_twice_is_the_layout_it_started_from() {
        let mut layout = Layout::delta();
        let before = layout.clone();
        layout.flip();
        assert_eq!(
            layout.panes(),
            &[Pane::Sidebar, Pane::Transcript, Pane::Document]
        );
        layout.flip();
        assert_eq!(layout, before);
    }

    #[test]
    fn the_sidebar_can_hold_either_end() {
        let mut layout = Layout::delta();
        assert!(!layout.sidebar_at_end());
        layout.move_sidebar();
        assert_eq!(
            layout.panes(),
            &[Pane::Document, Pane::Transcript, Pane::Sidebar]
        );
        assert!(layout.sidebar_at_end());
        // The other two keep their relative order, so a sidebar move is not
        // secretly a flip.
        layout.flip();
        assert_eq!(
            layout.panes(),
            &[Pane::Transcript, Pane::Document, Pane::Sidebar]
        );
        layout.move_sidebar();
        assert_eq!(
            layout.panes(),
            &[Pane::Sidebar, Pane::Transcript, Pane::Document]
        );
    }

    #[test]
    fn widths_travel_with_the_pane_not_the_position() {
        let mut layout = Layout::delta();
        layout.set_width(Pane::Transcript, 420.0);
        layout.flip();
        assert_eq!(layout.width(Pane::Transcript), Some(420.0));
        assert_eq!(layout.width(Pane::Document), None, "the document flexes");
    }

    #[test]
    fn widths_are_clamped_so_a_pane_cannot_vanish() {
        let mut layout = Layout::delta();
        layout.set_width(Pane::Sidebar, 1.0);
        assert_eq!(layout.width(Pane::Sidebar), Some(SIDEBAR_MIN));
        layout.set_width(Pane::Sidebar, 10_000.0);
        assert_eq!(layout.width(Pane::Sidebar), Some(SIDEBAR_MAX));
    }

    #[test]
    fn a_layout_round_trips_through_json() {
        let mut layout = Layout::delta();
        layout.flip();
        layout.move_sidebar();
        layout.set_width(Pane::Sidebar, 300.0);
        let json = serde_json::to_string(&layout).unwrap();
        let back: Layout = serde_json::from_str(&json).unwrap();
        assert_eq!(back, layout);
        assert!(back.is_valid());
    }

    #[test]
    fn a_layout_missing_a_pane_is_invalid() {
        let broken = Layout {
            order: vec![Pane::Sidebar, Pane::Document],
            widths: Widths::default(),
        };
        assert!(!broken.is_valid());
        let doubled = Layout {
            order: vec![Pane::Sidebar, Pane::Document, Pane::Document],
            widths: Widths::default(),
        };
        assert!(!doubled.is_valid());
    }
}
