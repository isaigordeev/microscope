use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use ms_core::movement;
use ms_core::search;
use ms_core::textobject;
use ms_core::transaction::Transaction;
use ms_tui::buffer::Rect;
use ms_view::command::{
    Action, InsertVariant, KeyCode as VKeyCode, KeyInput, Motion, MotionType,
    Operator, SpecialCommand, TextObjectTarget,
};
use ms_view::command_line::{self, Address, ExCommand, ExRange};
use ms_view::editor::Editor;
use ms_view::mode::{Mode, VisualKind};
use ms_view::theme::builtin_theme;

pub(crate) fn build_status_line(editor: &Editor, area: Rect) -> String {
    if let Some(ref msg) = editor.status_message {
        return msg.clone();
    }

    let file_name = editor.document.path.as_ref().map_or("[scratch]", |p| {
        p.file_name().and_then(|n| n.to_str()).unwrap_or("[scratch]")
    });
    let modified = if editor.document.modified { "[+]" } else { "" };
    let mode_name = editor.mode.display_name();
    let pos = format!(
        "{}:{} ",
        editor.view.cursor_line + 1,
        editor.view.cursor_col + 1,
    );
    format!(
        " -- {mode_name} -- {file_name}{modified}\
         {:>width$}",
        pos,
        width = (area.width as usize).saturating_sub(
            mode_name.len() + file_name.len() + modified.len() + 8
        ),
    )
}

// ── Key conversion ────────────────────────────────

#[allow(clippy::missing_const_for_fn)]
pub(crate) fn to_key_input(key: KeyEvent) -> KeyInput {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let code = match key.code {
        KeyCode::Char(c) => VKeyCode::Char(c),
        KeyCode::Esc => VKeyCode::Esc,
        KeyCode::Enter => VKeyCode::Enter,
        KeyCode::Backspace => VKeyCode::Backspace,
        KeyCode::Delete => VKeyCode::Delete,
        KeyCode::Left => VKeyCode::Left,
        KeyCode::Right => VKeyCode::Right,
        KeyCode::Up => VKeyCode::Up,
        KeyCode::Down => VKeyCode::Down,
        _ => return KeyInput { code: VKeyCode::Esc, ctrl },
    };
    KeyInput { code, ctrl }
}

// ── Key dispatch ──────────────────────────────────

pub(crate) fn handle_key(editor: &mut Editor, key: KeyEvent) {
    match editor.mode {
        Mode::Normal => handle_normal(editor, key),
        Mode::Insert => handle_insert(editor, key),
        Mode::Command => handle_command(editor, key),
        Mode::Search { backward } => handle_search_key(editor, key, backward),
        Mode::Visual { .. } => handle_visual(editor, key),
    }
}

pub(crate) fn handle_normal(editor: &mut Editor, key: KeyEvent) {
    if key.code == KeyCode::Esc {
        // Esc in normal mode clears search highlight
        // (user's `nohlsearch` mapping, built in).
        editor.search.active = false;
    }
    let input = to_key_input(key);
    let action = editor.vim.feed(input);
    execute_action(editor, action);
}

pub(crate) fn handle_visual(editor: &mut Editor, key: KeyEvent) {
    let input = to_key_input(key);
    let action = editor.vim.feed_visual(input);
    execute_action(editor, action);
}

pub(crate) fn handle_insert(editor: &mut Editor, key: KeyEvent) {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('w') => {
                delete_word_back(editor);
                return;
            }
            KeyCode::Char('u') => {
                delete_to_line_start(editor);
                return;
            }
            _ => {}
        }
    }

    match key.code {
        KeyCode::Esc => editor.enter_normal(),
        KeyCode::Char(c) => insert_char(editor, c),
        KeyCode::Enter => insert_newline(editor),
        KeyCode::Backspace => {
            delete_char_before_cursor(editor);
        }
        KeyCode::Delete => {
            delete_char_at_cursor(editor, 1);
        }
        KeyCode::Left => editor.view.move_left(),
        KeyCode::Right => {
            let max = editor.current_line_len();
            editor.view.move_right(max);
        }
        KeyCode::Up => {
            let doc = &editor.document;
            editor.view.move_up(|line| doc.line_len(line));
        }
        KeyCode::Down => {
            let max = editor.max_line();
            let doc = &editor.document;
            editor.view.move_down(max, |line| doc.line_len(line));
        }
        _ => {}
    }
}

pub(crate) fn handle_command(editor: &mut Editor, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => editor.enter_normal(),
        KeyCode::Enter => {
            let cmd = editor.command_buffer.clone();
            editor.enter_normal();
            execute_command(editor, &cmd);
        }
        KeyCode::Backspace => {
            if editor.command_buffer.is_empty() {
                editor.enter_normal();
            } else {
                editor.command_buffer.pop();
            }
        }
        KeyCode::Up => history_up(editor),
        KeyCode::Down => history_down(editor),
        KeyCode::Char(c) => {
            editor.command_buffer.push(c);
        }
        _ => {}
    }
}

fn history_up(editor: &mut Editor) {
    if editor.command_history.is_empty() {
        return;
    }
    let pos = editor
        .history_pos
        .map_or(editor.command_history.len() - 1, |p| p.saturating_sub(1));
    editor.history_pos = Some(pos);
    if let Some(cmd) = editor.command_history.get(pos) {
        editor.command_buffer.clone_from(cmd);
    }
}

fn history_down(editor: &mut Editor) {
    let Some(pos) = editor.history_pos else {
        return;
    };
    if pos + 1 >= editor.command_history.len() {
        editor.history_pos = None;
        editor.command_buffer.clear();
    } else {
        editor.history_pos = Some(pos + 1);
        if let Some(cmd) = editor.command_history.get(pos + 1) {
            editor.command_buffer.clone_from(cmd);
        }
    }
}

// ── Search prompt ─────────────────────────────────

pub(crate) fn handle_search_key(
    editor: &mut Editor,
    key: KeyEvent,
    backward: bool,
) {
    match key.code {
        KeyCode::Esc => {
            // Cancel: restore the cursor to where the
            // search started.
            let origin = editor.search_origin;
            editor.enter_normal();
            set_cursor_from_pos(editor, origin);
            editor.command_buffer.clear();
        }
        KeyCode::Enter => {
            commit_search(editor, backward);
        }
        KeyCode::Backspace => {
            if editor.command_buffer.is_empty() {
                let origin = editor.search_origin;
                editor.enter_normal();
                set_cursor_from_pos(editor, origin);
            } else {
                editor.command_buffer.pop();
                incremental_search(editor, backward);
            }
        }
        KeyCode::Char(c) => {
            editor.command_buffer.push(c);
            incremental_search(editor, backward);
        }
        _ => {}
    }
}

/// Live cursor preview while typing the pattern
/// (vim 'incsearch').
fn incremental_search(editor: &mut Editor, backward: bool) {
    let origin = editor.search_origin;
    let found = search::compile(&editor.command_buffer).and_then(|re| {
        if backward {
            search::find_backward(&editor.document.text, &re, origin)
        } else {
            search::find_forward(&editor.document.text, &re, origin + 1)
        }
    });
    set_cursor_from_pos(editor, found.map_or(origin, |(start, _)| start));
}

