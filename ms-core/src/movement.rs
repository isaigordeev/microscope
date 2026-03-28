//! Cursor motion functions operating on a Rope.
//!
//! Pure functions — no editor/view dependency. Each takes
//! a rope + position and returns a new position.

use ropey::Rope;

// ── Character classification ──────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharCat {
    Word,
    Punct,
    Whitespace,
}

/// Classify a character. When `big` is true, all
/// non-whitespace is Word (vim's WORD).
pub fn char_category(c: char, big: bool) -> CharCat {
    if c.is_whitespace() {
        CharCat::Whitespace
    } else if big || c.is_alphanumeric() || c == '_' {
        CharCat::Word
    } else {
        CharCat::Punct
    }
}

// ── Word motions ──────────────────────────────────

/// Next word start (vim `w`/`W`).
pub fn next_word_start(text: &Rope, pos: usize, big: bool) -> usize {
    let len = text.len_chars();
    if pos >= len {
        return pos;
    }
    let mut i = pos;
    let cat = char_category(text.char(i), big);

    while i < len && char_category(text.char(i), big) == cat {
        i += 1;
    }
    while i < len && text.char(i).is_whitespace() {
        i += 1;
    }
    i.min(len.saturating_sub(1))
}

/// Previous word start (vim `b`/`B`).
pub fn prev_word_start(text: &Rope, pos: usize, big: bool) -> usize {
    if pos == 0 {
        return 0;
    }
    let mut i = pos - 1;

    while i > 0 && text.char(i).is_whitespace() {
        i -= 1;
    }

    let cat = char_category(text.char(i), big);

    while i > 0 && char_category(text.char(i - 1), big) == cat {
        i -= 1;
    }
    i
}

/// Next word end (vim `e`/`E`).
pub fn next_word_end(text: &Rope, pos: usize, big: bool) -> usize {
    let len = text.len_chars();
    if pos + 1 >= len {
        return pos;
    }
    let mut i = pos + 1;

    while i < len && text.char(i).is_whitespace() {
        i += 1;
    }

    if i >= len {
        return len.saturating_sub(1);
    }

    let cat = char_category(text.char(i), big);

    while i + 1 < len && char_category(text.char(i + 1), big) == cat {
        i += 1;
    }
    i.min(len.saturating_sub(1))
}

// ── Paragraph motions ─────────────────────────────

/// Next blank-line boundary (vim `}`).
pub fn paragraph_forward(text: &Rope, pos: usize) -> usize {
    let line_count = text.len_lines();
    let mut line = text.char_to_line(pos);

    while line < line_count && !is_blank_line(text, line) {
        line += 1;
    }
    while line < line_count && is_blank_line(text, line) {
        line += 1;
    }

    if line >= line_count {
        text.len_chars().saturating_sub(1)
    } else {
        text.line_to_char(line)
    }
}

/// Previous blank-line boundary (vim `{`).
pub fn paragraph_backward(text: &Rope, pos: usize) -> usize {
    if pos == 0 {
        return 0;
    }
    let mut line = text.char_to_line(pos);

    line = line.saturating_sub(1);

    while line > 0 && !is_blank_line(text, line) {
        line -= 1;
    }
    while line > 0 && is_blank_line(text, line) {
        line -= 1;
    }
    while line > 0 && !is_blank_line(text, line - 1) {
        line -= 1;
    }

    text.line_to_char(line)
}

fn is_blank_line(text: &Rope, line: usize) -> bool {
    let start = text.line_to_char(line);
    let end = if line + 1 < text.len_lines() {
        text.line_to_char(line + 1)
    } else {
        text.len_chars()
    };
    (start..end).all(|i| text.char(i).is_whitespace())
}

// ── Bracket matching ──────────────────────────────

/// Find matching bracket at `pos` (vim `%`).
/// Returns `None` if not on a bracket.
pub fn find_matching_bracket(text: &Rope, pos: usize) -> Option<usize> {
    let len = text.len_chars();
    if pos >= len {
        return None;
    }

    let c = text.char(pos);
    let (target, forward) = match c {
        '(' => (')', true),
        '[' => (']', true),
        '{' => ('}', true),
        ')' => ('(', false),
        ']' => ('[', false),
        '}' => ('{', false),
        _ => return None,
    };

    let mut depth: i32 = 1;
    if forward {
        let mut i = pos + 1;
        while i < len {
            let ch = text.char(i);
            if ch == target {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            } else if ch == c {
                depth += 1;
            }
            i += 1;
        }
    } else {
        if pos == 0 {
            return None;
        }
        let mut i = pos - 1;
        loop {
            let ch = text.char(i);
            if ch == target {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            } else if ch == c {
                depth += 1;
            }
            if i == 0 {
                break;
            }
            i -= 1;
        }
    }

    None
}

// ── Find char motions ─────────────────────────────

