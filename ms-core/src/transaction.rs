use ropey::Rope;

/// A single text operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    /// Keep `n` characters unchanged.
    Retain(usize),
    /// Delete `n` characters.
    Delete(usize),
    /// Insert text.
    Insert(String),
}

/// An atomic, composable text change. All edits to a
/// document go through transactions — never mutate the
/// rope directly.
#[derive(Debug, Clone)]
pub struct Transaction {
    ops: Vec<Operation>,
    /// Cursor position (line, col) after applying.
    pub cursor_after: Option<(usize, usize)>,
}

impl Transaction {
    pub const fn new(ops: Vec<Operation>) -> Self {
        Self { ops, cursor_after: None }
    }

    #[must_use]
    pub const fn with_cursor(mut self, line: usize, col: usize) -> Self {
        self.cursor_after = Some((line, col));
        self
    }

    /// Insert text at a character offset.
    pub fn insert(pos: usize, text: &str) -> Self {
        let mut ops = Vec::new();
        if pos > 0 {
            ops.push(Operation::Retain(pos));
        }
        ops.push(Operation::Insert(text.to_owned()));
        Self::new(ops)
    }

    /// Delete `len` characters starting at `pos`.
    pub fn delete(pos: usize, len: usize) -> Self {
        let mut ops = Vec::new();
        if pos > 0 {
            ops.push(Operation::Retain(pos));
        }
        if len > 0 {
            ops.push(Operation::Delete(len));
        }
        Self::new(ops)
    }

    /// Replace `len` characters at `pos` with `text`.
    pub fn replace(pos: usize, len: usize, text: &str) -> Self {
        let mut ops = Vec::new();
        if pos > 0 {
            ops.push(Operation::Retain(pos));
        }
        if len > 0 {
            ops.push(Operation::Delete(len));
        }
        if !text.is_empty() {
            ops.push(Operation::Insert(text.to_owned()));
        }
        Self::new(ops)
    }

    /// Apply this transaction to a rope.
    ///
    /// # Errors
    /// Returns an error if offsets exceed rope length.
    pub fn apply(&self, rope: &mut Rope) -> Result<(), TransactionError> {
        let mut pos: usize = 0;
        for op in &self.ops {
            match op {
                Operation::Retain(n) => {
                    let new_pos = pos + n;
                    if new_pos > rope.len_chars() {
                        return Err(TransactionError::OutOfBounds {
                            offset: new_pos,
                            len: rope.len_chars(),
                        });
                    }
                    pos = new_pos;
                }
                Operation::Delete(n) => {
                    let end = pos + n;
                    if end > rope.len_chars() {
                        return Err(TransactionError::OutOfBounds {
                            offset: end,
                            len: rope.len_chars(),
                        });
                    }
                    rope.remove(pos..end);
                    // pos stays the same after delete
                }
                Operation::Insert(text) => {
                    rope.insert(pos, text);
                    pos += text.len();
                }
            }
        }
        Ok(())
    }

    /// Create the inverse transaction (for undo).
    /// Requires the rope state *before* this transaction
    /// was applied.
    #[must_use]
    pub fn invert(&self, rope: &Rope) -> Self {
        let mut inv_ops = Vec::new();
        let mut pos: usize = 0;

        for op in &self.ops {
            match op {
                Operation::Retain(n) => {
                    inv_ops.push(Operation::Retain(*n));
                    pos += n;
                }
                Operation::Delete(n) => {
                    // To invert a delete, we insert the
                    // deleted text back.
                    let end = (pos + n).min(rope.len_chars());
                    let deleted: String =
                        rope.slice(pos..end).chars().collect();
                    inv_ops.push(Operation::Insert(deleted));
                    pos += n;
                }
                Operation::Insert(text) => {
                    // To invert an insert, we delete the
                    // inserted length.
                    inv_ops.push(Operation::Delete(text.chars().count()));
                    // pos does NOT advance in the
                    // original rope for inserts
                }
            }
        }

        Self::new(inv_ops)
    }
}

/// Errors from transaction application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionError {
    OutOfBounds { offset: usize, len: usize },
}

impl std::fmt::Display for TransactionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutOfBounds { offset, len } => {
                write!(
                    f,
                    "offset {offset} exceeds \
                     document length {len}"
                )
            }
        }
    }
}

impl std::error::Error for TransactionError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_at_start() {
        let mut rope = Rope::from("hello");
        let txn = Transaction::insert(0, "world ");
        txn.apply(&mut rope).ok();
        assert_eq!(rope.to_string(), "world hello");
    }

    #[test]
    fn insert_at_end() {
        let mut rope = Rope::from("hello");
        let txn = Transaction::insert(5, " world");
        txn.apply(&mut rope).ok();
        assert_eq!(rope.to_string(), "hello world");
    }

    #[test]
    fn delete_middle() {
        let mut rope = Rope::from("hello world");
        let txn = Transaction::delete(5, 6);
        txn.apply(&mut rope).ok();
        assert_eq!(rope.to_string(), "hello");
    }

    #[test]
    fn replace_text() {
        let mut rope = Rope::from("hello world");
        let txn = Transaction::replace(6, 5, "rust");
        txn.apply(&mut rope).ok();
        assert_eq!(rope.to_string(), "hello rust");
    }

    #[test]
    fn invert_insert() {
        let mut rope = Rope::from("hello");
        let txn = Transaction::insert(5, " world");
        let inv = txn.invert(&rope);
        txn.apply(&mut rope).ok();
        assert_eq!(rope.to_string(), "hello world");
        inv.apply(&mut rope).ok();
        assert_eq!(rope.to_string(), "hello");
    }

    #[test]
    fn invert_delete() {
        let mut rope = Rope::from("hello world");
        let txn = Transaction::delete(5, 6);
        let inv = txn.invert(&rope);
        txn.apply(&mut rope).ok();
        assert_eq!(rope.to_string(), "hello");
        inv.apply(&mut rope).ok();
        assert_eq!(rope.to_string(), "hello world");
    }

    #[test]
    fn out_of_bounds() {
        let mut rope = Rope::from("hi");
        let txn = Transaction::delete(0, 10);
        let result = txn.apply(&mut rope);
        assert!(result.is_err());
    }
}
