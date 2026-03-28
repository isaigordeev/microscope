#![cfg(feature = "integration")]

mod test;

use test::helpers::{editor_annotated, parse_keys, test_editor};

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
        actual,
        expected,
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
    test(("line1\nline2\n#[|]#line3", "gg", "#[|]#line1\nline2\nline3"));
}

#[test]
fn go_to_bottom() {
    test(("#[|]#line1\nline2\nline3", "G", "line1\nline2\n#[|]#line3"));
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
    test(("#[|]#hello", "iab<esc>", "ab#[|]#hello"));
}

#[test]
fn open_line_below() {
    test(("#[|]#hello", "ohi<esc>", "hello\nh#[|]#i"));
}

#[test]
fn open_line_above() {
    test(("#[|]#hello", "Ohi<esc>", "h#[|]#i\nhello"));
}

#[test]
fn backspace_in_insert() {
    test(("he#[|]#llo", "i<bs><esc>", "h#[|]#llo"));
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

// ── Count + motion ───────────────────────────────

#[test]
fn count_motion_2j() {
    test(("#[|]#line1\nline2\nline3", "2j", "line1\nline2\n#[|]#line3"));
}

#[test]
fn count_motion_3l() {
    test(("#[|]#hello", "3l", "hel#[|]#lo"));
}

// ── Delete with motions ──────────────────────────

#[test]
fn delete_word_dw() {
    test(("#[|]#hello world", "dw", "#[|]#world"));
}

#[test]
fn delete_2_words() {
    test(("#[|]#hello cruel world", "d2w", "#[|]#world"));
}

#[test]
fn delete_line_dd() {
    test(("#[|]#hello\nworld", "dd", "#[|]#world"));
}

#[test]
fn delete_3_lines() {
    test(("#[|]#line1\nline2\nline3\nline4", "3dd", "#[|]#line4"));
}

#[test]
fn delete_to_end_d_dollar() {
    test(("he#[|]#llo world", "d$", "h#[|]#e"));
}

#[test]
fn delete_to_end_big_d() {
    test(("he#[|]#llo world", "D", "h#[|]#e"));
}

// ── Change with motions ──────────────────────────

#[test]
fn change_word_cw() {
    // cw behaves like ce (vim special case)
    test(("#[|]#hello world", "cwbye<esc>", "bye#[|]# world"));
}

#[test]
fn change_line_cc() {
    test(("  #[|]#hello\nworld", "ccbye<esc>", "by#[|]#e\nworld"));
}

#[test]
fn change_to_end_big_c() {
    test(("he#[|]#llo world", "Cbye<esc>", "heby#[|]#e"));
}

// ── Substitute ───────────────────────────────────

#[test]
fn substitute_s() {
    // s deletes char, enters insert; esc clamps left
    test(("#[|]#hello", "sx<esc>", "x#[|]#ello"));
}

// ── Yank + Paste ─────────────────────────────────

#[test]
fn yank_line_paste() {
    // p pastes below; cursor goes to pasted line
    test(("#[|]#hello\nworld", "yyp", "hello\n#[|]#hello\nworld"));
}

#[test]
fn yank_word_paste() {
    // yw yanks "hello " (exclusive), e→col4, p→paste
    test(("#[|]#hello world", "ywep", "hellohello#[|]#  world"));
}

#[test]
fn paste_before_big_p() {
    test(("#[|]#hello\nworld", "yyP", "#[|]#hello\nhello\nworld"));
}

// ── Undo / Redo ──────────────────────────────────

#[test]
fn undo_delete() {
    test(("#[|]#hello", "xu", "#[|]#hello"));
}

#[test]
fn undo_redo() {
    test(("#[|]#hello", "xu<C-r>", "#[|]#ello"));
}

// ── Replace char ─────────────────────────────────

#[test]
fn replace_char_r() {
    test(("#[|]#hello", "rx", "#[|]#xello"));
}

// ── Esc cancels operator ─────────────────────────

#[test]
fn esc_cancels_operator_then_moves() {
    test(("#[|]#hello\nworld", "d<esc>j", "hello\n#[|]#world"));
}

// ── Indent / Dedent ──────────────────────────────

#[test]
fn indent_line() {
    test(("#[|]#hello", ">>", "    #[|]#hello"));
}

#[test]
fn dedent_line() {
    test(("    #[|]#hello", "<<", "#[|]#hello"));
}

// ── Join lines ───────────────────────────────────

#[test]
fn join_lines_j_upper() {
    // J places cursor at the join point (space)
    test(("#[|]#hello\nworld", "J", "hello#[|]# world"));
}

// ── Toggle case ──────────────────────────────────

#[test]
fn toggle_case_tilde() {
    // ~ toggles case and advances cursor
    test(("#[|]#hello", "~", "H#[|]#ello"));
}

// ── Find char motions ────────────────────────────

#[test]
fn find_char_f() {
    test(("#[|]#hello world", "fw", "hello #[|]#world"));
}

#[test]
fn till_char_t() {
    // t stops one before target (space before w)
    test(("#[|]#hello world", "tw", "hello#[|]# world"));
}

#[test]
fn delete_find_char_df() {
    test(("#[|]#hello world", "dfw", "#[|]#orld"));
}

// ── Bracket matching ─────────────────────────────

#[test]
fn match_bracket() {
    test(("#[|]#(hello)", "%", "(hello#[|]#)"));
}

// ── Command mode ──────────────────────────────────

#[test]
fn command_quit_modified() {
    let mut editor = test_editor("#[|]#hello");
    editor.document.modified = true;
    let keys = parse_keys(":q<ret>");
    for key in keys {
        ms_term::application::handle_key(&mut editor, key);
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
        ms_term::application::handle_key(&mut editor, key);
    }
    assert!(editor.should_quit);
}

// ── Theme command ────────────────────────────────

#[test]
fn theme_switch_to_light() {
    let mut editor = test_editor("#[|]#hello");
    let keys = parse_keys(":theme light<ret>");
    for key in keys {
        ms_term::application::handle_key(&mut editor, key);
    }
    assert_eq!(editor.theme.name, "vs_light");
}

#[test]
fn theme_switch_to_dark() {
    let mut editor = test_editor("#[|]#hello");
    // Switch to light first, then back to dark
    for key in parse_keys(":theme light<ret>") {
        ms_term::application::handle_key(&mut editor, key);
    }
    assert_eq!(editor.theme.name, "vs_light");
    for key in parse_keys(":theme dark<ret>") {
        ms_term::application::handle_key(&mut editor, key);
    }
    assert_eq!(editor.theme.name, "vs_dark");
}

#[test]
fn theme_unknown_shows_error() {
    let mut editor = test_editor("#[|]#hello");
    let keys = parse_keys(":theme nonexistent<ret>");
    for key in keys {
        ms_term::application::handle_key(&mut editor, key);
    }
    assert_eq!(
        editor.status_message.as_deref(),
        Some("Unknown theme: nonexistent"),
    );
}

#[test]
fn theme_no_arg_shows_current() {
    let mut editor = test_editor("#[|]#hello");
    let keys = parse_keys(":theme<ret>");
    for key in keys {
        ms_term::application::handle_key(&mut editor, key);
    }
    assert!(editor
        .status_message
        .as_ref()
        .unwrap()
        .starts_with("Current theme:"));
}
