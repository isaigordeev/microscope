use std::path::PathBuf;

use ms_core::rope;
use ropey::Rope;

/// A document: text content + metadata.
#[derive(Debug)]
pub struct Document {
    pub text: Rope,
    pub path: Option<PathBuf>,
    pub modified: bool,
}

impl Document {
    /// Open a file into a document.
    ///
    /// # Errors
    /// Returns IO error if file cannot be read.
    pub fn open(path: &std::path::Path) -> std::io::Result<Self> {
        let text = rope::from_file(path)?;
        Ok(Self {
            text,
            path: Some(path.to_path_buf()),
            modified: false,
        })
    }

    /// Create an empty scratch document.
    pub fn scratch() -> Self {
        Self {
            text: Rope::new(),
            path: None,
            modified: false,
        }
    }

    pub fn line_count(&self) -> usize {
        self.text.len_lines()
    }

    /// Get the text of a line (0-indexed). Returns None if
    /// out of range.
    pub fn line(&self, idx: usize) -> Option<ropey::RopeSlice<'_>> {
        if idx < self.text.len_lines() {
            Some(self.text.line(idx))
        } else {
            None
        }
    }
}