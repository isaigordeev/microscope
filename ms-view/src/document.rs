use std::io::Write;
use std::path::PathBuf;

use ms_core::rope;
use ms_core::transaction::{Transaction, TransactionError};
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
        Ok(Self { text, path: Some(path.to_path_buf()), modified: false })
    }

    /// Create an empty scratch document.
    pub fn scratch() -> Self {
        Self { text: Rope::new(), path: None, modified: false }
    }

    pub fn line_count(&self) -> usize {
        self.text.len_lines()
    }

    /// Get the text of a line (0-indexed). Returns None
    /// if out of range.
    pub fn line(&self, idx: usize) -> Option<ropey::RopeSlice<'_>> {
        if idx < self.text.len_lines() {
            Some(self.text.line(idx))
        } else {
            None
        }
    }

    /// Length of a line in chars (excluding trailing
    /// newline).
    pub fn line_len(&self, idx: usize) -> usize {
        self.line(idx).map_or(0, |l| {
            let s: String = l.chars().collect();
            s.trim_end_matches('\n').chars().count()
        })
    }

    /// Character offset for (line, col).
    pub fn line_col_to_char(&self, line: usize, col: usize) -> usize {
        if line >= self.text.len_lines() {
            return self.text.len_chars();
        }
        let line_start = self.text.line_to_char(line);
        let max_col = self.line_len(line);
        line_start + col.min(max_col)
    }

    /// Convert a character offset to (line, col).
    pub fn char_to_line_col(&self, char_idx: usize) -> (usize, usize) {
        let char_idx = char_idx.min(self.text.len_chars());
        let line = self.text.char_to_line(char_idx);
        let line_start = self.text.line_to_char(line);
        (line, char_idx - line_start)
    }

    /// Apply a transaction to this document.
    ///
    /// # Errors
    /// Returns `TransactionError` if the transaction's
    /// offsets are invalid.
    pub fn apply_transaction(
        &mut self,
        txn: &Transaction,
    ) -> Result<(), TransactionError> {
        txn.apply(&mut self.text)?;
        self.modified = true;
        Ok(())
    }

    /// Save the document to its file path.
    ///
    /// # Errors
    /// Returns IO error if writing fails, or if there's
    /// no path set.
    pub fn save(&mut self) -> std::io::Result<()> {
        let path = self
            .path
            .as_ref()
            .ok_or_else(|| std::io::Error::other("no file path set"))?;
        let file = std::fs::File::create(path)?;
        let mut writer = std::io::BufWriter::new(file);
        for chunk in self.text.chunks() {
            writer.write_all(chunk.as_bytes())?;
        }
        writer.flush()?;
        self.modified = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ms_core::transaction::Transaction;

    fn doc(text: &str) -> Document {
        Document { text: Rope::from(text), path: None, modified: false }
    }

    #[test]
    fn scratch_is_empty() {
        let d = Document::scratch();
        assert_eq!(d.text.len_chars(), 0);
        assert!(!d.modified);
        assert!(d.path.is_none());
    }

    #[test]
    fn line_count() {
        let d = doc("hello\nworld\n");
        assert_eq!(d.line_count(), 3);
    }

    #[test]
    fn line_len_excludes_newline() {
        let d = doc("hello\nhi\n");
        assert_eq!(d.line_len(0), 5);
        assert_eq!(d.line_len(1), 2);
    }

    #[test]
    fn line_col_to_char_and_back() {
        let d = doc("hello\nworld\nfoo");
        // 'w' is at line 1, col 0 → char 6
        assert_eq!(d.line_col_to_char(1, 0), 6);
        assert_eq!(d.char_to_line_col(6), (1, 0));
        // 'o' in "foo" is at line 2, col 1 → char 13
        assert_eq!(d.line_col_to_char(2, 1), 13);
        assert_eq!(d.char_to_line_col(13), (2, 1));
    }

    #[test]
    fn line_col_clamps_past_end() {
        let d = doc("hi");
        // col 100 on line 0 should clamp to len
        assert_eq!(d.line_col_to_char(0, 100), 2);
        // line past end should return len_chars
        assert_eq!(d.line_col_to_char(99, 0), d.text.len_chars(),);
    }

    #[test]
    fn apply_transaction_marks_modified() {
        let mut d = doc("hello");
        assert!(!d.modified);
        let txn = Transaction::insert(5, " world");
        d.apply_transaction(&txn).ok();
        assert!(d.modified);
        assert_eq!(d.text.to_string(), "hello world");
    }

    #[test]
    fn line_out_of_range_returns_none() {
        let d = doc("hello");
        assert!(d.line(0).is_some());
        assert!(d.line(99).is_none());
    }

    #[test]
    fn line_len_out_of_range_returns_zero() {
        let d = doc("hello");
        assert_eq!(d.line_len(99), 0);
    }
}
