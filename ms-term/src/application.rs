use std::io;
use std::time::Duration;

use crossterm::cursor::SetCursorStyle;
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyModifiers,
};
use crossterm::execute;

use ms_core::transaction::Transaction;
use ms_tui::style::{Color, Style};
use ms_tui::terminal::Terminal;
use ms_view::editor::Editor;
use ms_view::mode::Mode;

const GUTTER_WIDTH: u16 = 6;

/// Run the main event loop.
///
/// # Errors
/// Returns IO error on terminal or rendering failure.
pub(super) fn run(
    mut editor: Editor,
) -> io::Result<()> {
    let mut terminal = Terminal::stdout()?;
    terminal.setup()?;

    // Set initial height
    editor.view.height =
        terminal.area().height.saturating_sub(1);

    let result = event_loop(&mut editor, &mut terminal);

    terminal.teardown()?;
    result
}

fn event_loop(
    editor: &mut Editor,
    terminal: &mut Terminal,
) -> io::Result<()> {
    loop {
        render(editor, terminal)?;

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) => {
                    editor.status_message = None;
                    handle_key(editor, key)?;
                }
                Event::Resize(_, _) => {
                    terminal.resize()?;
                    editor.view.height =
                        terminal.area().height
                            .saturating_sub(1);
                }
                _ => {}
            }
        }

        if editor.should_quit {
            return Ok(());
        }
    }
}

fn handle_key(
    editor: &mut Editor,
    key: KeyEvent,
) -> io::Result<()> {
    match editor.mode {
        Mode::Normal => handle_normal(editor, key),
        Mode::Insert => handle_insert(editor, key),
        Mode::Command => handle_command(editor, key),
    }
}

fn handle_normal(
    editor: &mut Editor,
    key: KeyEvent,
) -> io::Result<()> {
    match key.code {
        // Motions
        KeyCode::Char('h') | KeyCode::Left => {
            editor.view.move_left();
        }
        KeyCode::Char('l') | KeyCode::Right => {
            let max = editor
                .current_line_len()
                .saturating_sub(1);
            editor.view.move_right(max);
        }
        KeyCode::Char('j') | KeyCode::Down => {
            let max = editor.max_line();
            let doc = &editor.document;
            editor.view.move_down(max, |line| {
                let len = doc.line_len(line);
                if len == 0 { 0 } else { len - 1 }
            });
        }
        KeyCode::Char('k') | KeyCode::Up => {
            let doc = &editor.document;
            editor.view.move_up(|line| {
                let len = doc.line_len(line);
                if len == 0 { 0 } else { len - 1 }
            });
        }
        KeyCode::Char('0') => {
            editor.view.move_to_line_start();
        }
        KeyCode::Char('$') => {
            let len = editor.current_line_len();
            editor.view.move_to_line_end(len);
        }
        KeyCode::Char('^') => {
            let col = editor
                .first_non_blank_col(
                    editor.view.cursor_line,
                );
            editor.view.move_to_first_non_blank(col);
        }
        KeyCode::Char('w') => {
            move_word_forward(editor, false);
        }
        KeyCode::Char('W') => {
            move_word_forward(editor, true);
        }
        KeyCode::Char('b') => {
            move_word_backward(editor, false);
        }
        KeyCode::Char('B') => {
            move_word_backward(editor, true);
        }
        KeyCode::Char('e') => {
            move_word_end(editor, false);
        }
        KeyCode::Char('E') => {
            move_word_end(editor, true);
        }
        KeyCode::Char('g') => {
            editor.view.cursor_line = 0;
            editor.view.set_col(0);
            editor.view.ensure_cursor_visible();
        }
        KeyCode::Char('G') => {
            editor.view.cursor_line = editor.max_line();
            editor.clamp_cursor_col();
            editor.view.desired_col =
                editor.view.cursor_col;
            editor.view.ensure_cursor_visible();
        }

        // Enter insert mode
        KeyCode::Char('i') => editor.enter_insert(),
        KeyCode::Char('a') => editor.enter_insert_after(),
        KeyCode::Char('A') => editor.enter_insert_eol(),
        KeyCode::Char('I') => editor.enter_insert_bol(),
        KeyCode::Char('o') => {
            open_line_below(editor);
        }
        KeyCode::Char('O') => {
            open_line_above(editor);
        }

        // Command mode
        KeyCode::Char(':') => editor.enter_command(),

        // Delete char under cursor (x)
        KeyCode::Char('x') => {
            delete_char_at_cursor(editor);
        }

        _ => {}
    }
    Ok(())
}