fn commit_search(editor: &mut Editor, backward: bool) {
    let origin = editor.search_origin;
    let pattern = if editor.command_buffer.is_empty() {
        // Bare `/` repeats the previous search.
        editor.search.pattern.clone()
    } else {
        editor.command_buffer.clone()
    };
    editor.enter_normal();
    editor.command_buffer.clear();

    if pattern.is_empty() {
        editor.status_message = Some("No previous search".to_owned());
        return;
    }
    let Some(re) = search::compile(&pattern) else {
        editor.status_message = Some(format!("Invalid pattern: {pattern}"));
        set_cursor_from_pos(editor, origin);
        return;
    };
    let found = if backward {
        search::find_backward(&editor.document.text, &re, origin)
    } else {
        search::find_forward(&editor.document.text, &re, origin + 1)
    };
    editor.search.pattern.clone_from(&pattern);
    editor.search.backward = backward;
    editor.search.active = true;
    if let Some((start, _)) = found {
        set_cursor_from_pos(editor, start);
    } else {
        editor.status_message = Some(format!("Pattern not found: {pattern}"));
        set_cursor_from_pos(editor, origin);
    }
}

/// `n` / `N`: jump to the next/previous match of the
/// committed pattern.
fn search_next(editor: &mut Editor, reverse: bool, count: usize) {
    if editor.search.pattern.is_empty() {
        editor.status_message = Some("No previous search".to_owned());
        return;
    }
    let Some(re) = search::compile(&editor.search.pattern) else {
        return;
    };
    let backward = editor.search.backward != reverse;
    let mut pos = cursor_pos(editor);
    for _ in 0..count.max(1) {
        let found = if backward {
            search::find_backward(&editor.document.text, &re, pos)
        } else {
            search::find_forward(&editor.document.text, &re, pos + 1)
        };
        if let Some((start, _)) = found {
            pos = start;
        } else {
            editor.status_message =
                Some(format!("Pattern not found: {}", editor.search.pattern,));
            return;
        }
    }
    editor.search.active = true;
    set_cursor_from_pos(editor, pos);
}

/// `*` / `#`: whole-word search for the word under
/// the cursor.
fn search_word(editor: &mut Editor, backward: bool, count: usize) {
    let text = &editor.document.text;
    let pos = cursor_pos(editor);
    if pos >= text.len_chars()
        || movement::char_category(text.char(pos), false)
            != movement::CharCat::Word
    {
        editor.status_message = Some("No word under cursor".to_owned());
        return;
    }
    let Some((start, end)) = textobject::word(text, pos, 1, false, false)
    else {
        return;
    };
    let word: String = text.slice(start..end).chars().collect();
    editor.search.pattern = search::word_pattern(&word);
    editor.search.backward = backward;
    editor.search.active = true;
    // Jump off from the start of the current word so
    // `#` finds the previous occurrence, not this one.
    set_cursor_from_pos(editor, start);
    search_next(editor, false, count.max(1));
}

pub(crate) fn execute_command(editor: &mut Editor, cmd: &str) {
    let trimmed = cmd.trim();
    if !trimmed.is_empty()
        && editor.command_history.last().map(String::as_str) != Some(trimmed)
    {
        editor.command_history.push(trimmed.to_owned());
    }

    match command_line::parse(cmd) {
        ExCommand::Empty => {}
        ExCommand::Quit { force } => {
            if editor.document.modified && !force {
                editor.status_message = Some(
                    "No write since last change \
                     (add ! to override)"
                        .to_owned(),
                );
            } else {
                editor.should_quit = true;
            }
        }
        ExCommand::Write { path } => {
            if let Some(path) = path {
                editor.document.path = Some(path.into());
            }
            match editor.document.save() {
                Ok(()) => {
                    let name = editor.document.path.as_ref().map_or_else(
                        || "[scratch]".to_owned(),
                        |p| p.display().to_string(),
                    );
                    editor.status_message =
                        Some(format!("\"{name}\" written"));
                }
                Err(e) => {
                    editor.status_message = Some(format!("Error: {e}"));
                }
            }
        }
        ExCommand::WriteQuit => {
            if let Err(e) = editor.document.save() {
                editor.status_message = Some(format!("Error: {e}"));
            } else {
                editor.should_quit = true;
            }
        }
        ExCommand::Edit { path, force } => {
            ex_edit(editor, &path, force);
        }
        ExCommand::Substitute {
            range,
            pattern,
            replacement,
            global,
            ignore_case,
        } => {
            ex_substitute(
                editor,
                range,
                &pattern,
                &replacement,
                global,
                ignore_case,
            );
        }
        ExCommand::DeleteLines { range } => {
            ex_delete_lines(editor, range);
        }
        ExCommand::Goto(addr) => {
            if let Some(line) = resolve_address(editor, addr) {
                editor.view.cursor_line = line;
                editor.view.set_col(0);
                editor.view.ensure_cursor_visible();
            }
        }
        ExCommand::SetNumber(value) => {
            editor.show_numbers = value.unwrap_or(!editor.show_numbers);
        }
        ExCommand::Theme(name) => match name {
            None => {
                editor.status_message =
                    Some(format!("Current theme: {}", editor.theme.name,));
            }
            Some(name) => {
                if let Some(theme) = builtin_theme(&name) {
                    editor.status_message = Some(format!("Theme: {name}"));
                    editor.theme = theme;
                } else {
                    editor.status_message =
                        Some(format!("Unknown theme: {name}"));
                }
            }
        },
        ExCommand::Unknown(cmd) => {
            editor.status_message =
                Some(format!("Not an editor command: {cmd}"));
        }
    }
}

// ── Ex command execution ──────────────────────────

/// Resolve an ex address to a 0-based line index.
fn resolve_address(editor: &Editor, addr: Address) -> Option<usize> {
    match addr {
        Address::Line(n) => Some(n.saturating_sub(1).min(editor.max_line())),
        Address::Current => Some(editor.view.cursor_line),
        Address::Last => Some(editor.max_line()),
        Address::Mark(c) => editor.marks.get(&c).map(|&pos| {
            editor
                .document
                .text
                .char_to_line(pos.min(editor.document.text.len_chars()))
        }),
    }
}

/// Resolve a range to inclusive 0-based line indices;
/// defaults to the cursor line.
fn resolve_range(
    editor: &mut Editor,
    range: Option<ExRange>,
) -> Option<(usize, usize)> {
    let Some(range) = range else {
        let line = editor.view.cursor_line;
        return Some((line, line));
    };
    let (Some(start), Some(end)) = (
        resolve_address(editor, range.start),
        resolve_address(editor, range.end),
    ) else {
        editor.status_message = Some("Mark not set".to_owned());
        return None;
    };
    Some(if start <= end { (start, end) } else { (end, start) })
}

fn ex_substitute(
    editor: &mut Editor,
    range: Option<ExRange>,
    pattern: &str,
    replacement: &str,
    global: bool,
    ignore_case: bool,
) {
    let Some((start_line, end_line)) = resolve_range(editor, range) else {
        return;
    };
    let source = if ignore_case {
        format!("(?i){pattern}")
    } else {
        pattern.to_owned()
    };
    let Some(re) = search::compile(&source) else {
        editor.status_message = Some(format!("Invalid pattern: {pattern}"));
        return;
    };
    let repl = command_line::translate_backrefs(replacement);

    let mut new_text = String::new();
    let mut substitutions = 0usize;
    let mut lines_changed = 0usize;
    let mut last_changed = start_line;
    for line in start_line..=end_line {
        let original: String = editor
            .document
            .line(line)
            .map(|l| l.chars().collect())
            .unwrap_or_default();
        let (content, newline) = original
            .strip_suffix('\n')
            .map_or((original.as_str(), ""), |c| (c, "\n"));
        let count = if global {
            re.find_iter(content).count()
        } else {
            usize::from(re.is_match(content))
        };
        if count > 0 {
            substitutions += count;
            lines_changed += 1;
            last_changed = line;
            let replaced = if global {
                re.replace_all(content, repl.as_str())
            } else {
                re.replace(content, repl.as_str())
            };
            new_text.push_str(&replaced);
        } else {
            new_text.push_str(content);
        }
        new_text.push_str(newline);
    }

    if substitutions == 0 {
        editor.status_message = Some(format!("Pattern not found: {pattern}"));
        return;
    }

    let start = editor.document.text.line_to_char(start_line);
    let end = if end_line + 1 < editor.document.text.len_lines() {
        editor.document.text.line_to_char(end_line + 1)
    } else {
        editor.document.text.len_chars()
    };
    let txn = Transaction::replace(start, end - start, &new_text);
    let inv = txn.invert(&editor.document.text);
    if editor.document.apply_transaction(&txn).is_ok() {
        editor.history.commit(txn, inv);
        editor.view.cursor_line = last_changed.min(editor.max_line());
        let col = editor.first_non_blank_col(editor.view.cursor_line);
        editor.view.set_col(col);
        editor.view.ensure_cursor_visible();
        // `:s` patterns become the active search.
        editor.search.pattern = source;
        editor.search.active = true;
        if substitutions > 1 {
            editor.status_message = Some(format!(
                "{substitutions} substitutions on {lines_changed} lines",
            ));
        }
    }
}

