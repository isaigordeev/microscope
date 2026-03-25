use crate::document::Document;
use crate::view::View;

/// Global editor state.
#[derive(Debug)]
pub struct Editor {
    pub document: Document,
    pub view: View,
    pub should_quit: bool,
}

impl Editor {
    pub const fn new(document: Document, height: u16) -> Self {
        Self {
            document,
            view: View::new(height),
            should_quit: false,
        }
    }

    /// Max line index for cursor clamping.
    pub fn max_line(&self) -> usize {
        self.document.line_count().saturating_sub(1)
    }
}