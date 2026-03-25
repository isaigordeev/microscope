/// Viewport into a document: tracks scroll position
/// and cursor.
#[derive(Debug)]
pub struct View {
    /// First visible line (0-indexed).
    pub scroll_offset: usize,
    /// Cursor line (0-indexed, document-relative).
    pub cursor_line: usize,
    /// Cursor column (0-indexed).
    pub cursor_col: usize,
    /// Number of visible rows in the viewport.
    pub height: u16,
}

impl View {
    pub const fn new(height: u16) -> Self {
        Self {
            scroll_offset: 0,
            cursor_line: 0,
            cursor_col: 0,
            height,
        }
    }

    /// Scroll so that the cursor is visible, respecting
    /// scrolloff (4 lines).
    pub const fn ensure_cursor_visible(&mut self) {
        let scrolloff: usize = 4;
        let h = self.height as usize;

        if h == 0 {
            return;
        }

        // Cursor above viewport
        if self.cursor_line
            < self.scroll_offset + scrolloff
        {
            self.scroll_offset =
                self.cursor_line.saturating_sub(scrolloff);
        }

        // Cursor below viewport
        if self.cursor_line + scrolloff
            >= self.scroll_offset + h
        {
            self.scroll_offset =
                (self.cursor_line + scrolloff + 1)
                    .saturating_sub(h);
        }
    }

    /// Move cursor down, clamped to `max_line`.
    pub const fn move_down(&mut self, max_line: usize) {
        if self.cursor_line < max_line {
            self.cursor_line += 1;
        }
        self.ensure_cursor_visible();
    }

    /// Move cursor up.
    pub const fn move_up(&mut self) {
        self.cursor_line =
            self.cursor_line.saturating_sub(1);
        self.ensure_cursor_visible();
    }

    /// Screen row for the cursor (relative to viewport top).
    pub const fn cursor_screen_row(&self) -> u16 {
        (self.cursor_line - self.scroll_offset) as u16
    }
}