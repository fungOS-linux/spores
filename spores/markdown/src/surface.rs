//! Which host Soil launches. The Spore is identical on all of them.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    Web,
    Native,
    Canvas,
}

impl Surface {
    pub const ALL: [Self; 3] = [Self::Web, Self::Native, Self::Canvas];

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "web" => Some(Self::Web),
            "native" => Some(Self::Native),
            "canvas" => Some(Self::Canvas),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::Native => "native",
            Self::Canvas => "canvas",
        }
    }
}