fn handle_insert(
    editor: &mut Editor,
    key: KeyEvent,
) -> io::Result<()> {
    match key.code {
        KeyCode::Esc => {
            editor.enter_normal();
        }
        KeyCode::Char(c) => {
            insert_char(editor, c);
        }
        KeyCode::Enter => {
            insert_newline(editor);
        }
        KeyCode::Backspace => {
            delete_char_before_cursor(editor);
        }
        KeyCode::Delete => {
            delete_char_at_cursor(editor);
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
            editor
                .view
                .move_down(max, |line| doc.line_len(line));
        }
        // Ctrl-w: delete word back
        _ if key.code == KeyCode::Char('w')
            && key
                .modifiers
                .contains(KeyModifiers::CONTROL) =>
        {
            delete_word_back(editor);
        }
        // Ctrl-u: delete to line start
        _ if key.code == KeyCode::Char('u')
            && key
                .modifiers
                .contains(KeyModifiers::CONTROL) =>
        {
            delete_to_line_start(editor);
        }
        _ => {}
    }
    Ok(())
}

fn handle_command(
    editor: &mut Editor,
    key: KeyEvent,
) -> io::Result<()> {
    match key.code {
        KeyCode::Esc => {
            editor.enter_normal();
        }
        KeyCode::Enter => {
            let cmd = editor.command_buffer.clone();
            editor.enter_normal();
            execute_command(editor, &cmd)?;
        }
        KeyCode::Backspace => {
            if editor.command_buffer.is_empty() {
                editor.enter_normal();
            } else {
                editor.command_buffer.pop();
            }
        }
        KeyCode::Char(c) => {
            editor.command_buffer.push(c);
        }
        _ => {}
    }
    Ok(())
}

fn execute_command(
    editor: &mut Editor,
    cmd: &str,
) -> io::Result<()> {
    let cmd = cmd.trim();
    match cmd {
        "q" => {
            if editor.document.modified {
                editor.status_message = Some(
                    "No write since last change \
                     (add ! to override)"
                        .to_owned(),
                );
            } else {
                editor.should_quit = true;
            }
        }
        "q!" => {
            editor.should_quit = true;
        }
        "w" => match editor.document.save() {
            Ok(()) => {
                let name = editor
                    .document
                    .path
                    .as_ref()
                    .map_or(
                        "[scratch]".to_owned(),
                        |p| p.display().to_string(),
                    );
                editor.status_message =
                    Some(format!("\"{name}\" written"));
            }
            Err(e) => {
                editor.status_message =
                    Some(format!("Error: {e}"));
            }
        },
        "wq" | "x" => {
            if let Err(e) = editor.document.save() {
                editor.status_message =
                    Some(format!("Error: {e}"));
            } else {
                editor.should_quit = true;
            }
        }
        _ => {
            editor.status_message = Some(format!(
                "Not an editor command: {cmd}"
            ));
        }
    }
    Ok(())
}

// --- Text mutation helpers ---

fn insert_char(editor: &mut Editor, c: char) {
    let pos = editor.document.line_col_to_char(
        editor.view.cursor_line,
        editor.view.cursor_col,
    );
    let mut s = String::new();
    s.push(c);
    let txn = Transaction::insert(pos, &s);
    if editor.document.apply_transaction(&txn).is_ok() {
        editor.view.cursor_col += 1;
        editor.view.desired_col = editor.view.cursor_col;
    }
}

