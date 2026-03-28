/// Terminal color representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    /// 24-bit true color.
    Rgb(u8, u8, u8),
    /// 256-color palette index.
    Indexed(u8),
    /// Reset to terminal default.
    Reset,
}

/// Text style modifiers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Modifier {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub undercurl: bool,
    pub strikethrough: bool,
    pub dim: bool,
}

/// Complete style: foreground, background, and modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Style {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub modifier: Modifier,
    /// Underline/undercurl color (for diagnostics).
    pub underline_color: Option<Color>,
}

impl Style {
    #[must_use]
    pub const fn fg(mut self, color: Color) -> Self {
        self.fg = Some(color);
        self
    }

    #[must_use]
    pub const fn bg(mut self, color: Color) -> Self {
        self.bg = Some(color);
        self
    }

    /// Merge another style on top. Non-None fields in `other`
    /// override `self`.
    #[must_use]
    pub fn merge(self, other: Self) -> Self {
        Self {
            fg: other.fg.or(self.fg),
            bg: other.bg.or(self.bg),
            modifier: Modifier {
                bold: other.modifier.bold || self.modifier.bold,
                italic: other.modifier.italic || self.modifier.italic,
                underline: other.modifier.underline || self.modifier.underline,
                undercurl: other.modifier.undercurl || self.modifier.undercurl,
                strikethrough: other.modifier.strikethrough
                    || self.modifier.strikethrough,
                dim: other.modifier.dim || self.modifier.dim,
            },
            underline_color: other.underline_color.or(self.underline_color),
        }
    }
}
