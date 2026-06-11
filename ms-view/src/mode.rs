/// Visual mode flavour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualKind {
    /// Charwise (`v`).
    Char,
    /// Linewise (`V`).
    Line,
    /// Blockwise (`Ctrl-V`) — defined for forward
    /// compatibility, not bound yet.
    Block,
}

/// Vim editing mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    /// Ex command line (`:` prompt).
    Command,
    /// Visual selection. `anchor` is the fixed end of
    /// the selection as a char index; the cursor is
    /// the moving head.
    Visual {
        kind: VisualKind,
        anchor: usize,
    },
}

impl Mode {
    /// Display name for mode indicator.
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Insert => "INSERT",
            Self::Command => "COMMAND",
            Self::Visual { kind: VisualKind::Char, .. } => "VISUAL",
            Self::Visual { kind: VisualKind::Line, .. } => "VISUAL LINE",
            Self::Visual { kind: VisualKind::Block, .. } => "VISUAL BLOCK",
        }
    }

    /// Whether this is any visual mode.
    #[must_use]
    pub const fn is_visual(self) -> bool {
        matches!(self, Self::Visual { .. })
    }
}
