#![cfg(feature = "integration")]

mod test;

use test::helpers::{
    editor_annotated, parse_keys, test_editor,
};

/// Run a test case: (initial_annotated, keys, expected).
fn test<S, K, E>(case: (S, K, E))
where
    S: AsRef<str>,
    K: AsRef<str>,
    E: AsRef<str>,
{
    let mut editor = test_editor(case.0.as_ref());
    let keys = parse_keys(case.1.as_ref());

    for key in keys {
        ms_term::application::handle_key(&mut editor, key);
    }

    let actual = editor_annotated(&editor);
    let expected = case.2.as_ref();
    assert_eq!(
        actual, expected,
        "\nkeys: {:?}\nactual:   {actual}\nexpected: {expected}",
        case.1.as_ref(),
    );
}

// ── Normal mode movement ──────────────────────────

#[test]
fn move_right() {
    test(("#[|]#hello", "l", "h#[|]#ello"));
}

#[test]
fn move_left() {
    test(("he#[|]#llo", "h", "h#[|]#ello"));
}

#[test]
fn move_left_at_start_stays() {
    test(("#[|]#hello", "h", "#[|]#hello"));
}

#[test]
fn move_down() {
    test(("#[|]#hello\nworld", "j", "hello\n#[|]#world"));
}

#[test]
fn move_up() {
    test(("hello\n#[|]#world", "k", "#[|]#hello\nworld"));
}

#[test]
fn line_start() {
    test(("he#[|]#llo", "0", "#[|]#hello"));
}

#[test]
fn line_end() {
    test(("#[|]#hello", "$", "hell#[|]#o"));
}

#[test]
fn first_non_blank() {
    test(("    #[|]#hello", "^", "    #[|]#hello"));
    test(("#[|]#    hello", "^", "    #[|]#hello"));
}

#[test]
fn word_forward() {
    test(("#[|]#hello world", "w", "hello #[|]#world"));
}

#[test]
fn word_backward() {
    test(("hello #[|]#world", "b", "#[|]#hello world"));
}

#[test]
fn word_end() {
    test(("#[|]#hello world", "e", "hell#[|]#o world"));
}

#[test]
fn go_to_top() {
    test((
        "line1\nline2\n#[|]#line3",
        "g",
        "#[|]#line1\nline2\nline3",
    ));
}

#[test]
fn go_to_bottom() {
    test((
        "#[|]#line1\nline2\nline3",
        "G",
        "line1\nline2\n#[|]#line3",
    ));
}

// ── Insert mode ───────────────────────────────────

#[test]
fn insert_char() {
    // <esc> clamps cursor left (vim behavior)
    test(("#[|]#hello", "ix<esc>", "x#[|]#hello"));
}

#[test]
fn insert_at_end() {
    test(("hell#[|]#o", "Ax<esc>", "hello#[|]#x"));
}

#[test]
fn insert_multiple() {
    test((
        "#[|]#hello",
        "iab<esc>",
        "ab#[|]#hello",
    ));
}

#[test]
fn open_line_below() {
    test((
        "#[|]#hello",
        "ohi<esc>",
        "hello\nh#[|]#i",
    ));
}

#[test]
fn open_line_above() {
    test((
        "#[|]#hello",
        "Ohi<esc>",
        "h#[|]#i\nhello",
    ));
}

#[test]
fn backspace_in_insert() {
    test((
        "he#[|]#llo",
        "i<bs><esc>",
        "h#[|]#llo",
    ));
}

// ── Delete (x) ────────────────────────────────────

#[test]
fn delete_char_x() {
    test(("#[|]#hello", "x", "#[|]#ello"));
}

#[test]
fn delete_char_x_end() {
    test(("hell#[|]#o", "x", "hel#[|]#l"));
}

// ── Command mode ──────────────────────────────────

#[test]
fn command_quit_modified() {
    let mut editor = test_editor("#[|]#hello");
    editor.document.modified = true;
    let keys = parse_keys(":q<ret>");
    for key in keys {
        ms_term::application::handle_key(
            &mut editor, key,
        );
    }
    // Should NOT quit — modified buffer
    assert!(!editor.should_quit);
    assert!(editor.status_message.is_some());
}

#[test]
fn command_force_quit() {
    let mut editor = test_editor("#[|]#hello");
    editor.document.modified = true;
    let keys = parse_keys(":q!<ret>");
    for key in keys {
        ms_term::application::handle_key(
            &mut editor, key,
        );
    }
    assert!(editor.should_quit);
}
