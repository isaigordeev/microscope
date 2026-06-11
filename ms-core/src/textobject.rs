//! Text objects (vim `iw`, `a"`, `i(`, `ap`, ...).
//!
//! Pure functions — no editor/view dependency. Each
//! takes a rope + cursor position and returns the
//! half-open char range `[start, end)` of the object,
//! or `None` when the object cannot be found.
//!
//! Algorithms ported from Helix (`helix-core`), with
//! classic vim semantics: cursor on whitespace selects
//! the whitespace run, quotes pair up left-to-right on
//! the cursor line.

use ropey::Rope;

use crate::movement::char_category;

// ── Run helpers ───────────────────────────────────

/// `[start, end)` of the category run containing
/// `pos`, never crossing a newline. A newline is its
/// own run.
fn run_bounds(text: &Rope, pos: usize, big: bool) -> (usize, usize) {
    let len = text.len_chars();
    let c = text.char(pos);
    if c == '\n' {
        return (pos, pos + 1);
    }
    let cat = char_category(c, big);

    let mut start = pos;
    while start > 0 {
        let prev = text.char(start - 1);
        if prev == '\n' || char_category(prev, big) != cat {
            break;
        }
        start -= 1;
    }

    let mut end = pos + 1;
    while end < len {
        let next = text.char(end);
        if next == '\n' || char_category(next, big) != cat {
            break;
        }
        end += 1;
    }

    (start, end)
}

/// Whether the char at `i` continues the current line
/// (exists and is not a newline).
fn in_line(text: &Rope, i: usize) -> bool {
    i < text.len_chars() && text.char(i) != '\n'
}

// ── Word ──────────────────────────────────────────

/// Word text object (vim `iw`/`aw`/`iW`/`aW`).
pub fn word(
    text: &Rope,
    pos: usize,
    count: usize,
    big: bool,
    around: bool,
) -> Option<(usize, usize)> {
    if pos >= text.len_chars() {
        return None;
    }
    let count = count.max(1);
    let (mut start, mut end) = run_bounds(text, pos, big);
    let on_whitespace = text.char(pos).is_whitespace();

    if around {
        if on_whitespace {
            // `aw` on whitespace: whitespace + next word.
            if in_line(text, end) {
                end = run_bounds(text, end, big).1;
            }
        } else if in_line(text, end) && text.char(end).is_whitespace() {
            // Trailing whitespace preferred...
            end = run_bounds(text, end, big).1;
        } else if start > 0 {
            // ...else leading whitespace.
            let prev = text.char(start - 1);
            if prev != '\n' && prev.is_whitespace() {
                start = run_bounds(text, start - 1, big).0;
            }
        }
    }

    for _ in 1..count {
        if !in_line(text, end) {
            break;
        }
        let first_is_ws = text.char(end).is_whitespace();
        end = run_bounds(text, end, big).1;
        // `aw` counts whole words: pair each word with
        // its separating whitespace run. `iw` counts
        // every run individually.
        if around
            && in_line(text, end)
            && first_is_ws != text.char(end).is_whitespace()
        {
            end = run_bounds(text, end, big).1;
        }
    }

    Some((start, end))
}

// ── Paragraph ─────────────────────────────────────

fn is_blank_line(text: &Rope, line: usize) -> bool {
    text.line(line).chars().all(char::is_whitespace)
}

/// Last real line index (ignores ropey's phantom line
/// after a trailing newline).
fn last_line(text: &Rope) -> usize {
    text.char_to_line(text.len_chars().saturating_sub(1))
}

