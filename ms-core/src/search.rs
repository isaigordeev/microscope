//! Regex search over a rope (vim `/`, `?`, `n`, `*`).
//!
//! Pure functions — no editor dependency. Positions
//! are char indices; matches are half-open ranges
//! `[start, end)`. Forward/backward searches wrap
//! around (vim 'wrapscan').

use ropey::Rope;

pub use regex::Regex;

/// Compile a search pattern. `None` when the pattern
/// is not a valid regex.
#[must_use]
pub fn compile(pattern: &str) -> Option<Regex> {
    Regex::new(pattern).ok()
}

/// Whole-word pattern for the word under cursor
/// (vim `*`/`#` → `\<word\>`).
#[must_use]
pub fn word_pattern(word: &str) -> String {
    format!(r"\b{}\b", regex::escape(word))
}

/// First match starting at or after `from`, wrapping
/// to the top when none is found below.
#[must_use]
pub fn find_forward(
    text: &Rope,
    re: &Regex,
    from: usize,
) -> Option<(usize, usize)> {
    let s = text.to_string();
    let from_byte = text.char_to_byte(from.min(text.len_chars()));
    let found = re.find_at(&s, from_byte).or_else(|| re.find(&s))?;
    Some((text.byte_to_char(found.start()), text.byte_to_char(found.end())))
}

/// Last match starting strictly before `before`,
/// wrapping to the bottom when none is found above.
#[must_use]
pub fn find_backward(
    text: &Rope,
    re: &Regex,
    before: usize,
) -> Option<(usize, usize)> {
    let s = text.to_string();
    let before_byte = text.char_to_byte(before.min(text.len_chars()));
    let mut prev = None;
    let mut last = None;
    for m in re.find_iter(&s) {
        if m.start() < before_byte {
            prev = Some(m);
        }
        last = Some(m);
    }
    let found = prev.or(last)?;
    Some((text.byte_to_char(found.start()), text.byte_to_char(found.end())))
}

/// All matches in the text (for highlight overlays).
#[must_use]
pub fn find_all(text: &Rope, re: &Regex) -> Vec<(usize, usize)> {
    let s = text.to_string();
    re.find_iter(&s)
        .map(|m| (text.byte_to_char(m.start()), text.byte_to_char(m.end())))
        .collect()
}

// ── Tests ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn rope(s: &str) -> Rope {
        Rope::from(s)
    }

    #[test]
    fn forward_finds_next() {
        let r = rope("hello hello");
        let re = compile("ll").unwrap_or_else(|| unreachable!());
        assert_eq!(find_forward(&r, &re, 3), Some((8, 10)));
    }

    #[test]
    fn forward_wraps() {
        let r = rope("hello world");
        let re = compile("he").unwrap_or_else(|| unreachable!());
        assert_eq!(find_forward(&r, &re, 5), Some((0, 2)));
    }

    #[test]
    fn forward_at_match_start_finds_it() {
        let r = rope("hello");
        let re = compile("ll").unwrap_or_else(|| unreachable!());
        assert_eq!(find_forward(&r, &re, 2), Some((2, 4)));
    }

    #[test]
    fn backward_finds_previous() {
        let r = rope("hello hello");
        let re = compile("ll").unwrap_or_else(|| unreachable!());
        assert_eq!(find_backward(&r, &re, 8), Some((2, 4)));
    }

    #[test]
    fn backward_wraps() {
        let r = rope("hello hello");
        let re = compile("ll").unwrap_or_else(|| unreachable!());
        assert_eq!(find_backward(&r, &re, 1), Some((8, 10)));
    }

    #[test]
    fn no_match_is_none() {
        let r = rope("hello");
        let re = compile("zz").unwrap_or_else(|| unreachable!());
        assert_eq!(find_forward(&r, &re, 0), None);
        assert_eq!(find_backward(&r, &re, 5), None);
    }

    #[test]
    fn find_all_matches() {
        let r = rope("ab ab ab");
        let re = compile("ab").unwrap_or_else(|| unreachable!());
        assert_eq!(find_all(&r, &re), vec![(0, 2), (3, 5), (6, 8)]);
    }

    #[test]
    fn invalid_pattern_is_none() {
        assert!(compile("[").is_none());
    }

    #[test]
    fn word_pattern_escapes() {
        let r = rope("a.b x a.b");
        let re =
            compile(&word_pattern("a.b")).unwrap_or_else(|| unreachable!());
        // dot is literal, not any-char
        assert_eq!(find_all(&r, &re).len(), 2);
    }

    #[test]
    fn word_pattern_whole_word_only() {
        let r = rope("foo foobar foo");
        let re =
            compile(&word_pattern("foo")).unwrap_or_else(|| unreachable!());
        assert_eq!(find_all(&r, &re), vec![(0, 3), (11, 14)]);
    }

    #[test]
    fn multiline_search() {
        let r = rope("abc\ndef\nabc");
        let re = compile("abc").unwrap_or_else(|| unreachable!());
        assert_eq!(find_forward(&r, &re, 1), Some((8, 11)));
    }
}
