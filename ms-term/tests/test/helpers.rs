use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ms_core::test as test_util;
use ms_view::document::Document;
use ms_view::editor::Editor;
use ropey::Rope;

/// Create a test editor from annotated text.
/// Cursor is placed at the `#[|]#` marker.
pub(crate) fn test_editor(annotated: &str) -> Editor {
    let (text, line, col) = test_util::print(annotated);
    let doc = Document {
        text: Rope::from(text.as_str()),
        path: None,
        modified: false,
    };
    let mut editor = Editor::new(doc, 24);
    editor.view.cursor_line = line;
    editor.view.cursor_col = col;
    editor.view.desired_col = col;
    editor
}

/// Get annotated text from current editor state.
pub(crate) fn editor_annotated(editor: &Editor) -> String {
    let text = editor.document.text.to_string();
    test_util::plain(&text, editor.view.cursor_line, editor.view.cursor_col)
}

/// Parse a key string into a sequence of `KeyEvent`s.
///
/// Supports:
/// - Single chars: `"jjk"` → j, j, k
/// - Special keys: `"<esc>"`, `"<ret>"`, `"<bs>"`
/// - Ctrl combos: `"<C-w>"`
/// - Raw colons, etc.
pub(crate) fn parse_keys(input: &str) -> Vec<KeyEvent> {
    let mut keys = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '<' {
            // Find closing >
            if let Some(end) = chars[i..].iter().position(|&c| c == '>') {
                let tag: String = chars[i + 1..i + end].iter().collect();
                keys.push(parse_special_key(&tag));
                i += end + 1;
            } else {
                keys.push(KeyEvent::new(
                    KeyCode::Char('<'),
                    KeyModifiers::NONE,
                ));
                i += 1;
            }
        } else {
            keys.push(KeyEvent::new(
                KeyCode::Char(chars[i]),
                KeyModifiers::NONE,
            ));
            i += 1;
        }
    }

    keys
}

fn parse_special_key(tag: &str) -> KeyEvent {
    let lower = tag.to_lowercase();
    match lower.as_str() {
        "esc" => KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        "ret" | "cr" | "enter" => {
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
        }
        "bs" | "backspace" => {
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)
        }
        "del" | "delete" => KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE),
        "left" => KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
        "right" => KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
        "up" => KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
        "down" => KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
        s if s.starts_with("c-") => {
            let ch = s[2..].chars().next().unwrap_or('?');
            KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL)
        }
        _ => KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_keys() {
        let keys = parse_keys("jjk");
        assert_eq!(keys.len(), 3);
        assert_eq!(keys[0].code, KeyCode::Char('j'));
        assert_eq!(keys[2].code, KeyCode::Char('k'));
    }

    #[test]
    fn parse_special() {
        let keys = parse_keys("<esc>");
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].code, KeyCode::Esc);
    }

    #[test]
    fn parse_ctrl() {
        let keys = parse_keys("<C-w>");
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].code, KeyCode::Char('w'));
        assert!(keys[0].modifiers.contains(KeyModifiers::CONTROL));
    }

    #[test]
    fn parse_mixed() {
        let keys = parse_keys("ihi<esc>");
        assert_eq!(keys.len(), 4);
        assert_eq!(keys[0].code, KeyCode::Char('i'));
        assert_eq!(keys[3].code, KeyCode::Esc);
    }
}