fn ex_delete_lines(editor: &mut Editor, range: Option<ExRange>) {
    let Some((start_line, end_line)) = resolve_range(editor, range) else {
        return;
    };
    let start = editor.document.text.line_to_char(start_line);
    let end = if end_line + 1 < editor.document.text.len_lines() {
        editor.document.text.line_to_char(end_line + 1)
    } else {
        editor.document.text.len_chars()
    };
    if start >= end {
        return;
    }
    let mut text: String =
        editor.document.text.slice(start..end).chars().collect();
    if !text.ends_with('\n') {
        text.push('\n');
    }
    store_yank(editor, text, true);
    apply_delete(editor, start, end, MotionType::Linewise);
}

fn ex_edit(editor: &mut Editor, path: &str, force: bool) {
    if editor.document.modified && !force {
        editor.status_message = Some(
            "No write since last change \
             (add ! to override)"
                .to_owned(),
        );
        return;
    }
    let path_buf = std::path::PathBuf::from(path);
    let document = match ms_view::document::Document::open(&path_buf) {
        Ok(doc) => doc,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // New file: empty buffer with this path.
            let mut doc = ms_view::document::Document::scratch();
            doc.path = Some(path_buf);
            editor.status_message = Some(format!("\"{path}\" [new file]"));
            doc
        }
        Err(e) => {
            editor.status_message = Some(format!("Error: {e}"));
            return;
        }
    };
    let lines = document.line_count();
    if editor.status_message.is_none() {
        editor.status_message = Some(format!("\"{path}\" {lines} lines"));
    }
    editor.document = document;
    editor.history = ms_core::history::History::new();
    editor.marks.clear();
    editor.view.cursor_line = 0;
    editor.view.set_col(0);
    editor.view.scroll_offset = 0;
}

// ── Action dispatch ───────────────────────────────

#[allow(clippy::needless_pass_by_value)]
pub(crate) fn execute_action(editor: &mut Editor, action: Action) {
    // `"x` only applies to the immediately following
    // command; any other executed action consumes it.
    let keep_register =
        matches!(action, Action::SelectRegister(_) | Action::None);
    match action {
        Action::Move(motion, count) => {
            let motion = remember_or_repeat_find(editor, motion);
            execute_motion(editor, motion, count);
        }
        Action::OperatorMotion { operator, motion, count } => {
            // Vim special case: cw/cW behaves like ce/cE
            let motion = if operator == Operator::Change {
                match motion {
                    Motion::WordStart => Motion::WordEnd,
                    Motion::WordStartBig => Motion::WordEndBig,
                    m => m,
                }
            } else {
                motion
            };
            let motion = remember_or_repeat_find(editor, motion);
            let (start, end, mt) = motion_range(editor, motion, count);
            if start != end {
                apply_operator(editor, operator, start, end, mt);
            }
        }
        Action::OperatorLine { operator, count } => {
            if operator == Operator::Change {
                // cc: clear line content, keep newline
                change_lines(editor, count);
            } else {
                let (start, end) = line_range(editor, count);
                apply_operator(
                    editor,
                    operator,
                    start,
                    end,
                    MotionType::Linewise,
                );
            }
        }
        Action::EnterInsert(variant) => {
            execute_insert(editor, variant);
        }
        Action::EnterCommand => {
            let visual_range_marks =
                if let Mode::Visual { anchor, .. } = editor.mode {
                    let head = cursor_pos(editor);
                    Some((anchor.min(head), anchor.max(head)))
                } else {
                    None
                };
            editor.enter_command();
            if let Some((start, end)) = visual_range_marks {
                // vim: `:` from visual prefills '<,'>
                editor.marks.insert('<', start);
                editor.marks.insert('>', end);
                editor.command_buffer.push_str("'<,'>");
            }
        }
        Action::EnterSearch { backward } => {
            editor.search_origin = cursor_pos(editor);
            editor.command_buffer.clear();
            editor.mode = Mode::Search { backward };
        }
        Action::Special(cmd, count) => {
            execute_special(editor, cmd, count);
        }
        Action::EnterVisual(kind) => {
            enter_visual(editor, kind);
        }
        Action::ExitVisual => {
            editor.enter_normal();
        }
        Action::SwapAnchor => {
            swap_visual_anchor(editor);
        }
        Action::VisualOperator(operator) => {
            visual_operator(editor, operator);
        }
        Action::OperatorTextObject { operator, around, target, count } => {
            operator_textobject(editor, operator, around, target, count);
        }
        Action::VisualTextObject { around, target, count } => {
            select_textobject(editor, around, target, count);
        }
        Action::SelectRegister(reg) => {
            editor.yank_register = reg;
        }
        Action::None => {}
    }
    if !keep_register {
        editor.yank_register = '"';
    }
}

// ── Visual mode ───────────────────────────────────

/// Enter visual mode, or toggle/switch the kind when
/// already in visual mode (vim `v`/`V`).
pub(crate) fn enter_visual(editor: &mut Editor, kind: VisualKind) {
    if let Mode::Visual { kind: current, anchor } = editor.mode {
        if current == kind {
            editor.enter_normal();
        } else {
            editor.mode = Mode::Visual { kind, anchor };
        }
    } else {
        let anchor = cursor_pos(editor);
        editor.mode = Mode::Visual { kind, anchor };
    }
}

/// Swap anchor and cursor (vim `o`).
pub(crate) fn swap_visual_anchor(editor: &mut Editor) {
    if let Mode::Visual { kind, anchor } = editor.mode {
        let head = cursor_pos(editor);
        editor.mode = Mode::Visual { kind, anchor: head };
        set_cursor_from_pos(editor, anchor);
        editor.view.desired_col = editor.view.cursor_col;
    }
}

/// Char range covered by the visual selection.
/// Charwise is head-inclusive (vim semantics);
/// linewise expands to whole lines.
pub(crate) fn visual_range(
    editor: &Editor,
    kind: VisualKind,
    anchor: usize,
) -> (usize, usize, MotionType) {
    let head = cursor_pos(editor);
    let (lo, hi) =
        if anchor <= head { (anchor, head) } else { (head, anchor) };

    match kind {
        VisualKind::Char | VisualKind::Block => {
            let end = (hi + 1).min(editor.document.text.len_chars());
            (lo, end, MotionType::Charwise)
        }
        VisualKind::Line => {
            let start_line = editor.document.text.char_to_line(lo);
            let end_line = editor.document.text.char_to_line(hi);
            let start = editor.document.text.line_to_char(start_line);
            let end = if end_line + 1 < editor.document.text.len_lines() {
                editor.document.text.line_to_char(end_line + 1)
            } else {
                editor.document.text.len_chars()
            };
            (start, end, MotionType::Linewise)
        }
    }
}

