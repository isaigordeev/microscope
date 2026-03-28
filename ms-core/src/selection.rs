//! Selection primitives: Range and Selection.
//!
//! A `Range` is a pair of character offsets (anchor, head)
//! into a document's rope. A `Selection` is one or more
//! ranges with a primary index.

/// A single selection range defined by anchor and head.
///
/// - `anchor`: the fixed end (where the selection started)
/// - `head`: the moving end (where the cursor is)
///
/// When anchor == head, this is a simple cursor position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub anchor: usize,
    pub head: usize,
}

impl Range {
    #[must_use]
    pub const fn new(anchor: usize, head: usize) -> Self {
        Self { anchor, head }
    }

    /// Create a cursor (zero-width range) at `pos`.
    #[must_use]
    pub const fn point(pos: usize) -> Self {
        Self { anchor: pos, head: pos }
    }

    /// The lesser of anchor/head.
    #[must_use]
    pub const fn from(&self) -> usize {
        if self.anchor <= self.head {
            self.anchor
        } else {
            self.head
        }
    }

    /// The greater of anchor/head.
    #[must_use]
    pub const fn to(&self) -> usize {
        if self.anchor >= self.head {
            self.anchor
        } else {
            self.head
        }
    }

    /// Whether this is a zero-width cursor.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// Number of characters spanned.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.to() - self.from()
    }

    /// Whether the range contains a character offset.
    #[must_use]
    pub const fn contains(&self, pos: usize) -> bool {
        pos >= self.from() && pos < self.to()
    }

    /// Whether head is before anchor (backward selection).
    #[must_use]
    pub const fn is_backward(&self) -> bool {
        self.head < self.anchor
    }
}

/// A set of one or more selection ranges with a primary.
///
/// Invariant: ranges is never empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    ranges: Vec<Range>,
    primary_index: usize,
}

impl Selection {
    /// Create a selection with a single range.
    #[must_use]
    pub fn single(anchor: usize, head: usize) -> Self {
        Self { ranges: vec![Range::new(anchor, head)], primary_index: 0 }
    }

    /// Create a selection from a cursor position.
    #[must_use]
    pub fn point(pos: usize) -> Self {
        Self { ranges: vec![Range::point(pos)], primary_index: 0 }
    }

    /// The primary range (the main cursor).
    #[must_use]
    pub fn primary(&self) -> Range {
        self.ranges[self.primary_index]
    }

    /// The primary range's head position.
    #[must_use]
    pub fn cursor(&self) -> usize {
        self.primary().head
    }

    /// All ranges.
    #[must_use]
    pub fn ranges(&self) -> &[Range] {
        &self.ranges
    }

    /// Number of ranges.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.ranges.len()
    }

    /// Whether this selection has only one range.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    /// Apply a function to each range, producing a new
    /// selection.
    #[must_use]
    pub fn transform<F>(&self, f: F) -> Self
    where
        F: Fn(Range) -> Range,
    {
        Self {
            ranges: self.ranges.iter().copied().map(f).collect(),
            primary_index: self.primary_index,
        }
    }

    /// Set the primary cursor to a new position.
    #[must_use]
    pub fn with_cursor(mut self, pos: usize) -> Self {
        self.ranges[self.primary_index] = Range::point(pos);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_point() {
        let r = Range::point(5);
        assert_eq!(r.anchor, 5);
        assert_eq!(r.head, 5);
        assert!(r.is_empty());
        assert_eq!(r.len(), 0);
    }

    #[test]
    fn range_forward() {
        let r = Range::new(2, 8);
        assert_eq!(r.from(), 2);
        assert_eq!(r.to(), 8);
        assert!(!r.is_backward());
        assert_eq!(r.len(), 6);
    }

    #[test]
    fn range_backward() {
        let r = Range::new(8, 2);
        assert_eq!(r.from(), 2);
        assert_eq!(r.to(), 8);
        assert!(r.is_backward());
    }

    #[test]
    fn range_contains() {
        let r = Range::new(2, 6);
        assert!(r.contains(2));
        assert!(r.contains(5));
        assert!(!r.contains(6)); // exclusive end
        assert!(!r.contains(1));
    }

    #[test]
    fn selection_single() {
        let s = Selection::single(0, 5);
        assert_eq!(s.cursor(), 5);
        assert_eq!(s.len(), 1);
        assert_eq!(s.primary(), Range::new(0, 5));
    }

    #[test]
    fn selection_point() {
        let s = Selection::point(10);
        assert_eq!(s.cursor(), 10);
        assert!(s.primary().is_empty());
    }

    #[test]
    fn selection_transform() {
        let s = Selection::single(0, 5);
        let moved = s.transform(|r| Range::new(r.anchor + 1, r.head + 1));
        assert_eq!(moved.primary(), Range::new(1, 6));
    }

    #[test]
    fn selection_with_cursor() {
        let s = Selection::point(5).with_cursor(10);
        assert_eq!(s.cursor(), 10);
    }
}
