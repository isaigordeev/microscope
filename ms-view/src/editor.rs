use std::collections::HashMap;

use ms_core::history::History;

use crate::command::{Motion, VimMachine};
use crate::config::Config;
use crate::document::Document;
use crate::mode::Mode;
use crate::register::Registers;
use crate::theme::Theme;
use crate::view::View;

/// Committed search state (vim `/`, `?`, `n`, `*`).
#[derive(Debug, Default)]
pub struct SearchState {
    /// Last committed pattern (regex source).
    pub pattern: String,
    /// Direction of the last search command.
    pub backward: bool,
    /// Whether match highlighting is on
    /// (Esc = :nohlsearch turns it off).
    pub active: bool,
}

/// A buffer that is open but not currently shown.
/// Carries everything buffer-local: text, view
/// position, undo history and marks.
#[derive(Debug)]
pub struct StashedBuffer {
    pub document: Document,
    cursor_line: usize,
    cursor_col: usize,
    scroll_offset: usize,
    history: History,
    marks: HashMap<char, usize>,
}

/// One row of `:ls` / the buffer picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferInfo {
    /// 0-based position in the open order.
    pub index: usize,
    pub path: Option<std::path::PathBuf>,
    pub modified: bool,
    pub current: bool,
}

/// Global editor state.
#[derive(Debug)]
pub struct Editor {
    pub document: Document,
    pub view: View,
    pub mode: Mode,
    pub should_quit: bool,
    /// Command line buffer (for `:` and search
    /// prompts).
    pub command_buffer: String,
    /// Status message (shown at bottom, clears on next key).
    pub status_message: Option<String>,
    /// Vim grammar state machine.
    pub vim: VimMachine,
    /// Named registers for yank/paste.
    pub registers: Registers,
    /// Undo/redo history.
    pub history: History,
    /// Active yank register (default `"`).
    pub yank_register: char,
    /// Active color theme.
    pub theme: Theme,
    /// Search state (`/`, `?`, `n`, `N`, `*`, `#`).
    pub search: SearchState,
    /// Cursor position when the search prompt opened
    /// (incremental search jumps from here; Esc
    /// restores it).
    pub search_origin: usize,
    /// Marks (`m{a-z}`) as char positions. Not yet
    /// adjusted on edits.
    pub marks: HashMap<char, usize>,
    /// Last `f`/`F`/`t`/`T` motion, for `;` and `,`.
    pub last_find: Option<Motion>,
    /// Settings (config.toml / .microscope.toml /
    /// `:set`).
    pub config: Config,
    /// Executed `:` commands, oldest first.
    pub command_history: Vec<String>,
    /// Position while cycling history with Up/Down.
    pub history_pos: Option<usize>,
    /// Open buffers other than the displayed one, in
    /// open order (the displayed document sits at
    /// `buffer_index` in the conceptual list).
    stashed: Vec<StashedBuffer>,
    /// Position of the displayed document in the
    /// conceptual buffer list.
    buffer_index: usize,
}

impl Editor {
    pub fn new(document: Document, height: u16) -> Self {
        Self {
            document,
            view: View::new(height),
            mode: Mode::Normal,
            should_quit: false,
            command_buffer: String::new(),
            status_message: None,
            vim: VimMachine::new(),
            registers: Registers::new(),
            history: History::new(),
            yank_register: '"',
            theme: Theme::default(),
            search: SearchState::default(),
            search_origin: 0,
            marks: HashMap::new(),
            last_find: None,
            config: Config::default(),
            command_history: Vec::new(),
            history_pos: None,
            stashed: Vec::new(),
            buffer_index: 0,
        }
    }

    /// Max line index for cursor clamping.
    pub fn max_line(&self) -> usize {
        self.document.line_count().saturating_sub(1)
    }