/// Apply an operator to the visual selection and
/// return to normal mode (or insert for change).
pub(crate) fn visual_operator(editor: &mut Editor, operator: Operator) {
    let Mode::Visual { kind, anchor } = editor.mode else {
        return;
    };
    let (start, end, mt) = visual_range(editor, kind, anchor);
    editor.mode = Mode::Normal;

    if operator == Operator::Change && mt == MotionType::Linewise {
        // Linewise change keeps an empty line open,
        // like `cc` (vim `Vc`).
        let start_line = editor.document.text.char_to_line(start);
        let end_line = editor
            .document
            .text
            .char_to_line(end.saturating_sub(1).max(start));
        set_cursor_from_pos(editor, start);
        change_lines(editor, end_line - start_line + 1);
        return;
    }

    match operator {
        Operator::Yank
        | Operator::Lowercase
        | Operator::Uppercase
        | Operator::ToggleCase
        | Operator::Indent
        | Operator::Dedent => {
            // Vim leaves the cursor at selection start.
            set_cursor_from_pos(editor, start);
        }
        Operator::Delete | Operator::Change => {}
    }
    apply_operator(editor, operator, start, end, mt);
    editor.clamp_cursor_col();
}

/// Resolve a text object at the cursor to a char
/// range. Paragraphs are linewise, everything else
/// charwise.
pub(crate) fn textobject_range(
    editor: &Editor,
    target: TextObjectTarget,
    around: bool,
    count: usize,
) -> Option<(usize, usize, MotionType)> {
    let text = &editor.document.text;
    let pos = cursor_pos(editor);
    let (start, end) = match target {
        TextObjectTarget::Word { big } => {
            textobject::word(text, pos, count, big, around)?
        }
        TextObjectTarget::Paragraph => {
            textobject::paragraph(text, pos, count, around)?
        }
        TextObjectTarget::Quote(c) => textobject::quote(text, pos, c, around)?,
        TextObjectTarget::Pair { open, close } => {
            textobject::pair(text, pos, open, close, count, around)?
        }
    };
    let mt = if matches!(target, TextObjectTarget::Paragraph) {
        MotionType::Linewise
    } else {
        MotionType::Charwise
    };
    Some((start, end, mt))
}

/// Operator + text object from normal mode (`diw`).
pub(crate) fn operator_textobject(
    editor: &mut Editor,
    operator: Operator,
    around: bool,
    target: TextObjectTarget,
    count: usize,
) {
    let Some((start, end, mt)) =
        textobject_range(editor, target, around, count)
    else {
        return;
    };
    if start >= end {
        // Empty inner object (`ci(` on `()`): change
        // still enters insert at the gap.
        if operator == Operator::Change {
            set_insert_at(editor, start);
        }
        return;
    }
    if operator == Operator::Change && mt == MotionType::Linewise {
        // Linewise change keeps an empty line open,
        // like `cc` (vim `cip`).
        let start_line = editor.document.text.char_to_line(start);
        let end_line = editor
            .document
            .text
            .char_to_line(end.saturating_sub(1).max(start));
        set_cursor_from_pos(editor, start);
        change_lines(editor, end_line - start_line + 1);
        return;
    }
    apply_operator(editor, operator, start, end, mt);
}

/// Select a text object in visual mode (`viw`).
pub(crate) fn select_textobject(
    editor: &mut Editor,
    around: bool,
    target: TextObjectTarget,
    count: usize,
) {
    let Mode::Visual { kind, .. } = editor.mode else {
        return;
    };
    let Some((start, end, _mt)) =
        textobject_range(editor, target, around, count)
    else {
        return;
    };
    if start >= end {
        return;
    }
    editor.mode = Mode::Visual { kind, anchor: start };
    set_cursor_from_pos(editor, end - 1);
    editor.view.desired_col = editor.view.cursor_col;
}

/// Replace the visual selection with register
/// contents (vim visual `p`).
pub(crate) fn paste_over_selection(editor: &mut Editor) {
    let Mode::Visual { kind, anchor } = editor.mode else {
        return;
    };
    let reg = editor.yank_register;
    let Some(new_text) = register_text(editor, reg) else {
        editor.enter_normal();
        return;
    };
    let (start, end, _mt) = visual_range(editor, kind, anchor);
    editor.mode = Mode::Normal;

    let replaced: String =
        editor.document.text.slice(start..end).chars().collect();
    let txn = Transaction::replace(start, end - start, &new_text);
    let inv = txn.invert(&editor.document.text);
    if editor.document.apply_transaction(&txn).is_ok() {
        editor.history.commit(txn, inv);
        // Replaced text goes to the unnamed register.
        editor.registers.write('"', replaced);
        let last = start + new_text.chars().count();
        set_cursor_from_pos(editor, last.saturating_sub(1).max(start));
        editor.clamp_cursor_col();
    }
}

/// Enter insert mode with the cursor at an exact char
/// position (may be one past line end).
fn set_insert_at(editor: &mut Editor, pos: usize) {
    editor.mode = Mode::Insert;
    let pos = pos.min(editor.document.text.len_chars());
    let (line, col) = editor.document.char_to_line_col(pos);
    editor.view.cursor_line = line;
    editor.view.set_col(col);
    editor.view.ensure_cursor_visible();
}

/// Record `f`/`F`/`t`/`T` for later `;`/`,` repeat,
/// and substitute `;`/`,` with the recorded find.
fn remember_or_repeat_find(editor: &mut Editor, motion: Motion) -> Motion {
    match motion {
        Motion::FindChar(_)
        | Motion::FindCharBack(_)
        | Motion::TillChar(_)
        | Motion::TillCharBack(_) => {
            editor.last_find = Some(motion);
            motion
        }
        Motion::RepeatFind => editor.last_find.unwrap_or(motion),
        Motion::RepeatFindReverse => {
            editor.last_find.map_or(motion, reverse_find)
        }
        m => m,
    }
}

const fn reverse_find(motion: Motion) -> Motion {
    match motion {
        Motion::FindChar(c) => Motion::FindCharBack(c),
        Motion::FindCharBack(c) => Motion::FindChar(c),
        Motion::TillChar(c) => Motion::TillCharBack(c),
        Motion::TillCharBack(c) => Motion::TillChar(c),
        m => m,
    }
}

// ── Motion execution ──────────────────────────────

pub(crate) fn cursor_pos(editor: &Editor) -> usize {
    editor
        .document
        .line_col_to_char(editor.view.cursor_line, editor.view.cursor_col)
}

pub(crate) fn set_cursor_from_pos(editor: &mut Editor, pos: usize) {
    let (line, col) = editor.document.char_to_line_col(pos);
    editor.view.cursor_line = line;
    editor.view.set_col(col);
    editor.view.ensure_cursor_visible();
}

