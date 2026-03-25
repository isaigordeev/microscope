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
    /// Desired column for vertical movement (vim's
    /// `w_curswant`). When moving j/k, the cursor tries
    /// to return to this column.
    pub desired_col: usize,
    /// Number of visible rows in the viewport.
    pub height: u16,
}

impl View {
    pub const fn new(height: u16) -> Self {
        Self {
            scroll_offset: 0,
            cursor_line: 0,
            cursor_col: 0,
            desired_col: 0,
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

        if self.cursor_line
            < self.scroll_offset + scrolloff
        {
            self.scroll_offset =
                self.cursor_line.saturating_sub(scrolloff);
        }

        if self.cursor_line + scrolloff
            >= self.scroll_offset + h
        {
            self.scroll_offset =
                (self.cursor_line + scrolloff + 1)
                    .saturating_sub(h);
        }
    }

    /// Move cursor down, clamped to `max_line`.
    /// Uses `desired_col` for column stickiness.
    pub fn move_down(
        &mut self,
        max_line: usize,
        line_len_fn: impl Fn(usize) -> usize,
    ) {
        if self.cursor_line < max_line {
            self.cursor_line += 1;
            self.cursor_col = self
                .desired_col
                .min(line_len_fn(self.cursor_line));
        }
        self.ensure_cursor_visible();
    }

    /// Move cursor up. Uses `desired_col` for column
    /// stickiness.
    pub fn move_up(
        &mut self,
        line_len_fn: impl Fn(usize) -> usize,
    ) {
        if self.cursor_line > 0 {
            self.cursor_line -= 1;
            self.cursor_col = self
                .desired_col
                .min(line_len_fn(self.cursor_line));
        }
        self.ensure_cursor_visible();
    }

    /// Move cursor left.
    pub const fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
            self.desired_col = self.cursor_col;
        }
    }

    /// Move cursor right, clamped to `max_col`.
    pub const fn move_right(&mut self, max_col: usize) {
        if self.cursor_col < max_col {
            self.cursor_col += 1;
            self.desired_col = self.cursor_col;
        }
    }

    /// Move to column 0 (vim `0`).
    pub const fn move_to_line_start(&mut self) {
        self.cursor_col = 0;
        self.desired_col = 0;
    }

    /// Move to end of line (vim `$`).
    pub const fn move_to_line_end(&mut self, line_len: usize) {
        self.cursor_col =
            if line_len == 0 { 0 } else { line_len - 1 };
        self.desired_col = usize::MAX;
    }

    /// Move to first non-blank (vim `^`).
    pub const fn move_to_first_non_blank(
        &mut self,
        col: usize,
    ) {
        self.cursor_col = col;
        self.desired_col = col;
    }

    /// Set column and update `desired_col`.
    pub const fn set_col(&mut self, col: usize) {
        self.cursor_col = col;
        self.desired_col = col;
    }

    /// Screen row for the cursor (relative to viewport
    /// top).
    pub const fn cursor_screen_row(&self) -> u16 {
        (self.cursor_line - self.scroll_offset) as u16
    }
}