fn insert_newline(editor: &mut Editor) {
    let pos = editor.document.line_col_to_char(
        editor.view.cursor_line,
        editor.view.cursor_col,
    );
    let txn = Transaction::insert(pos, "\n");
    if editor.document.apply_transaction(&txn).is_ok() {
        editor.view.cursor_line += 1;
        editor.view.cursor_col = 0;
        editor.view.desired_col = 0;
        editor.view.ensure_cursor_visible();
    }
}

fn delete_char_before_cursor(editor: &mut Editor) {
    if editor.view.cursor_col == 0
        && editor.view.cursor_line == 0
    {
        return;
    }

    if editor.view.cursor_col == 0 {
        // Join with previous line
        let prev_line = editor.view.cursor_line - 1;
        let prev_len = editor.document.line_len(prev_line);
        let pos = editor.document.line_col_to_char(
            editor.view.cursor_line,
            0,
        );
        // Delete the newline before this line
        let txn = Transaction::delete(pos - 1, 1);
        if editor
            .document
            .apply_transaction(&txn)
            .is_ok()
        {
            editor.view.cursor_line = prev_line;
            editor.view.cursor_col = prev_len;
            editor.view.desired_col = prev_len;
            editor.view.ensure_cursor_visible();
        }
    } else {
        let pos = editor.document.line_col_to_char(
            editor.view.cursor_line,
            editor.view.cursor_col,
        );
        let txn = Transaction::delete(pos - 1, 1);
        if editor
            .document
            .apply_transaction(&txn)
            .is_ok()
        {
            editor.view.cursor_col -= 1;
            editor.view.desired_col =
                editor.view.cursor_col;
        }
    }
}

fn delete_char_at_cursor(editor: &mut Editor) {
    let line_len = editor.current_line_len();
    if line_len == 0 {
        return;
    }
    let pos = editor.document.line_col_to_char(
        editor.view.cursor_line,
        editor.view.cursor_col,
    );
    let txn = Transaction::delete(pos, 1);
    if editor.document.apply_transaction(&txn).is_ok() {
        editor.clamp_cursor_col();
    }
}

fn delete_word_back(editor: &mut Editor) {
    if editor.view.cursor_col == 0 {
        return;
    }
    let line = editor.view.cursor_line;
    let col = editor.view.cursor_col;
    let new_col = find_word_start_back(editor, line, col);
    let start = editor
        .document
        .line_col_to_char(line, new_col);
    let end =
        editor.document.line_col_to_char(line, col);
    let txn = Transaction::delete(start, end - start);
    if editor.document.apply_transaction(&txn).is_ok() {
        editor.view.cursor_col = new_col;
        editor.view.desired_col = new_col;
    }
}

fn delete_to_line_start(editor: &mut Editor) {
    if editor.view.cursor_col == 0 {
        return;
    }
    let line = editor.view.cursor_line;
    let col = editor.view.cursor_col;
    let start =
        editor.document.line_col_to_char(line, 0);
    let end =
        editor.document.line_col_to_char(line, col);
    let txn = Transaction::delete(start, end - start);
    if editor.document.apply_transaction(&txn).is_ok() {
        editor.view.cursor_col = 0;
        editor.view.desired_col = 0;
    }
}

fn open_line_below(editor: &mut Editor) {
    let line = editor.view.cursor_line;
    let line_end = editor
        .document
        .line_col_to_char(line, editor.document.line_len(line));
    // Insert newline at end of current line (before the
    // existing newline if present)
    let pos = if editor.document.line(line).is_some_and(
        |l| {
            l.chars().last() == Some('\n')
        },
    ) {
        line_end
    } else {
        line_end
    };
    let txn = Transaction::insert(pos, "\n");
    if editor.document.apply_transaction(&txn).is_ok() {
        editor.view.cursor_line = line + 1;
        editor.view.cursor_col = 0;
        editor.view.desired_col = 0;
        editor.mode = Mode::Insert;
        editor.view.ensure_cursor_visible();
    }
}