#[allow(clippy::too_many_lines)]
pub(crate) fn resolve_motion(
    editor: &Editor,
    motion: Motion,
    count: usize,
) -> usize {
    let text = &editor.document.text;
    let pos = cursor_pos(editor);

    match motion {
        Motion::Left => pos.saturating_sub(count),
        Motion::Right => {
            let max = text.len_chars().saturating_sub(1);
            (pos + count).min(max)
        }
        Motion::Down => {
            let target_line =
                (editor.view.cursor_line + count).min(editor.max_line());
            let col = editor
                .view
                .desired_col
                .min(normal_max_col(editor, target_line));
            editor.document.line_col_to_char(target_line, col)
        }
        Motion::Up => {
            let target_line = editor.view.cursor_line.saturating_sub(count);
            let col = editor
                .view
                .desired_col
                .min(normal_max_col(editor, target_line));
            editor.document.line_col_to_char(target_line, col)
        }
        Motion::WordStart => {
            let mut p = pos;
            for _ in 0..count {
                p = movement::next_word_start(text, p, false);
            }
            p
        }
        Motion::WordStartBig => {
            let mut p = pos;
            for _ in 0..count {
                p = movement::next_word_start(text, p, true);
            }
            p
        }
        Motion::WordEnd => {
            let mut p = pos;
            for _ in 0..count {
                p = movement::next_word_end(text, p, false);
            }
            p
        }
        Motion::WordEndBig => {
            let mut p = pos;
            for _ in 0..count {
                p = movement::next_word_end(text, p, true);
            }
            p
        }
        Motion::BackWord => {
            let mut p = pos;
            for _ in 0..count {
                p = movement::prev_word_start(text, p, false);
            }
            p
        }
        Motion::BackWordBig => {
            let mut p = pos;
            for _ in 0..count {
                p = movement::prev_word_start(text, p, true);
            }
            p
        }
        Motion::LineStart => {
            editor.document.line_col_to_char(editor.view.cursor_line, 0)
        }
        Motion::LineEnd => {
            let len = editor.current_line_len();
            editor.document.line_col_to_char(
                editor.view.cursor_line,
                if len == 0 { 0 } else { len - 1 },
            )
        }
        Motion::FirstNonBlank => {
            let col = editor.first_non_blank_col(editor.view.cursor_line);
            editor.document.line_col_to_char(editor.view.cursor_line, col)
        }
        Motion::GotoTop => editor.document.line_col_to_char(0, 0),
        Motion::GotoBottom => {
            let line = editor.max_line();
            let col =
                normal_max_col(editor, line).min(editor.view.desired_col);
            editor.document.line_col_to_char(line, col)
        }
        Motion::GotoLine => {
            let line = (count.saturating_sub(1)).min(editor.max_line());
            editor.document.line_col_to_char(line, 0)
        }
        Motion::ParagraphForward => movement::paragraph_forward(text, pos),
        Motion::ParagraphBackward => movement::paragraph_backward(text, pos),
        Motion::FindChar(c) => {
            let mut p = pos;
            for _ in 0..count {
                if let Some(np) = movement::find_char_forward(text, p, c) {
                    p = np;
                } else {
                    return pos;
                }
            }
            p
        }
        Motion::FindCharBack(c) => {
            let mut p = pos;
            for _ in 0..count {
                if let Some(np) = movement::find_char_backward(text, p, c) {
                    p = np;
                } else {
                    return pos;
                }
            }
            p
        }
        Motion::TillChar(c) => {
            let mut p = pos;
            for _ in 0..count {
                if let Some(np) = movement::till_char_forward(text, p, c) {
                    p = np;
                } else {
                    return pos;
                }
            }
            p
        }
        Motion::TillCharBack(c) => {
            let mut p = pos;
            for _ in 0..count {
                if let Some(np) = movement::till_char_backward(text, p, c) {
                    p = np;
                } else {
                    return pos;
                }
            }
            p
        }
        Motion::MatchBracket => {
            movement::find_matching_bracket(text, pos).unwrap_or(pos)
        }
        Motion::MarkLine(c) => editor.marks.get(&c).map_or(pos, |&mark| {
            let line =
                editor.document.text.char_to_line(mark.min(text.len_chars()));
            let col = editor.first_non_blank_col(line);
            editor.document.line_col_to_char(line, col)
        }),
        Motion::MarkChar(c) => editor
            .marks
            .get(&c)
            .map_or(pos, |&mark| mark.min(text.len_chars().saturating_sub(1))),
        // `;`/`,` are substituted by
        // `remember_or_repeat_find` before resolution;
        // with no recorded find they are no-ops.
        Motion::RepeatFind | Motion::RepeatFindReverse => pos,
        Motion::ScreenTop => {
            let line = editor.view.scroll_offset;
            editor.document.line_col_to_char(line, 0)
        }
        Motion::ScreenMiddle => {
            let mid =
                editor.view.scroll_offset + (editor.view.height as usize / 2);
            let line = mid.min(editor.max_line());
            editor.document.line_col_to_char(line, 0)
        }
        Motion::ScreenBottom => {
            let bot =
                editor.view.scroll_offset + editor.view.height as usize - 1;
            let line = bot.min(editor.max_line());
            editor.document.line_col_to_char(line, 0)
        }
    }
}

pub(crate) fn normal_max_col(editor: &Editor, line: usize) -> usize {
    let len = editor.document.line_len(line);
    if len == 0 {
        0
    } else {
        len - 1
    }
}

pub(crate) fn execute_motion(
    editor: &mut Editor,
    motion: Motion,
    count: usize,
) {
    let new_pos = resolve_motion(editor, motion, count);
    set_cursor_from_pos(editor, new_pos);

    // Update desired_col for non-vertical motions
    match motion {
        Motion::Down | Motion::Up => {}
        Motion::LineEnd => {
            editor.view.desired_col = usize::MAX;
        }
        _ => {
            editor.view.desired_col = editor.view.cursor_col;
        }
    }
}

// ── Motion range (for operators) ──────────────────

pub(crate) fn motion_range(
    editor: &Editor,
    motion: Motion,
    count: usize,
) -> (usize, usize, MotionType) {
    let start = cursor_pos(editor);
    let end = resolve_motion(editor, motion, count);
    let mt = motion.motion_type();

    let (lo, hi) = if start <= end { (start, end) } else { (end, start) };

    match mt {
        MotionType::Charwise => {
            // Inclusive motions include the endpoint;
            // exclusive motions don't.
            let end_pos = if motion.is_inclusive() {
                (hi + 1).min(editor.document.text.len_chars())
            } else {
                hi
            };
            (lo, end_pos, mt)
        }
        MotionType::Linewise => {
            let start_line = editor.document.text.char_to_line(lo);
            let end_line = editor.document.text.char_to_line(hi);
            let line_start = editor.document.text.line_to_char(start_line);
            let line_end = if end_line + 1 < editor.document.text.len_lines() {
                editor.document.text.line_to_char(end_line + 1)
            } else {
                editor.document.text.len_chars()
            };
            (line_start, line_end, mt)
        }
    }
}

pub(crate) fn line_range(editor: &Editor, count: usize) -> (usize, usize) {
    let line = editor.view.cursor_line;
    let end_line = (line + count - 1).min(editor.max_line());
    let start = editor.document.text.line_to_char(line);
    let end = if end_line + 1 < editor.document.text.len_lines() {
        editor.document.text.line_to_char(end_line + 1)
    } else {
        editor.document.text.len_chars()
    };
    (start, end)
}

// ── Operator application ──────────────────────────

pub(crate) fn apply_operator(
    editor: &mut Editor,
    op: Operator,
    start: usize,
    end: usize,
    mt: MotionType,
) {
    if start >= end {
        return;
    }
    let mut text: String =
        editor.document.text.slice(start..end).chars().collect();
    // Linewise yanks always paste as whole lines, even
    // when the last line has no trailing newline.
    if mt == MotionType::Linewise && !text.ends_with('\n') {
        text.push('\n');
    }

    match op {
        Operator::Delete => {
            store_yank(editor, text, true);
            apply_delete(editor, start, end, mt);
        }
        Operator::Change => {
            store_yank(editor, text, true);
            apply_delete(editor, start, end, mt);
            // Insert exactly at the deletion point,
            // which may be one past line end (`ciw` on
            // the last word of a line).
            set_insert_at(editor, start);
        }
        Operator::Yank => {
            store_yank(editor, text, false);
        }
        Operator::Indent => {
            apply_indent(editor, start, end, true);
        }
        Operator::Dedent => {
            apply_indent(editor, start, end, false);
        }
        Operator::Lowercase => {
            apply_case(editor, start, end, CaseOp::Lower);
        }
        Operator::Uppercase => {
            apply_case(editor, start, end, CaseOp::Upper);
        }
        Operator::ToggleCase => {
            apply_case(editor, start, end, CaseOp::Toggle);
        }
    }
}

