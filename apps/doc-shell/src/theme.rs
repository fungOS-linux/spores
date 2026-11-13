//! What the colours mean, in one place.
//!
//! The kit owns the palette: `gpui-kit` ships its themes as JSON and resolves them
//! into `Theme`, so light and dark are one setting and not two stylesheets. This
//! module owns the layer above that — the *semantics* this window needs a colour
//! for, so that "a thread whose block moved" is decided once and not at four call
//! sites with `theme().warning` repeated.
//!
//! That split is also the answer to "how do all the Spores look the same": none of
//! them should name a colour. A Spore reads the same semantic tokens from the same
//! theme, and an appearance change moves every surface at once. Anything this module
//! needs that the kit does not yet have a token for belongs in the kit, not here.

use gpui_kit::component::theme::{Theme, ThemeMode};
use gpui_kit::gpui::Hsla;

/// The colours this window draws with.
#[derive(Clone, Copy, Debug)]
pub struct Tokens {
    /// The block is here, unchanged.
    pub exact: Hsla,
    /// The block is here but the words moved under the range.
    pub moved: Hsla,
    /// The block is gone; this is what took its place.
    pub gone: Hsla,
    /// There is nothing left to point at.
    pub orphan: Hsla,
    /// Work that has not been written yet.
    pub dirty: Hsla,
    /// A note that is not a warning.
    pub note: Hsla,
}

impl Tokens {
    #[must_use]
    pub fn of(theme: &Theme) -> Self {
        Self {
            exact: theme.success,
            // `clamped` and `neighbour` share a colour on purpose: both mean "this
            // moved", and the difference between them is the sentence next to it.
            moved: theme.warning,
            gone: theme.warning,
            orphan: theme.danger,
            dirty: theme.warning,
            note: theme.muted_foreground,
        }
    }

    /// The colour for a resolution, by the word the document layer prints.
    #[must_use]
    pub fn resolution(&self, state: &str) -> Hsla {
        match state {
            "exact" => self.exact,
            "clamped" | "neighbour" => self.moved,
            _ => self.orphan,
        }
    }
}

/// Which side of the light the window is on.
///
/// Two modes, because that is what the kit has: `ThemeMode` is light or dark, and
/// "follow the desktop" is a capability it does not expose yet. When it grows one,
/// this enum grows a variant and nothing else changes — which is the point of the
/// window asking *this* rather than reading a system setting in four places.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Appearance {
    Light,
    #[default]
    Dark,
}

impl Appearance {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    /// The other appearance, for a keypress or a click.
    #[must_use]
    pub fn toggled(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Light,
        }
    }

    #[must_use]
    pub fn mode(self) -> ThemeMode {
        match self {
            Self::Light => ThemeMode::Light,
            Self::Dark => ThemeMode::Dark,
        }
    }

    /// Read one from the command line, so a script can open the window in dark.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            _ => None,
        }
    }
}