fn open_line_above(editor: &mut Editor) {
    let line = editor.view.cursor_line;
    let pos =
        editor.document.line_col_to_char(line, 0);
    let txn = Transaction::insert(pos, "\n");
    if editor.document.apply_transaction(&txn).is_ok() {
        // Cursor stays at the same line (now the new
        // empty line)
        editor.view.cursor_col = 0;
        editor.view.desired_col = 0;
        editor.mode = Mode::Insert;
        editor.view.ensure_cursor_visible();
    }
}

// --- Word motion helpers ---

fn move_word_forward(editor: &mut Editor, big: bool) {
    let text = &editor.document.text;
    let pos = editor.document.line_col_to_char(
        editor.view.cursor_line,
        editor.view.cursor_col,
    );
    let new_pos = next_word_start(text, pos, big);
    let (line, col) =
        editor.document.char_to_line_col(new_pos);
    editor.view.cursor_line = line;
    editor.view.set_col(col);
    editor.view.ensure_cursor_visible();
}

fn move_word_backward(editor: &mut Editor, big: bool) {
    let text = &editor.document.text;
    let pos = editor.document.line_col_to_char(
        editor.view.cursor_line,
        editor.view.cursor_col,
    );
    let new_pos = prev_word_start(text, pos, big);
    let (line, col) =
        editor.document.char_to_line_col(new_pos);
    editor.view.cursor_line = line;
    editor.view.set_col(col);
    editor.view.ensure_cursor_visible();
}

fn move_word_end(editor: &mut Editor, big: bool) {
    let text = &editor.document.text;
    let pos = editor.document.line_col_to_char(
        editor.view.cursor_line,
        editor.view.cursor_col,
    );
    let new_pos = next_word_end(text, pos, big);
    let (line, col) =
        editor.document.char_to_line_col(new_pos);
    editor.view.cursor_line = line;
    editor.view.set_col(col);
    editor.view.ensure_cursor_visible();
}

/// Find the next word start (vim `w`/`W`).
fn next_word_start(
    text: &ropey::Rope,
    pos: usize,
    big: bool,
) -> usize {
    let len = text.len_chars();
    if pos >= len {
        return pos;
    }
    let mut i = pos;
    let ch = text.char(i);
    let cat = char_category(ch, big);

    // Skip current word
    while i < len && char_category(text.char(i), big) == cat
    {
        i += 1;
    }
    // Skip whitespace
    while i < len && text.char(i).is_whitespace() {
        i += 1;
    }
    i.min(len.saturating_sub(1))
}

/// Find the previous word start (vim `b`/`B`).
fn prev_word_start(
    text: &ropey::Rope,
    pos: usize,
    big: bool,
) -> usize {
    if pos == 0 {
        return 0;
    }
    let mut i = pos - 1;

    // Skip whitespace backward
    while i > 0 && text.char(i).is_whitespace() {
        i -= 1;
    }

    let cat = char_category(text.char(i), big);

    // Skip same-category chars backward
    while i > 0
        && char_category(text.char(i - 1), big) == cat
    {
        i -= 1;
    }
    i
}

/// Find the next word end (vim `e`/`E`).
fn next_word_end(
    text: &ropey::Rope,
    pos: usize,
    big: bool,
) -> usize {
    let len = text.len_chars();
    if pos + 1 >= len {
        return pos;
    }
    let mut i = pos + 1;

    // Skip whitespace
    while i < len && text.char(i).is_whitespace() {
        i += 1;
    }

    if i >= len {
        return len.saturating_sub(1);
    }

    let cat = char_category(text.char(i), big);

    // Skip same-category chars
    while i + 1 < len
        && char_category(text.char(i + 1), big) == cat
    {
        i += 1;
    }
    i.min(len.saturating_sub(1))
}

fn find_word_start_back(
    editor: &Editor,
    line: usize,
    col: usize,
) -> usize {
    let text = &editor.document.text;
    let pos = editor.document.line_col_to_char(line, col);
    let new_pos = prev_word_start(text, pos, false);
    let line_start =
        editor.document.line_col_to_char(line, 0);
    // Don't go before the start of the current line
    new_pos.saturating_sub(line_start)
}

