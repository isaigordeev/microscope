/// Vim editing mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    /// Ex command line (`:` prompt).
    Command,
}

impl Mode {
    /// Display name for mode indicator.
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Insert => "INSERT",
            Self::Command => "COMMAND",
        }
    }
}