pub(crate) fn apply_delete(
    editor: &mut Editor,
    start: usize,
    end: usize,
    mt: MotionType,
) {
    let len = end - start;
    let txn = Transaction::delete(start, len);
    let inv = txn.invert(&editor.document.text);
    if editor.document.apply_transaction(&txn).is_ok() {
        editor.history.commit(txn, inv);
        // Position cursor at start of deleted range
        let new_pos =
            start.min(editor.document.text.len_chars().saturating_sub(1));
        set_cursor_from_pos(editor, new_pos);
        if mt == MotionType::Linewise {
            // After linewise delete, go to first
            // non-blank
            let col = editor.first_non_blank_col(editor.view.cursor_line);
            editor.view.set_col(col);
        }
        editor.clamp_cursor_col();
    }
}

pub(crate) fn change_lines(editor: &mut Editor, count: usize) {
    let line = editor.view.cursor_line;

    if count == 1 {
        // Single line: clear content, keep newline
        let len = editor.document.line_len(line);
        if len > 0 {
            let start = editor.document.line_col_to_char(line, 0);
            let text: String = editor
                .document
                .text
                .slice(start..start + len)
                .chars()
                .collect();
            store_yank(editor, text, true);
            let txn = Transaction::delete(start, len);
            let inv = txn.invert(&editor.document.text);
            if editor.document.apply_transaction(&txn).is_ok() {
                editor.history.commit(txn, inv);
            }
        }
    } else {
        // Multi-line: delete extra lines, clear first
        let (start, end) = line_range(editor, count);
        // Keep the first line's newline
        let first_nl = editor
            .document
            .line_col_to_char(line, editor.document.line_len(line));
        // Delete from after first line's newline
        // through end of range, then clear first line
        let text: String =
            editor.document.text.slice(start..end).chars().collect();
        store_yank(editor, text, true);

        // Delete lines after first
        if first_nl + 1 < end {
            let txn = Transaction::delete(first_nl + 1, end - first_nl - 1);
            let inv = txn.invert(&editor.document.text);
            if editor.document.apply_transaction(&txn).is_ok() {
                editor.history.commit(txn, inv);
            }
        }
        // Clear first line content
        let len = editor.document.line_len(line);
        if len > 0 {
            let txn = Transaction::delete(
                editor.document.line_col_to_char(line, 0),
                len,
            );
            let inv = txn.invert(&editor.document.text);
            if editor.document.apply_transaction(&txn).is_ok() {
                editor.history.commit(txn, inv);
            }
        }
    }
    editor.view.set_col(0);
    editor.mode = Mode::Insert;
}

#[derive(Clone, Copy)]
pub(crate) enum CaseOp {
    Lower,
    Upper,
    Toggle,
}

pub(crate) fn apply_case(
    editor: &mut Editor,
    start: usize,
    end: usize,
    op: CaseOp,
) {
    let text: String =
        editor.document.text.slice(start..end).chars().collect();
    let new_text: String = text
        .chars()
        .map(|c| match op {
            CaseOp::Lower => c.to_lowercase().next().unwrap_or(c),
            CaseOp::Upper => c.to_uppercase().next().unwrap_or(c),
            CaseOp::Toggle => {
                if c.is_uppercase() {
                    c.to_lowercase().next().unwrap_or(c)
                } else {
                    c.to_uppercase().next().unwrap_or(c)
                }
            }
        })
        .collect();
    if text == new_text {
        return;
    }
    let len = end - start;
    let txn = Transaction::replace(start, len, &new_text);
    let inv = txn.invert(&editor.document.text);
    if editor.document.apply_transaction(&txn).is_ok() {
        editor.history.commit(txn, inv);
    }
}

pub(crate) fn apply_indent(
    editor: &mut Editor,
    start: usize,
    end: usize,
    indent: bool,
) {
    let start_line = editor.document.text.char_to_line(start);
    let end_line =
        editor.document.text.char_to_line(end.saturating_sub(1).max(start));

    // Build replacement for the entire range
    // Process lines from last to first to keep offsets
    // valid
    for line in (start_line..=end_line).rev() {
        let line_start = editor.document.text.line_to_char(line);
        if indent {
            let txn = Transaction::insert(line_start, "    ");
            let inv = txn.invert(&editor.document.text);
            if editor.document.apply_transaction(&txn).is_ok() {
                editor.history.commit(txn, inv);
            }
        } else {
            // Remove up to 4 leading spaces
            let line_text: String = editor
                .document
                .line(line)
                .map(|l| l.chars().collect())
                .unwrap_or_default();
            let spaces =
                line_text.chars().take(4).take_while(|c| *c == ' ').count();
            if spaces > 0 {
                let txn = Transaction::delete(line_start, spaces);
                let inv = txn.invert(&editor.document.text);
                if editor.document.apply_transaction(&txn).is_ok() {
                    editor.history.commit(txn, inv);
                }
            }
        }
    }
    // Move cursor to first non-blank after indent
    let col = editor.first_non_blank_col(editor.view.cursor_line);
    editor.view.set_col(col);
    editor.clamp_cursor_col();
}

// ── Insert variant execution ──────────────────────

pub(crate) fn execute_insert(editor: &mut Editor, variant: InsertVariant) {
    match variant {
        InsertVariant::Before => editor.enter_insert(),
        InsertVariant::After => {
            editor.enter_insert_after();
        }
        InsertVariant::LineEnd => {
            editor.enter_insert_eol();
        }
        InsertVariant::LineStart => {
            editor.enter_insert_bol();
        }
        InsertVariant::LineBelow => {
            open_line_below(editor);
        }
        InsertVariant::LineAbove => {
            open_line_above(editor);
        }
    }
}

// ── Special command execution ─────────────────────