/// Paragraph text object (vim `ip`/`ap`). Returns a
/// linewise char range.
pub fn paragraph(
    text: &Rope,
    pos: usize,
    count: usize,
    around: bool,
) -> Option<(usize, usize)> {
    if text.len_chars() == 0 {
        return None;
    }
    let count = count.max(1);
    let max_line = last_line(text);
    let line = text.char_to_line(pos);
    let blank = is_blank_line(text, line);

    let mut start_line = line;
    while start_line > 0 && is_blank_line(text, start_line - 1) == blank {
        start_line -= 1;
    }
    let mut end_line = line;
    while end_line < max_line && is_blank_line(text, end_line + 1) == blank {
        end_line += 1;
    }

    // Each extra count appends the next block
    // (alternating blank / non-blank).
    for _ in 1..count {
        if end_line >= max_line {
            break;
        }
        let next_blank = is_blank_line(text, end_line + 1);
        while end_line < max_line
            && is_blank_line(text, end_line + 1) == next_blank
        {
            end_line += 1;
        }
    }

    if around {
        if end_line < max_line && is_blank_line(text, end_line + 1) != blank {
            // Trailing block preferred...
            let next_blank = is_blank_line(text, end_line + 1);
            while end_line < max_line
                && is_blank_line(text, end_line + 1) == next_blank
            {
                end_line += 1;
            }
        } else if !blank
            && start_line > 0
            && is_blank_line(text, start_line - 1)
        {
            // ...else preceding blank lines.
            while start_line > 0 && is_blank_line(text, start_line - 1) {
                start_line -= 1;
            }
        }
    }

    let start = text.line_to_char(start_line);
    let end = if end_line + 1 < text.len_lines() {
        text.line_to_char(end_line + 1)
    } else {
        text.len_chars()
    };
    Some((start, end))
}

// ── Quote ─────────────────────────────────────────

/// Quoted-string text object (vim `i"`/`a"`/`i'`/...).
///
/// Vim semantics: quotes on the cursor line pair up
/// left-to-right; the pair containing the cursor wins,
/// else the next pair after the cursor. `a"` includes
/// trailing whitespace, or leading when there is none.
pub fn quote(
    text: &Rope,
    pos: usize,
    quote_char: char,
    around: bool,
) -> Option<(usize, usize)> {
    if pos >= text.len_chars() {
        return None;
    }
    let line = text.char_to_line(pos);
    let line_start = text.line_to_char(line);
    let line_len = text.line(line).chars().take_while(|c| *c != '\n').count();
    let line_end = line_start + line_len;

    let quotes: Vec<usize> = (line_start..line_end)
        .filter(|&i| text.char(i) == quote_char)
        .collect();

    let (open, close) = quotes
        .chunks_exact(2)
        .map(|pair| (pair[0], pair[1]))
        .find(|&(o, c)| o > pos || pos <= c)?;

    if !around {
        return Some((open + 1, close));
    }

    let (mut start, mut end) = (open, close + 1);
    let trailing_ws = in_line(text, end) && text.char(end).is_whitespace();
    if trailing_ws {
        while in_line(text, end) && text.char(end).is_whitespace() {
            end += 1;
        }
    } else {
        while start > line_start && text.char(start - 1).is_whitespace() {
            start -= 1;
        }
    }
    Some((start, end))
}

// ── Bracket pair ──────────────────────────────────

/// Find the `n`-th unmatched `open` searching backward
/// from `pos` (cursor on `open` counts as that
/// bracket).
fn find_open(
    text: &Rope,
    open: char,
    close: char,
    pos: usize,
    n: usize,
) -> Option<usize> {
    let mut remaining = n;
    let mut depth: usize = 0;
    let mut i = pos;
    loop {
        let c = text.char(i);
        if c == open {
            if depth == 0 {
                remaining -= 1;
                if remaining == 0 {
                    return Some(i);
                }
            } else {
                depth -= 1;
            }
        } else if c == close && i != pos {
            depth += 1;
        }
        if i == 0 {
            return None;
        }
        i -= 1;
    }
}

/// Find the `n`-th unmatched `close` searching forward
/// from `pos` (cursor on `close` counts as that
/// bracket).
fn find_close(
    text: &Rope,
    open: char,
    close: char,
    pos: usize,
    n: usize,
) -> Option<usize> {
    let len = text.len_chars();
    let mut remaining = n;
    let mut depth: usize = 0;
    let mut i = pos;
    while i < len {
        let c = text.char(i);
        if c == close {
            if depth == 0 {
                remaining -= 1;
                if remaining == 0 {
                    return Some(i);
                }
            } else {
                depth -= 1;
            }
        } else if c == open && i != pos {
            depth += 1;
        }
        i += 1;
    }
    None
}

