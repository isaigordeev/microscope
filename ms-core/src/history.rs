//! Undo/redo history tree.
//!
//! Stores revisions as a `Vec` with parent pointers.
//! Each revision holds both the forward transaction and
//! its inversion, so undo/redo is just applying the
//! appropriate one.

use std::num::NonZeroUsize;
use std::time::Instant;

use crate::transaction::Transaction;

/// A single revision in the history.
#[derive(Debug, Clone)]
struct Revision {
    /// Index of parent revision (0 = initial state).
    parent: usize,
    /// Index of last child (for redo).
    last_child: Option<NonZeroUsize>,
    /// Transaction to apply to go parent → this.
    transaction: Transaction,
    /// Transaction to apply to go this → parent (undo).
    inversion: Transaction,
    /// When this revision was created.
    #[allow(dead_code)]
    timestamp: Instant,
}

/// Undo/redo history.
///
/// Revision 0 is a sentinel (the initial document state).
/// `current` points to the active revision.
#[derive(Debug)]
pub struct History {
    revisions: Vec<Revision>,
    current: usize,
}

impl History {
    /// Create a new empty history.
    #[must_use]
    pub fn new() -> Self {
        // Sentinel revision 0: no-op, no parent.
        Self {
            revisions: vec![Revision {
                parent: 0,
                last_child: None,
                transaction: Transaction::new(vec![]),
                inversion: Transaction::new(vec![]),
                timestamp: Instant::now(),
            }],
            current: 0,
        }
    }

    /// Record a new revision. `transaction` is the forward
    /// change; `inversion` undoes it.
    pub fn commit(
        &mut self,
        transaction: Transaction,
        inversion: Transaction,
    ) {
        let new_idx = self.revisions.len();

        // Update parent's last_child to point to us.
        self.revisions[self.current].last_child = NonZeroUsize::new(new_idx);

        self.revisions.push(Revision {
            parent: self.current,
            last_child: None,
            transaction,
            inversion,
            timestamp: Instant::now(),
        });

        self.current = new_idx;
    }

    /// Undo: return the inversion transaction to apply.
    /// Returns `None` if already at the initial state.
    pub fn undo(&mut self) -> Option<&Transaction> {
        if self.current == 0 {
            return None;
        }
        let rev = &self.revisions[self.current];
        let txn = &rev.inversion;
        self.current = rev.parent;
        Some(txn)
    }

    /// Redo: return the forward transaction of the last
    /// child. Returns `None` if no redo available.
    pub fn redo(&mut self) -> Option<&Transaction> {
        let child_idx = self.revisions[self.current].last_child?.get();
        self.current = child_idx;
        Some(&self.revisions[child_idx].transaction)
    }

    /// Whether undo is possible.
    #[must_use]
    pub const fn can_undo(&self) -> bool {
        self.current != 0
    }

    /// Whether redo is possible.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.revisions[self.current].last_child.is_some()
    }

    /// Current revision index.
    #[must_use]
    pub const fn current(&self) -> usize {
        self.current
    }

    /// Number of revisions (including sentinel).
    #[must_use]
    pub const fn len(&self) -> usize {
        self.revisions.len()
    }

    /// Whether this history is empty (only sentinel).
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.revisions.len() <= 1
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaction::Transaction;
    use ropey::Rope;

    fn insert_txn(pos: usize, text: &str) -> (Transaction, Transaction) {
        let fwd = Transaction::insert(pos, text);
        let inv = Transaction::delete(pos, text.chars().count());
        (fwd, inv)
    }

    #[test]
    fn new_history_is_at_zero() {
        let h = History::new();
        assert_eq!(h.current(), 0);
        assert!(!h.can_undo());
        assert!(!h.can_redo());
        assert!(h.is_empty());
    }

    #[test]
    fn commit_and_undo() {
        let mut h = History::new();
        let mut rope = Rope::from("hello");

        let (fwd, inv) = insert_txn(5, " world");
        fwd.apply(&mut rope).ok();
        h.commit(fwd, inv);

        assert_eq!(rope.to_string(), "hello world");
        assert!(h.can_undo());
        assert_eq!(h.current(), 1);

        let undo_txn = h.undo().cloned();
        if let Some(txn) = undo_txn {
            txn.apply(&mut rope).ok();
        }
        assert_eq!(rope.to_string(), "hello");
        assert_eq!(h.current(), 0);
    }

    #[test]
    fn undo_and_redo() {
        let mut h = History::new();
        let mut rope = Rope::from("hello");

        let (fwd, inv) = insert_txn(5, " world");
        fwd.apply(&mut rope).ok();
        h.commit(fwd, inv);

        // Undo
        let undo_txn = h.undo().cloned();
        if let Some(txn) = undo_txn {
            txn.apply(&mut rope).ok();
        }
        assert_eq!(rope.to_string(), "hello");

        // Redo
        assert!(h.can_redo());
        let redo_txn = h.redo().cloned();
        if let Some(txn) = redo_txn {
            txn.apply(&mut rope).ok();
        }
        assert_eq!(rope.to_string(), "hello world");
    }

    #[test]
    fn multiple_commits() {
        let mut h = History::new();
        let mut rope = Rope::from("a");

        let (fwd, inv) = insert_txn(1, "b");
        fwd.apply(&mut rope).ok();
        h.commit(fwd, inv);

        let (fwd, inv) = insert_txn(2, "c");
        fwd.apply(&mut rope).ok();
        h.commit(fwd, inv);

        assert_eq!(rope.to_string(), "abc");
        assert_eq!(h.len(), 3); // sentinel + 2

        // Undo both
        let u1 = h.undo().cloned();
        if let Some(txn) = u1 {
            txn.apply(&mut rope).ok();
        }
        assert_eq!(rope.to_string(), "ab");

        let u2 = h.undo().cloned();
        if let Some(txn) = u2 {
            txn.apply(&mut rope).ok();
        }
        assert_eq!(rope.to_string(), "a");
        assert!(!h.can_undo());
    }

    #[test]
    fn branch_history() {
        let mut h = History::new();
        let mut rope = Rope::from("a");

        // Commit "b"
        let (fwd, inv) = insert_txn(1, "b");
        fwd.apply(&mut rope).ok();
        h.commit(fwd, inv);
        assert_eq!(rope.to_string(), "ab");

        // Undo
        let u = h.undo().cloned();
        if let Some(txn) = u {
            txn.apply(&mut rope).ok();
        }
        assert_eq!(rope.to_string(), "a");

        // Commit "c" (branch)
        let (fwd, inv) = insert_txn(1, "c");
        fwd.apply(&mut rope).ok();
        h.commit(fwd, inv);
        assert_eq!(rope.to_string(), "ac");

        // Redo goes to "c" branch (last child)
        let u = h.undo().cloned();
        if let Some(txn) = u {
            txn.apply(&mut rope).ok();
        }
        assert_eq!(rope.to_string(), "a");

        let r = h.redo().cloned();
        if let Some(txn) = r {
            txn.apply(&mut rope).ok();
        }
        assert_eq!(rope.to_string(), "ac");
    }
}