#[allow(clippy::too_many_lines)]
pub(crate) fn execute_special(
    editor: &mut Editor,
    cmd: SpecialCommand,
    count: usize,
) {
    match cmd {
        SpecialCommand::DeleteChar => {
            delete_char_at_cursor(editor, count);
        }
        SpecialCommand::DeleteCharBefore => {
            for _ in 0..count {
                if editor.view.cursor_col > 0 {
                    let pos = cursor_pos(editor);
                    let txn = Transaction::delete(pos - 1, 1);
                    let inv = txn.invert(&editor.document.text);
                    if editor.document.apply_transaction(&txn).is_ok() {
                        editor.history.commit(txn, inv);
                        editor.view.cursor_col -= 1;
                        editor.view.desired_col = editor.view.cursor_col;
                    }
                }
            }
        }
        SpecialCommand::Substitute => {
            // s = cl (delete char, enter insert)
            delete_char_at_cursor(editor, count);
            editor.mode = Mode::Insert;
        }
        SpecialCommand::SubstituteLine => {
            // S = cc (delete line content, enter insert)
            let line = editor.view.cursor_line;
            let fnb = editor.first_non_blank_col(line);
            let len = editor.current_line_len();
            if len > fnb {
                let start = editor.document.line_col_to_char(line, fnb);
                let end = editor.document.line_col_to_char(line, len);
                let text: String =
                    editor.document.text.slice(start..end).chars().collect();
                store_yank(editor, text, true);
                let txn = Transaction::delete(start, end - start);
                let inv = txn.invert(&editor.document.text);
                if editor.document.apply_transaction(&txn).is_ok() {
                    editor.history.commit(txn, inv);
                }
            }
            editor.view.set_col(fnb);
            editor.mode = Mode::Insert;
        }
        SpecialCommand::ReplaceChar(c) => {
            let pos = cursor_pos(editor);
            let len = editor.document.text.len_chars();
            if pos < len {
                let mut s = String::new();
                s.push(c);
                let txn = Transaction::replace(pos, 1, &s);
                let inv = txn.invert(&editor.document.text);
                if editor.document.apply_transaction(&txn).is_ok() {
                    editor.history.commit(txn, inv);
                }
            }
        }
        SpecialCommand::JoinLines => {
            for _ in 0..count {
                join_line(editor);
            }
        }
        SpecialCommand::ToggleCaseChar => {
            let pos = cursor_pos(editor);
            let len = editor.document.text.len_chars();
            for i in 0..count {
                let p = pos + i;
                if p >= len {
                    break;
                }
                apply_case(editor, p, p + 1, CaseOp::Toggle);
            }
            // Move cursor forward
            let new_pos =
                (pos + count).min(editor.current_line_len().saturating_sub(1));
            editor.view.set_col(new_pos);
        }
        SpecialCommand::ChangeToEnd => {
            // C = c$ (delete to end, enter insert)
            let line = editor.view.cursor_line;
            let col = editor.view.cursor_col;
            let len = editor.current_line_len();
            if col < len {
                let start = cursor_pos(editor);
                let end = editor.document.line_col_to_char(line, len);
                let text: String =
                    editor.document.text.slice(start..end).chars().collect();
                store_yank(editor, text, true);
                let txn = Transaction::delete(start, end - start);
                let inv = txn.invert(&editor.document.text);
                if editor.document.apply_transaction(&txn).is_ok() {
                    editor.history.commit(txn, inv);
                }
            }
            editor.mode = Mode::Insert;
        }
        SpecialCommand::DeleteToEnd => {
            // D = d$
            let line = editor.view.cursor_line;
            let col = editor.view.cursor_col;
            let len = editor.current_line_len();
            if col < len {
                let start = cursor_pos(editor);
                let end = editor.document.line_col_to_char(line, len);
                let text: String =
                    editor.document.text.slice(start..end).chars().collect();
                store_yank(editor, text, true);
                let txn = Transaction::delete(start, end - start);
                let inv = txn.invert(&editor.document.text);
                if editor.document.apply_transaction(&txn).is_ok() {
                    editor.history.commit(txn, inv);
                    editor.clamp_cursor_col();
                }
            }
        }
        SpecialCommand::YankLine => {
            let (start, end) = line_range(editor, count);
            let mut text: String =
                editor.document.text.slice(start..end).chars().collect();
            if !text.ends_with('\n') {
                text.push('\n');
            }
            store_yank(editor, text, false);
        }
        SpecialCommand::IndentLine => {
            let (start, end) = line_range(editor, count);
            apply_indent(editor, start, end, true);
        }
        SpecialCommand::DedentLine => {
            let (start, end) = line_range(editor, count);
            apply_indent(editor, start, end, false);
        }
        SpecialCommand::Paste | SpecialCommand::PasteBefore
            if editor.mode.is_visual() =>
        {
            paste_over_selection(editor);
        }
        SpecialCommand::Paste => {
            paste(editor, false, count);
        }
        SpecialCommand::PasteBefore => {
            paste(editor, true, count);
        }
        SpecialCommand::Undo => {
            for _ in 0..count {
                undo(editor);
            }
        }
        SpecialCommand::Redo => {
            for _ in 0..count {
                redo(editor);
            }
        }
        SpecialCommand::SearchNext => {
            search_next(editor, false, count);
        }
        SpecialCommand::SearchPrev => {
            search_next(editor, true, count);
        }
        SpecialCommand::SearchWordForward => {
            search_word(editor, false, count);
        }
        SpecialCommand::SearchWordBackward => {
            search_word(editor, true, count);
        }
        SpecialCommand::SetMark(c) => {
            let pos = cursor_pos(editor);
            editor.marks.insert(c, pos);
        }
        SpecialCommand::DotRepeat => {
            // TODO: dot repeat (needs last-action
            // recording)
        }
    }
}

// ── Register routing ──────────────────────────────

/// Route yanked/deleted text to the right registers:
/// the selected one (named, `+` clipboard, `_`
/// discard), plus vim's implicit ones — `"` always,
/// `0` for yanks, `1`-`9` shift for line deletes,
/// `-` for small deletes.
pub(crate) fn store_yank(editor: &mut Editor, text: String, is_delete: bool) {
    let reg = editor.yank_register;
    match reg {
        '_' => {}
        '+' => {
            clipboard_set(&text);
            editor.registers.write('"', text);
        }
        '"' => {
            if is_delete {
                if text.contains('\n') {
                    shift_numbered_registers(editor);
                    editor.registers.write('1', text.clone());
                } else {
                    editor.registers.write('-', text.clone());
                }
            } else {
                editor.registers.write('0', text.clone());
            }
            editor.registers.write('"', text);
        }
        c if c.is_ascii_uppercase() => {
            editor.registers.push(c, text.clone());
            editor.registers.write('"', text);
        }
        c => {
            editor.registers.write(c, text.clone());
            editor.registers.write('"', text);
        }
    }
}

fn shift_numbered_registers(editor: &mut Editor) {
    for i in (1..=8u32).rev() {
        let from = char::from_digit(i, 10).unwrap_or('1');
        let to = char::from_digit(i + 1, 10).unwrap_or('9');
        if let Some(v) = editor.registers.joined(from) {
            editor.registers.write(to, v);
        }
    }
}

/// Text a paste should insert from a register
/// (joined for `"A` appends; `+` reads the system
/// clipboard).
pub(crate) fn register_text(editor: &Editor, reg: char) -> Option<String> {
    if reg == '+' {
        clipboard_get()
    } else {
        editor.registers.joined(reg)
    }
}

fn clipboard_set(text: &str) {
    if let Ok(mut cb) = arboard::Clipboard::new() {
        drop(cb.set_text(text.to_owned()));
    }
}

fn clipboard_get() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

// ── Paste ─────────────────────────────────────────

pub(crate) fn paste(editor: &mut Editor, before: bool, count: usize) {
    let reg = editor.yank_register;
    let Some(text) = register_text(editor, reg) else {
        return;
    };

    let is_linewise = text.ends_with('\n');
    let paste_text = text.repeat(count);

    if is_linewise {
        let line = editor.view.cursor_line;
        let pos = if before {
            editor.document.line_col_to_char(line, 0)
        } else {
            let next = line + 1;
            if next < editor.document.text.len_lines() {
                editor.document.text.line_to_char(next)
            } else {
                // At last line, need to insert newline
                // first
                let len = editor.document.text.len_chars();
                let has_trailing_nl =
                    len > 0 && editor.document.text.char(len - 1) == '\n';
                if has_trailing_nl {
                    len
                } else {
                    // Insert a newline at the end
                    let nl_txn = Transaction::insert(len, "\n");
                    let nl_inv = nl_txn.invert(&editor.document.text);
                    if editor.document.apply_transaction(&nl_txn).is_ok() {
                        editor.history.commit(nl_txn, nl_inv);
                    }
                    editor.document.text.len_chars()
                }
            }
        };
        let txn = Transaction::insert(pos, &paste_text);
        let inv = txn.invert(&editor.document.text);
        if editor.document.apply_transaction(&txn).is_ok() {
            editor.history.commit(txn, inv);
            // Move cursor to first non-blank of first
            // pasted line
            let (pline, _) = editor.document.char_to_line_col(pos);
            editor.view.cursor_line = pline;
            let fnb = editor.first_non_blank_col(pline);
            editor.view.set_col(fnb);
            editor.view.ensure_cursor_visible();
        }
    } else {
        let pos = if before {
            cursor_pos(editor)
        } else {
            let p = cursor_pos(editor);
            (p + 1).min(editor.document.text.len_chars())
        };
        let txn = Transaction::insert(pos, &paste_text);
        let inv = txn.invert(&editor.document.text);
        if editor.document.apply_transaction(&txn).is_ok() {
            editor.history.commit(txn, inv);
            // Cursor at end of pasted text - 1
            let new_pos = pos + paste_text.chars().count() - 1;
            set_cursor_from_pos(editor, new_pos);
        }
    }
}