    /// Length of the line at cursor (excluding newline).
    pub fn current_line_len(&self) -> usize {
        self.document.line(self.view.cursor_line).map_or(0, |l| {
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
            // Normal/Visual: cursor is ON a char, can't
            // go past last char.
            Mode::Normal | Mode::Visual { .. } => {
                if line_len == 0 {
                    0
                } else {
                    line_len - 1
                }
            }
            // Insert/Command/Search: cursor can be
            // after last char.
            Mode::Insert | Mode::Command | Mode::Search { .. } => line_len,
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
        self.view.cursor_col = self.first_non_blank_col(self.view.cursor_line);
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
        self.history_pos = None;
    }

    // ── Buffer management ─────────────────────────

    /// Total number of open buffers.
    #[must_use]
    pub const fn buffer_count(&self) -> usize {
        self.stashed.len() + 1
    }

    /// Position of the displayed buffer in the open
    /// order.
    #[must_use]
    pub const fn buffer_index(&self) -> usize {
        self.buffer_index
    }

    /// All open buffers in open order (`:ls`, picker).
    #[must_use]
    pub fn buffer_infos(&self) -> Vec<BufferInfo> {
        (0..self.buffer_count())
            .map(|index| {
                if index == self.buffer_index {
                    BufferInfo {
                        index,
                        path: self.document.path.clone(),
                        modified: self.document.modified,
                        current: true,
                    }
                } else {
                    let buf = &self.stashed[self.stash_slot(index)];
                    BufferInfo {
                        index,
                        path: buf.document.path.clone(),
                        modified: buf.document.modified,
                        current: false,
                    }
                }
            })
            .collect()
    }

    /// Whether any open buffer has unsaved changes.
    #[must_use]
    pub fn any_modified(&self) -> bool {
        self.document.modified
            || self.stashed.iter().any(|b| b.document.modified)
    }

    /// `stashed` slot for a conceptual list position
    /// (must not be `buffer_index`).
    const fn stash_slot(&self, index: usize) -> usize {
        if index < self.buffer_index {
            index
        } else {
            index - 1
        }
    }

    /// Park the displayed document and its
    /// buffer-local state, leaving `replacement`
    /// displayed.
    fn stash_current(&mut self, replacement: Document) -> StashedBuffer {
        StashedBuffer {
            document: std::mem::replace(&mut self.document, replacement),
            cursor_line: self.view.cursor_line,
            cursor_col: self.view.cursor_col,
            scroll_offset: self.view.scroll_offset,
            history: std::mem::take(&mut self.history),
            marks: std::mem::take(&mut self.marks),
        }
    }

    /// Display a stashed buffer (assumes the previous
    /// document was already stashed or replaced).
    fn restore(&mut self, buf: StashedBuffer) {
        self.document = buf.document;
        self.history = buf.history;
        self.marks = buf.marks;
        self.view.cursor_line = buf.cursor_line.min(self.max_line());
        self.view.cursor_col = buf.cursor_col;
        self.view.desired_col = buf.cursor_col;
        self.view.scroll_offset = buf.scroll_offset;
        self.clamp_cursor_col();
        self.view.ensure_cursor_visible();
    }

    /// Reset view + buffer-local state for a freshly
    /// opened document.
    fn reset_buffer_state(&mut self) {
        self.history = History::new();
        self.marks = HashMap::new();
        self.view.cursor_line = 0;
        self.view.cursor_col = 0;
        self.view.desired_col = 0;
        self.view.scroll_offset = 0;
    }

    /// Open a document as a new buffer. Reuses an
    /// existing buffer with the same path (vim `:e`
    /// on an already-open file). An empty pristine
    /// scratch buffer is replaced instead of kept.
    pub fn open_document(&mut self, doc: Document) {
        if let Some(path) = &doc.path {
            if self.document.path.as_deref() == Some(path) {
                return;
            }
            let existing = self
                .buffer_infos()
                .into_iter()
                .find(|b| b.path.as_deref() == Some(path.as_path()));
            if let Some(info) = existing {
                self.switch_buffer(info.index);
                return;
            }
        }
        let pristine_scratch = self.document.path.is_none()
            && !self.document.modified
            && self.document.text.len_chars() == 0;
        if !pristine_scratch {
            let stashed = self.stash_current(Document::scratch());
            self.stashed.push(stashed);
            self.buffer_index = self.stashed.len();
        }
        self.document = doc;
        self.reset_buffer_state();
    }

    /// Switch to the buffer at `index` in the open
    /// order. Returns false when out of range.
    pub fn switch_buffer(&mut self, index: usize) -> bool {
        if index >= self.buffer_count() {
            return false;
        }
        if index == self.buffer_index {
            return true;
        }
        let slot = self.stash_slot(index);
        let target = self.stashed.remove(slot);
        let insert_at = if index < self.buffer_index {
            self.buffer_index - 1
        } else {
            self.buffer_index
        };
        let stashed = self.stash_current(Document::scratch());
        self.stashed.insert(insert_at, stashed);
        self.buffer_index = index;
        self.restore(target);
        true
    }

    /// Cycle to the next buffer (vim `:bn`).
    pub fn next_buffer(&mut self) {
        let next = (self.buffer_index + 1) % self.buffer_count();
        self.switch_buffer(next);
    }

    /// Cycle to the previous buffer (vim `:bp`).
    pub fn prev_buffer(&mut self) {
        let count = self.buffer_count();
        let prev = (self.buffer_index + count - 1) % count;
        self.switch_buffer(prev);
    }

    /// Close the displayed buffer (vim `:bd`).
    ///
    /// # Errors
    /// Refuses when the buffer is modified and `force`
    /// is false.
    pub fn close_buffer(&mut self, force: bool) -> Result<(), String> {
        if self.document.modified && !force {
            return Err("No write since last change \
                 (add ! to override)"
                .to_owned());
        }
        if self.stashed.is_empty() {
            self.document = Document::scratch();
            self.reset_buffer_state();
            self.buffer_index = 0;
            return Ok(());
        }
        let slot = self.buffer_index.min(self.stashed.len() - 1);
        let target = self.stashed.remove(slot);
        self.buffer_index = slot;
        self.restore(target);
        Ok(())
    }

    /// First non-blank column on a line.
    pub fn first_non_blank_col(&self, line: usize) -> usize {
        self.document.line(line).map_or(0, |l| {
            l.chars().take_while(|c| c.is_whitespace() && *c != '\n').count()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use ropey::Rope;

    fn editor(text: &str) -> Editor {
        let doc =
            Document { text: Rope::from(text), path: None, modified: false };
        Editor::new(doc, 24)
    }

    #[test]
    fn initial_mode_is_normal() {
        let e = editor("hello");
        assert_eq!(e.mode, Mode::Normal);
    }

    #[test]
    fn enter_insert_and_back() {
        let mut e = editor("hello");
        e.enter_insert();
        assert_eq!(e.mode, Mode::Insert);
        e.enter_normal();
        assert_eq!(e.mode, Mode::Normal);
    }

    #[test]
    fn enter_insert_after_advances_col() {
        let mut e = editor("hello");
        e.view.cursor_col = 2;
        e.enter_insert_after();
        assert_eq!(e.mode, Mode::Insert);
        assert_eq!(e.view.cursor_col, 3);
    }

    #[test]
    fn enter_insert_eol() {
        let mut e = editor("hello");
        e.enter_insert_eol();
        assert_eq!(e.mode, Mode::Insert);
        assert_eq!(e.view.cursor_col, 5);
    }

    #[test]
    fn enter_insert_bol_skips_whitespace() {
        let mut e = editor("    hello");
        e.enter_insert_bol();
        assert_eq!(e.mode, Mode::Insert);
        assert_eq!(e.view.cursor_col, 4);
    }

    #[test]
    fn enter_command_clears_buffer() {
        let mut e = editor("hello");
        e.command_buffer = "old".to_owned();
        e.enter_command();
        assert_eq!(e.mode, Mode::Command);
        assert!(e.command_buffer.is_empty());
    }

    #[test]
    fn clamp_cursor_col_normal_mode() {
        let mut e = editor("hello");
        e.view.cursor_col = 99;
        e.clamp_cursor_col();
        // Normal mode: max is len-1 = 4
        assert_eq!(e.view.cursor_col, 4);
    }

    #[test]
    fn clamp_cursor_col_insert_mode() {
        let mut e = editor("hello");
        e.mode = Mode::Insert;
        e.view.cursor_col = 99;
        e.clamp_cursor_col();
        // Insert mode: max is len = 5
        assert_eq!(e.view.cursor_col, 5);
    }

    #[test]
    fn max_line() {
        let e = editor("line1\nline2\nline3");
        assert_eq!(e.max_line(), 2);
    }

    #[test]
    fn first_non_blank_col_works() {
        let e = editor("  \thello");
        assert_eq!(e.first_non_blank_col(0), 3);
    }

    #[test]
    fn current_line_len_excludes_newline() {
        let e = editor("hello\nworld");
        assert_eq!(e.current_line_len(), 5);
    }

    fn doc_with_path(text: &str, path: &str) -> Document {
        Document {
            text: Rope::from(text),
            path: Some(path.into()),
            modified: false,
        }
    }

    #[test]
    fn open_document_stashes_current() {
        let mut e = editor("first");
        e.open_document(doc_with_path("second", "/tmp/b.txt"));
        assert_eq!(e.buffer_count(), 2);
        assert_eq!(e.buffer_index(), 1);
        assert_eq!(e.document.text.to_string(), "second");
    }

    #[test]
    fn pristine_scratch_is_replaced_not_kept() {
        let mut e = editor("");
        e.open_document(doc_with_path("second", "/tmp/b.txt"));
        assert_eq!(e.buffer_count(), 1);
    }

    #[test]
    fn open_same_path_reuses_buffer() {
        let mut e = editor("first");
        e.open_document(doc_with_path("second", "/tmp/b.txt"));
        e.open_document(doc_with_path("stale copy", "/tmp/b.txt"));
        assert_eq!(e.buffer_count(), 2);
        assert_eq!(e.document.text.to_string(), "second");
    }

    #[test]
    fn switch_restores_cursor() {
        let mut e = editor("one\ntwo\nthree");
        e.view.cursor_line = 2;
        e.view.cursor_col = 1;
        e.open_document(doc_with_path("other", "/tmp/b.txt"));
        assert_eq!(e.view.cursor_line, 0);
        assert!(e.switch_buffer(0));
        assert_eq!(e.view.cursor_line, 2);
        assert_eq!(e.view.cursor_col, 1);
        assert_eq!(e.document.text.to_string(), "one\ntwo\nthree");
    }

    #[test]
    fn next_prev_cycle_in_order() {
        let mut e = editor("a");
        e.open_document(doc_with_path("b", "/tmp/b.txt"));
        e.open_document(doc_with_path("c", "/tmp/c.txt"));
        assert_eq!(e.buffer_index(), 2);
        e.next_buffer();
        assert_eq!(e.buffer_index(), 0);
        e.prev_buffer();
        assert_eq!(e.buffer_index(), 2);
        e.prev_buffer();
        assert_eq!(e.buffer_index(), 1);
        assert_eq!(e.document.text.to_string(), "b");
    }

    #[test]
    fn close_buffer_switches_to_next() {
        let mut e = editor("a");
        e.open_document(doc_with_path("b", "/tmp/b.txt"));
        assert_eq!(e.close_buffer(false), Ok(()));
        assert_eq!(e.buffer_count(), 1);
        assert_eq!(e.document.text.to_string(), "a");
    }

    #[test]
    fn close_modified_refuses_without_force() {
        let mut e = editor("a");
        e.document.modified = true;
        assert!(e.close_buffer(false).is_err());
        assert_eq!(e.close_buffer(true), Ok(()));
        assert_eq!(e.document.text.to_string(), "");
    }

    #[test]
    fn close_last_buffer_leaves_scratch() {
        let mut e = editor("a");
        assert_eq!(e.close_buffer(false), Ok(()));
        assert_eq!(e.buffer_count(), 1);
        assert!(e.document.path.is_none());
    }

    #[test]
    fn any_modified_sees_stashed() {
        let mut e = editor("a");
        e.document.modified = true;
        e.open_document(doc_with_path("b", "/tmp/b.txt"));
        assert!(!e.document.modified);
        assert!(e.any_modified());
    }

    #[test]
    fn buffer_infos_in_open_order() {
        let mut e = editor("a");
        e.open_document(doc_with_path("b", "/tmp/b.txt"));
        e.switch_buffer(0);
        let infos = e.buffer_infos();
        assert_eq!(infos.len(), 2);
        assert!(infos[0].current);
        assert!(!infos[1].current);
        assert_eq!(
            infos[1].path.as_deref(),
            Some(std::path::Path::new("/tmp/b.txt")),
        );
    }

    #[test]
    fn enter_normal_clamps_col() {
        let mut e = editor("hello");
        e.mode = Mode::Insert;
        e.view.cursor_col = 5; // past last char
        e.enter_normal();
        assert_eq!(e.view.cursor_col, 4);
    }
}