/// Bracket-pair text object (vim `i(`/`a{`/`i[`/...).
/// `count` selects the `count`-th surrounding pair.
pub fn pair(
    text: &Rope,
    pos: usize,
    open: char,
    close: char,
    count: usize,
    around: bool,
) -> Option<(usize, usize)> {
    if pos >= text.len_chars() {
        return None;
    }
    let n = count.max(1);
    let open_pos = find_open(text, open, close, pos, n)?;
    let close_pos = find_close(text, open, close, pos, n)?;

    if around {
        Some((open_pos, close_pos + 1))
    } else {
        Some((open_pos + 1, close_pos))
    }
}

// ── Tests ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn rope(s: &str) -> Rope {
        Rope::from(s)
    }

    // ── word ──────────────────────────────────────

    #[test]
    fn inner_word_mid() {
        let r = rope("foo bar baz");
        assert_eq!(word(&r, 5, 1, false, false), Some((4, 7)));
    }

    #[test]
    fn inner_word_on_punct() {
        let r = rope("foo.bar");
        assert_eq!(word(&r, 3, 1, false, false), Some((3, 4)));
    }

    #[test]
    fn inner_big_word_spans_punct() {
        let r = rope("foo.bar baz");
        assert_eq!(word(&r, 3, 1, true, false), Some((0, 7)));
    }

    #[test]
    fn inner_word_on_whitespace_selects_run() {
        let r = rope("foo   bar");
        assert_eq!(word(&r, 4, 1, false, false), Some((3, 6)));
    }

    #[test]
    fn around_word_trailing_ws() {
        let r = rope("foo bar baz");
        assert_eq!(word(&r, 4, 1, false, true), Some((4, 8)));
    }

    #[test]
    fn around_word_leading_ws_at_eol() {
        let r = rope("foo bar");
        assert_eq!(word(&r, 5, 1, false, true), Some((3, 7)));
    }

    #[test]
    fn around_word_on_whitespace() {
        let r = rope("foo  bar");
        assert_eq!(word(&r, 3, 1, false, true), Some((3, 8)));
    }

    #[test]
    fn inner_word_count() {
        // 3iw = word + ws + word
        let r = rope("foo bar baz");
        assert_eq!(word(&r, 0, 3, false, false), Some((0, 7)));
    }

    #[test]
    fn around_word_count() {
        // 2aw = two words with their whitespace
        let r = rope("foo bar baz");
        assert_eq!(word(&r, 0, 2, false, true), Some((0, 8)));
    }

    #[test]
    fn word_does_not_cross_newline() {
        let r = rope("foo\nbar");
        assert_eq!(word(&r, 0, 1, false, true), Some((0, 3)));
    }

    #[test]
    fn word_on_newline_of_empty_line() {
        let r = rope("foo\n\nbar");
        assert_eq!(word(&r, 4, 1, false, false), Some((4, 5)));
    }

    #[test]
    fn word_past_end_is_none() {
        let r = rope("foo");
        assert_eq!(word(&r, 3, 1, false, false), None);
    }

    // ── paragraph ─────────────────────────────────

    #[test]
    fn inner_paragraph() {
        let r = rope("aaa\nbbb\n\nccc\n");
        assert_eq!(paragraph(&r, 5, 1, false), Some((0, 8)));
    }

    #[test]
    fn around_paragraph_takes_trailing_blanks() {
        let r = rope("aaa\nbbb\n\n\nccc\n");
        assert_eq!(paragraph(&r, 0, 1, true), Some((0, 10)));
    }

    #[test]
    fn around_paragraph_leading_blanks_at_eof() {
        // blank line 1 starts at char 4
        let r = rope("aaa\n\nbbb\nccc");
        assert_eq!(paragraph(&r, 6, 1, true), Some((4, 12)));
    }

    #[test]
    fn inner_paragraph_on_blank_line() {
        let r = rope("aaa\n\n\nbbb");
        assert_eq!(paragraph(&r, 4, 1, false), Some((4, 6)));
    }

    #[test]
    fn inner_paragraph_count() {
        // 2ip = paragraph + following blank block
        let r = rope("aaa\n\nbbb\n");
        assert_eq!(paragraph(&r, 0, 2, false), Some((0, 5)));
    }

    // ── quote ─────────────────────────────────────

    #[test]
    fn inner_quote_cursor_inside() {
        let r = rope(r#"say "hello" now"#);
        assert_eq!(quote(&r, 6, '"', false), Some((5, 10)));
    }

    #[test]
    fn inner_quote_cursor_on_open() {
        let r = rope(r#"say "hello" now"#);
        assert_eq!(quote(&r, 4, '"', false), Some((5, 10)));
    }

    #[test]
    fn inner_quote_cursor_on_close() {
        let r = rope(r#"say "hello" now"#);
        assert_eq!(quote(&r, 10, '"', false), Some((5, 10)));
    }

    #[test]
    fn inner_quote_cursor_before_string() {
        // vim jumps forward to the next quoted string
        let r = rope(r#"say "hello" now"#);
        assert_eq!(quote(&r, 0, '"', false), Some((5, 10)));
    }

    #[test]
    fn quote_parity_between_strings() {
        // between two strings: cursor sits after pair 1
        // closes — next pair (chunks 2-3) is chosen
        let r = rope(r#""a" mid "b""#);
        assert_eq!(quote(&r, 5, '"', false), Some((9, 10)));
    }

    #[test]
    fn around_quote_trailing_ws() {
        let r = rope(r#"say "hello" now"#);
        assert_eq!(quote(&r, 6, '"', true), Some((4, 12)));
    }

    #[test]
    fn around_quote_leading_ws_at_eol() {
        let r = rope(r#"say "hello""#);
        assert_eq!(quote(&r, 6, '"', true), Some((3, 11)));
    }

    #[test]
    fn quote_unmatched_is_none() {
        let r = rope("say \"hello now");
        assert_eq!(quote(&r, 6, '"', false), None);
    }

    #[test]
    fn quote_none_on_line_without_quotes() {
        let r = rope("hello");
        assert_eq!(quote(&r, 2, '"', false), None);
    }

    // ── pair ──────────────────────────────────────

    #[test]
    fn inner_pair_simple() {
        let r = rope("a(bc)d");
        assert_eq!(pair(&r, 2, '(', ')', 1, false), Some((2, 4)));
    }

    #[test]
    fn around_pair_simple() {
        let r = rope("a(bc)d");
        assert_eq!(pair(&r, 2, '(', ')', 1, true), Some((1, 5)));
    }

    #[test]
    fn pair_cursor_on_open() {
        let r = rope("a(bc)d");
        assert_eq!(pair(&r, 1, '(', ')', 1, false), Some((2, 4)));
    }

    #[test]
    fn pair_cursor_on_close() {
        let r = rope("a(bc)d");
        assert_eq!(pair(&r, 4, '(', ')', 1, false), Some((2, 4)));
    }

    #[test]
    fn pair_nested_inner() {
        let r = rope("(a(b)c)");
        assert_eq!(pair(&r, 3, '(', ')', 1, false), Some((3, 4)));
    }

    #[test]
    fn pair_nested_count_two() {
        let r = rope("(a(b)c)");
        assert_eq!(pair(&r, 3, '(', ')', 2, false), Some((1, 6)));
    }

    #[test]
    fn pair_nested_from_between() {
        // cursor between nested pairs picks the outer
        let r = rope("(a(b)c)");
        assert_eq!(pair(&r, 5, '(', ')', 1, false), Some((1, 6)));
    }

    #[test]
    fn pair_unmatched_is_none() {
        let r = rope("(abc");
        assert_eq!(pair(&r, 2, '(', ')', 1, false), None);
    }

    #[test]
    fn pair_multiline() {
        let r = rope("{\nfoo\n}");
        assert_eq!(pair(&r, 3, '{', '}', 1, false), Some((1, 6)));
    }

    #[test]
    fn pair_empty_inner() {
        let r = rope("a()b");
        assert_eq!(pair(&r, 1, '(', ')', 1, false), Some((2, 2)));
    }
}
