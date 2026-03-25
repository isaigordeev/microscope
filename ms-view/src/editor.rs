use crate::document::Document;
use crate::mode::Mode;
use crate::view::View;

/// Global editor state.
#[derive(Debug)]
pub struct Editor {
    pub document: Document,
    pub view: View,
    pub mode: Mode,
    pub should_quit: bool,
    /// Command line buffer (for `:` prompt).
    pub command_buffer: String,
    /// Status message (shown at bottom, clears on next key).
    pub status_message: Option<String>,
}

impl Editor {
    pub const fn new(document: Document, height: u16) -> Self {
        Self {
            document,
            view: View::new(height),
            mode: Mode::Normal,
            should_quit: false,
            command_buffer: String::new(),
            status_message: None,
        }
    }

    /// Max line index for cursor clamping.
    pub fn max_line(&self) -> usize {
        self.document.line_count().saturating_sub(1)
    }

    /// Length of the line at cursor (excluding newline).
    pub fn current_line_len(&self) -> usize {
        self.document
            .line(self.view.cursor_line)
            .map_or(0, |l| {
                let s: String = l.chars().collect();
                let trimmed = s.trim_end_matches('\n');
                trimmed.chars().count()
            })
    }

    /// Clamp cursor column to valid range for current
    /// mode and line.
    pub fn clamp_cursor_col(&mut self) {
        let line_len = self.current_line_len();
        let max_col = match self.mode {
            // Normal mode: cursor is ON a char, can't go
            // past last char.
            Mode::Normal => {
                if line_len == 0 {
                    0
                } else {
                    line_len - 1
                }
            }
            // Insert/Command: cursor can be after last
            // char.
            Mode::Insert | Mode::Command => line_len,
        };
        if self.view.cursor_col > max_col {
            self.view.cursor_col = max_col;
        }
    }

    /// Enter insert mode at cursor position.
    pub const fn enter_insert(&mut self) {
        self.mode = Mode::Insert;
    }

    /// Enter insert mode after cursor (vim `a`).
    pub fn enter_insert_after(&mut self) {
        self.mode = Mode::Insert;
        let line_len = self.current_line_len();
        if self.view.cursor_col < line_len {
            self.view.cursor_col += 1;
        }
    }

    /// Enter insert at end of line (vim `A`).
    pub fn enter_insert_eol(&mut self) {
        self.mode = Mode::Insert;
        self.view.cursor_col = self.current_line_len();
    }

    /// Enter insert at first non-blank (vim `I`).
    pub fn enter_insert_bol(&mut self) {
        self.mode = Mode::Insert;
        self.view.cursor_col =
            self.first_non_blank_col(self.view.cursor_line);
    }

    /// Return to normal mode.
    pub fn enter_normal(&mut self) {
        self.mode = Mode::Normal;
        self.clamp_cursor_col();
    }

    /// Enter command mode.
    pub fn enter_command(&mut self) {
        self.mode = Mode::Command;
        self.command_buffer.clear();
    }

    /// First non-blank column on a line.
    pub fn first_non_blank_col(
        &self,
        line: usize,
    ) -> usize {
        self.document
            .line(line)
            .map_or(0, |l| {
                l.chars()
                    .take_while(|c| {
                        c.is_whitespace() && *c != '\n'
                    })
                    .count()
            })
    }
}