/// Find next `target` on current line (vim `f`).
pub fn find_char_forward(
    text: &Rope,
    pos: usize,
    target: char,
) -> Option<usize> {
    let len = text.len_chars();
    let mut i = pos + 1;
    while i < len {
        let c = text.char(i);
        if c == '\n' {
            return None;
        }
        if c == target {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Find previous `target` on current line (vim `F`).
pub fn find_char_backward(
    text: &Rope,
    pos: usize,
    target: char,
) -> Option<usize> {
    if pos == 0 {
        return None;
    }
    let mut i = pos - 1;
    loop {
        let c = text.char(i);
        if c == '\n' {
            return None;
        }
        if c == target {
            return Some(i);
        }
        if i == 0 {
            return None;
        }
        i -= 1;
    }
}

/// Find char forward, stop one before (vim `t`).
pub fn till_char_forward(
    text: &Rope,
    pos: usize,
    target: char,
) -> Option<usize> {
    find_char_forward(text, pos, target).map(|p| p.saturating_sub(1).max(pos))
}

/// Find char backward, stop one after (vim `T`).
pub fn till_char_backward(
    text: &Rope,
    pos: usize,
    target: char,
) -> Option<usize> {
    find_char_backward(text, pos, target).map(|p| (p + 1).min(pos))
}

// ── Tests ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn rope(s: &str) -> Rope {
        Rope::from(s)
    }

    #[test]
    fn word_start_simple() {
        let r = rope("hello world");
        assert_eq!(next_word_start(&r, 0, false), 6);
    }

    #[test]
    fn word_start_big() {
        let r = rope("foo.bar baz");
        assert_eq!(next_word_start(&r, 0, false), 3);
        assert_eq!(next_word_start(&r, 0, true), 8);
    }

    #[test]
    fn word_start_at_end() {
        let r = rope("hi");
        assert_eq!(next_word_start(&r, 1, false), 1);
    }

    #[test]
    fn word_back_simple() {
        let r = rope("hello world");
        assert_eq!(prev_word_start(&r, 6, false), 0);
    }

    #[test]
    fn word_back_at_start() {
        let r = rope("hello");
        assert_eq!(prev_word_start(&r, 0, false), 0);
    }

    #[test]
    fn word_end_simple() {
        let r = rope("hello world");
        assert_eq!(next_word_end(&r, 0, false), 4);
    }

    #[test]
    fn word_end_from_space() {
        let r = rope("hi there");
        assert_eq!(next_word_end(&r, 2, false), 7);
    }

    #[test]
    fn paragraph_forward_basic() {
        let r = rope("hello\nworld\n\nfoo");
        let pos = paragraph_forward(&r, 0);
        assert_eq!(r.char_to_line(pos), 3);
    }

    #[test]
    fn paragraph_backward_basic() {
        // "aaa\n\nbbb\n\nccc" — from end, { goes to "bbb"
        let r = rope("aaa\n\nbbb\n\nccc");
        let end_pos = r.len_chars() - 1;
        let pos = paragraph_backward(&r, end_pos);
        assert_eq!(r.char_to_line(pos), 2);
    }

    #[test]
    fn bracket_forward() {
        let r = rope("(hello)");
        assert_eq!(find_matching_bracket(&r, 0), Some(6),);
    }

    #[test]
    fn bracket_backward() {
        let r = rope("(hello)");
        assert_eq!(find_matching_bracket(&r, 6), Some(0),);
    }

    #[test]
    fn bracket_nested() {
        let r = rope("((a)(b))");
        assert_eq!(find_matching_bracket(&r, 0), Some(7),);
        assert_eq!(find_matching_bracket(&r, 1), Some(3),);
    }

    #[test]
    fn bracket_no_match() {
        let r = rope("(hello");
        assert_eq!(find_matching_bracket(&r, 0), None);
    }

    #[test]
    fn bracket_not_on_bracket() {
        let r = rope("hello");
        assert_eq!(find_matching_bracket(&r, 0), None);
    }

    #[test]
    fn find_char_fwd() {
        let r = rope("hello world");
        assert_eq!(find_char_forward(&r, 0, 'o'), Some(4),);
    }

    #[test]
    fn find_char_fwd_not_found() {
        let r = rope("hello");
        assert_eq!(find_char_forward(&r, 0, 'z'), None,);
    }

    #[test]
    fn find_char_fwd_stops_at_newline() {
        let r = rope("hello\nworld");
        assert_eq!(find_char_forward(&r, 0, 'w'), None,);
    }

    #[test]
    fn find_char_bwd() {
        let r = rope("hello world");
        assert_eq!(find_char_backward(&r, 10, 'o'), Some(7),);
    }

    #[test]
    fn till_char_fwd() {
        let r = rope("hello world");
        assert_eq!(till_char_forward(&r, 0, 'o'), Some(3),);
    }

    #[test]
    fn till_char_bwd() {
        let r = rope("hello world");
        assert_eq!(till_char_backward(&r, 10, 'o'), Some(8),);
    }
}
