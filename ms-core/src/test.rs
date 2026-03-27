/// Test utilities for annotated cursor/selection strings.
///
/// Annotation syntax (adopted from Helix):
///   `#[|]#`  — cursor position (pipe = head)
///
/// Example: `"he#[|]#llo"` means cursor is at column 2 on
/// the line containing "hello".
///
/// `print` parses annotations → plain text + cursor pos.
/// `plain` inserts annotations back → readable assertions.

/// Marker tokens.
const HEAD_START: &str = "#[";
const HEAD_END: &str = "]#";
const PIPE: char = '|';

/// Parse an annotated string into plain text and cursor
/// position (line, col).
///
/// # Panics
/// Panics if the annotation is malformed or missing.
#[must_use]
pub fn print(s: &str) -> (String, usize, usize) {
    let start = s
        .find(HEAD_START)
        .expect("test string missing #[ marker");
    let end = s
        .find(HEAD_END)
        .expect("test string missing ]# marker");

    // Content between markers should be just `|`
    let inner =
        &s[start + HEAD_START.len()..end];
    assert!(
        inner == "|",
        "expected `|` between #[ and ]#, got `{inner}`",
    );

    // Build plain text: everything before #[, then
    // everything after ]#
    let before = &s[..start];
    let after = &s[end + HEAD_END.len()..];
    let plain_text = format!("{before}{after}");

    // Cursor position is the char offset of `before`
    let cursor_char_offset = before.chars().count();

    // Convert char offset to (line, col)
    let mut line = 0;
    let mut col = 0;
    for (i, c) in plain_text.chars().enumerate() {
        if i == cursor_char_offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }

    (plain_text, line, col)
}

/// Insert cursor annotation into plain text at the given
/// (line, col) position.
#[must_use]
pub fn plain(
    text: &str,
    line: usize,
    col: usize,
) -> String {
    let mut current_line = 0;
    let mut current_col = 0;
    let mut result = String::with_capacity(
        text.len() + HEAD_START.len() + 1 + HEAD_END.len(),
    );

    let mut inserted = false;
    for c in text.chars() {
        if !inserted
            && current_line == line
            && current_col == col
        {
            result.push_str(HEAD_START);
            result.push(PIPE);
            result.push_str(HEAD_END);
            inserted = true;
        }
        result.push(c);
        if c == '\n' {
            current_line += 1;
            current_col = 0;
        } else {
            current_col += 1;
        }
    }

    // Cursor at end of text
    if !inserted {
        result.push_str(HEAD_START);
        result.push(PIPE);
        result.push_str(HEAD_END);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_start_of_line() {
        let (text, line, col) = print("#[|]#hello");
        assert_eq!(text, "hello");
        assert_eq!(line, 0);
        assert_eq!(col, 0);
    }

    #[test]
    fn parse_middle() {
        let (text, line, col) = print("he#[|]#llo");
        assert_eq!(text, "hello");
        assert_eq!(line, 0);
        assert_eq!(col, 2);
    }

    #[test]
    fn parse_end() {
        let (text, line, col) = print("hello#[|]#");
        assert_eq!(text, "hello");
        assert_eq!(line, 0);
        assert_eq!(col, 5);
    }

    #[test]
    fn parse_multiline() {
        let (text, line, col) =
            print("hello\nwo#[|]#rld");
        assert_eq!(text, "hello\nworld");
        assert_eq!(line, 1);
        assert_eq!(col, 2);
    }

    #[test]
    fn roundtrip() {
        let input = "he#[|]#llo\nworld";
        let (text, line, col) = print(input);
        let output = plain(&text, line, col);
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_end() {
        let input = "hello\nworld#[|]#";
        let (text, line, col) = print(input);
        let output = plain(&text, line, col);
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_newline_boundary() {
        let input = "#[|]#hello\nworld";
        let (text, line, col) = print(input);
        let output = plain(&text, line, col);
        assert_eq!(output, input);
    }

    #[test]
    fn plain_inserts_correctly() {
        let result = plain("hello\nworld", 1, 3);
        assert_eq!(result, "hello\nwor#[|]#ld");
    }
}