#[derive(PartialEq, Eq)]
enum CharCat {
    Word,
    Punct,
    Whitespace,
}

fn char_category(c: char, big: bool) -> CharCat {
    if c.is_whitespace() {
        CharCat::Whitespace
    } else if big {
        // W/B/E: everything non-whitespace is one
        // category
        CharCat::Word
    } else if c.is_alphanumeric() || c == '_' {
        CharCat::Word
    } else {
        CharCat::Punct
    }
}

// --- Rendering ---

fn render(
    editor: &Editor,
    terminal: &mut Terminal,
) -> io::Result<()> {
    let area = terminal.area();
    terminal.buffer.clear();

    let line_num_style = Style::default()
        .fg(Color::Rgb(0x6B, 0x6B, 0x6B));
    let text_style = Style::default()
        .fg(Color::Rgb(0xD4, 0xD4, 0xD4));
    let cursor_line_num_style = Style::default()
        .fg(Color::Rgb(0xD4, 0xD4, 0xD4));
    let status_style = Style::default()
        .fg(Color::Rgb(0x00, 0x00, 0x00))
        .bg(Color::Rgb(0xD4, 0xD4, 0xD4));

    // Reserve bottom line for status/command
    let text_height = area.height.saturating_sub(1);

    for row in 0..text_height {
        let doc_line =
            editor.view.scroll_offset + row as usize;

        if let Some(line) =
            editor.document.line(doc_line)
        {
            let num_str =
                format!("{:>4} ", doc_line + 1);
            let ln_style =
                if doc_line == editor.view.cursor_line {
                    cursor_line_num_style
                } else {
                    line_num_style
                };
            terminal.buffer.put_str(
                0, row, &num_str, ln_style,
            );

            let line_text: String = line
                .chars()
                .take_while(|c| *c != '\n')
                .collect();
            let max_cols =
                area.width.saturating_sub(GUTTER_WIDTH);
            let truncated: String = line_text
                .chars()
                .take(max_cols as usize)
                .collect();
            terminal.buffer.put_str(
                GUTTER_WIDTH,
                row,
                &truncated,
                text_style,
            );
        } else {
            terminal.buffer.put_str(
                0,
                row,
                "~",
                line_num_style,
            );
        }
    }

    // Status bar / command line
    let status_row = area.height - 1;
    let status_text = match editor.mode {
        Mode::Command => {
            format!(":{}", editor.command_buffer)
        }
        _ => {
            if let Some(ref msg) = editor.status_message {
                msg.clone()
            } else {
                let file_name = editor
                    .document
                    .path
                    .as_ref()
                    .map_or("[scratch]", |p| {
                        p.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("[scratch]")
                    });
                let modified_indicator = if editor
                    .document
                    .modified
                {
                    "[+]"
                } else {
                    ""
                };
                let mode_name =
                    editor.mode.display_name();
                let pos = format!(
                    "{}:{} ",
                    editor.view.cursor_line + 1,
                    editor.view.cursor_col + 1,
                );
                format!(
                    " -- {mode_name} -- \
                     {file_name}{modified_indicator}\
                     {:>width$}",
                    pos,
                    width = (area.width as usize)
                        .saturating_sub(
                            mode_name.len()
                                + file_name.len()
                                + modified_indicator.len()
                                + 8
                        ),
                )
            }
        }
    };
    terminal.buffer.put_str(
        0,
        status_row,
        &status_text,
        status_style,
    );

    terminal.flush()?;

    // Cursor position and shape
    let cursor_x =
        GUTTER_WIDTH + editor.view.cursor_col as u16;
    let cursor_y = editor.view.cursor_screen_row();

    let cursor_style = match editor.mode {
        Mode::Normal => SetCursorStyle::SteadyBlock,
        Mode::Insert => SetCursorStyle::SteadyBar,
        Mode::Command => SetCursorStyle::SteadyBar,
    };
    execute!(io::stdout(), cursor_style)?;
    terminal.set_cursor(cursor_x, cursor_y)?;

    Ok(())
}