// ── Undo/Redo ─────────────────────────────────────

pub(crate) fn undo(editor: &mut Editor) {
    let txn = editor.history.undo().cloned();
    if let Some(txn) = txn {
        if editor.document.apply_transaction(&txn).is_ok() {
            if let Some((line, col)) = txn.cursor_after {
                editor.view.cursor_line = line;
                editor.view.set_col(col);
            }
            editor.clamp_cursor_col();
            editor.view.ensure_cursor_visible();
            // Check if document is back to unmodified
            if !editor.history.can_undo() {
                editor.document.modified = false;
            }
        }
    }
}

pub(crate) fn redo(editor: &mut Editor) {
    let txn = editor.history.redo().cloned();
    if let Some(txn) = txn {
        if editor.document.apply_transaction(&txn).is_ok() {
            if let Some((line, col)) = txn.cursor_after {
                editor.view.cursor_line = line;
                editor.view.set_col(col);
            }
            editor.clamp_cursor_col();
            editor.view.ensure_cursor_visible();
        }
    }
}

// ── Join line ─────────────────────────────────────

pub(crate) fn join_line(editor: &mut Editor) {
    let line = editor.view.cursor_line;
    if line >= editor.max_line() {
        return;
    }
    // Replace newline (and leading whitespace of next
    // line) with a single space
    let eol =
        editor.document.line_col_to_char(line, editor.document.line_len(line));
    let next_fnb = editor.first_non_blank_col(line + 1);
    let next_start = editor.document.line_col_to_char(line + 1, 0);
    let replace_end = next_start + next_fnb;
    let len = replace_end - eol;
    let txn = Transaction::replace(eol, len, " ");
    let inv = txn.invert(&editor.document.text);
    if editor.document.apply_transaction(&txn).is_ok() {
        editor.history.commit(txn, inv);
        // Cursor at the join point
        set_cursor_from_pos(editor, eol);
    }
}

// ── Text mutation helpers ─────────────────────────

pub(crate) fn insert_char(editor: &mut Editor, c: char) {
    let pos = cursor_pos(editor);
    let mut s = String::new();
    s.push(c);
    let txn = Transaction::insert(pos, &s);
    if editor.document.apply_transaction(&txn).is_ok() {
        editor.view.cursor_col += 1;
        editor.view.desired_col = editor.view.cursor_col;
    }
}

pub(crate) fn insert_newline(editor: &mut Editor) {
    let pos = cursor_pos(editor);
    let txn = Transaction::insert(pos, "\n");
    if editor.document.apply_transaction(&txn).is_ok() {
        editor.view.cursor_line += 1;
        editor.view.cursor_col = 0;
        editor.view.desired_col = 0;
        editor.view.ensure_cursor_visible();
    }
}

pub(crate) fn delete_char_before_cursor(editor: &mut Editor) {
    if editor.view.cursor_col == 0 && editor.view.cursor_line == 0 {
        return;
    }

    if editor.view.cursor_col == 0 {
        let prev_line = editor.view.cursor_line - 1;
        let prev_len = editor.document.line_len(prev_line);
        let pos = editor.document.line_col_to_char(editor.view.cursor_line, 0);
        let txn = Transaction::delete(pos - 1, 1);
        if editor.document.apply_transaction(&txn).is_ok() {
            editor.view.cursor_line = prev_line;
            editor.view.cursor_col = prev_len;
            editor.view.desired_col = prev_len;
            editor.view.ensure_cursor_visible();
        }
    } else {
        let pos = cursor_pos(editor);
        let txn = Transaction::delete(pos - 1, 1);
        if editor.document.apply_transaction(&txn).is_ok() {
            editor.view.cursor_col -= 1;
            editor.view.desired_col = editor.view.cursor_col;
        }
    }
}

pub(crate) fn delete_char_at_cursor(editor: &mut Editor, count: usize) {
    let line_len = editor.current_line_len();
    if line_len == 0 {
        return;
    }
    let pos = cursor_pos(editor);
    let del_count = count.min(line_len - editor.view.cursor_col);
    if del_count == 0 {
        return;
    }
    let text: String =
        editor.document.text.slice(pos..pos + del_count).chars().collect();
    store_yank(editor, text, true);
    let txn = Transaction::delete(pos, del_count);
    let inv = txn.invert(&editor.document.text);
    if editor.document.apply_transaction(&txn).is_ok() {
        editor.history.commit(txn, inv);
        editor.clamp_cursor_col();
    }
}

pub(crate) fn delete_word_back(editor: &mut Editor) {
    if editor.view.cursor_col == 0 {
        return;
    }
    let pos = cursor_pos(editor);
    let new_pos = movement::prev_word_start(&editor.document.text, pos, false);
    let line_start =
        editor.document.line_col_to_char(editor.view.cursor_line, 0);
    let clamped = new_pos.max(line_start);
    let del = pos - clamped;
    if del == 0 {
        return;
    }
    let txn = Transaction::delete(clamped, del);
    if editor.document.apply_transaction(&txn).is_ok() {
        let (_, col) = editor.document.char_to_line_col(clamped);
        editor.view.cursor_col = col;
        editor.view.desired_col = col;
    }
}

pub(crate) fn delete_to_line_start(editor: &mut Editor) {
    if editor.view.cursor_col == 0 {
        return;
    }
    let line = editor.view.cursor_line;
    let col = editor.view.cursor_col;
    let start = editor.document.line_col_to_char(line, 0);
    let end = editor.document.line_col_to_char(line, col);
    let txn = Transaction::delete(start, end - start);
    if editor.document.apply_transaction(&txn).is_ok() {
        editor.view.cursor_col = 0;
        editor.view.desired_col = 0;
    }
}

pub(crate) fn open_line_below(editor: &mut Editor) {
    let line = editor.view.cursor_line;
    let pos =
        editor.document.line_col_to_char(line, editor.document.line_len(line));
    let txn = Transaction::insert(pos, "\n");
    let inv = txn.invert(&editor.document.text);
    if editor.document.apply_transaction(&txn).is_ok() {
        editor.history.commit(txn, inv);
        editor.view.cursor_line = line + 1;
        editor.view.cursor_col = 0;
        editor.view.desired_col = 0;
        editor.mode = Mode::Insert;
        editor.view.ensure_cursor_visible();
    }
}

pub(crate) fn open_line_above(editor: &mut Editor) {
    let line = editor.view.cursor_line;
    let pos = editor.document.line_col_to_char(line, 0);
    let txn = Transaction::insert(pos, "\n");
    let inv = txn.invert(&editor.document.text);
    if editor.document.apply_transaction(&txn).is_ok() {
        editor.history.commit(txn, inv);
        editor.view.cursor_col = 0;
        editor.view.desired_col = 0;
        editor.mode = Mode::Insert;
        editor.view.ensure_cursor_visible();
    }
}
