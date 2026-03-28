use std::io;

use crossterm::cursor::SetCursorStyle;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use futures_core::Stream;

use ms_core::movement;
use ms_core::transaction::Transaction;
use ms_tui::style::Style;
use ms_tui::terminal::Terminal;
use ms_view::command::{
    Action, InsertVariant, KeyCode as VKeyCode, KeyInput, Motion, MotionType,
    Operator, SpecialCommand,
};
use ms_view::editor::Editor;
use ms_view::mode::Mode;

const GUTTER_WIDTH: u16 = 6;

#[derive(Debug)]
pub struct Application {
    editor: Editor,
    terminal: Terminal,
}

impl Application {
    /// # Errors
    /// Returns IO error if terminal cannot be created.
    pub fn new(editor: Editor) -> io::Result<Self> {
        let terminal = Terminal::stdout()?;
        Ok(Self { editor, terminal })
    }

    /// Create an event stream from the terminal
    /// backend. Wraps crossterm's `EventStream`.
    #[allow(clippy::unused_self)]
    pub fn event_stream(&self) -> EventStream {
        EventStream::new()
    }

    /// Run the editor: claim terminal, drive event
    /// loop, then restore.
    ///
    /// # Errors
    /// Returns IO error on terminal or rendering
    /// failure.
    pub async fn run<S>(&mut self, input_stream: &mut S) -> io::Result<()>
    where
        S: Stream<Item = io::Result<Event>> + Unpin,
    {
        self.terminal.setup()?;
        self.editor.view.height =
            self.terminal.area().height.saturating_sub(1);

        let result = self.event_loop(input_stream).await;

        self.terminal.teardown()?;
        result
    }

    async fn event_loop<S>(&mut self, input_stream: &mut S) -> io::Result<()>
    where
        S: Stream<Item = io::Result<Event>> + Unpin,
    {
        use tokio_stream::StreamExt;

        self.render()?;

        loop {
            if self.editor.should_quit {
                return Ok(());
            }

            let Some(event_result) = input_stream.next().await else {
                return Ok(());
            };

            match event_result? {
                Event::Key(key) => {
                    self.editor.status_message = None;
                    handle_key(&mut self.editor, key);
                }
                Event::Resize(_, _) => {
                    self.terminal.resize()?;
                    self.editor.view.height =
                        self.terminal.area().height.saturating_sub(1);
                }
                _ => {}
            }

            self.render()?;
        }
    }

    fn render(&mut self) -> io::Result<()> {
        let area = self.terminal.area();
        self.terminal.buffer.clear();

        let theme = &self.editor.theme;
        let line_num_style = theme.resolve("ui.linenr");
        let text_style = theme.resolve("ui.text");
        let cursor_ln_style = theme.resolve("ui.linenr.selected");
        let status_style = theme.resolve("ui.statusline");

        let text_height = area.height.saturating_sub(1);

        self.render_lines(
            text_height,
            area,
            line_num_style,
            cursor_ln_style,
            text_style,
        );

        self.render_status_bar(area, status_style);

        self.terminal.flush()?;

        let cursor_x = GUTTER_WIDTH + self.editor.view.cursor_col as u16;
        let cursor_y = self.editor.view.cursor_screen_row();

        let cursor_style = match self.editor.mode {
            Mode::Normal => SetCursorStyle::SteadyBlock,
            Mode::Insert | Mode::Command => SetCursorStyle::SteadyBar,
        };
        execute!(io::stdout(), cursor_style)?;
        self.terminal.set_cursor(cursor_x, cursor_y)?;

        Ok(())
    }

    fn render_lines(
        &mut self,
        text_height: u16,
        area: ms_tui::buffer::Rect,
        line_num_style: Style,
        cursor_ln_style: Style,
        text_style: Style,
    ) {
        for row in 0..text_height {
            let doc_line = self.editor.view.scroll_offset + row as usize;

            if let Some(line) = self.editor.document.line(doc_line) {
                let num_str = format!("{:>4} ", doc_line + 1);
                let ln_style = if doc_line == self.editor.view.cursor_line {
                    cursor_ln_style
                } else {
                    line_num_style
                };
                self.terminal.buffer.put_str(0, row, &num_str, ln_style);

                let line_text: String =
                    line.chars().take_while(|c| *c != '\n').collect();
                let max_cols = area.width.saturating_sub(GUTTER_WIDTH);
                let truncated: String =
                    line_text.chars().take(max_cols as usize).collect();
                self.terminal.buffer.put_str(
                    GUTTER_WIDTH,
                    row,
                    &truncated,
                    text_style,
                );
            } else {
                self.terminal.buffer.put_str(0, row, "~", line_num_style);
            }
        }
    }

    fn render_status_bar(
        &mut self,
        area: ms_tui::buffer::Rect,
        status_style: Style,
    ) {
        let status_row = area.height - 1;
        let status_text = match self.editor.mode {
            Mode::Command => {
                format!(":{}", self.editor.command_buffer)
            }
            _ => build_status_line(&self.editor, area),
        };
        self.terminal.buffer.put_str(
            0,
            status_row,
            &status_text,
            status_style,
        );
    }
}

fn build_status_line(editor: &Editor, area: ms_tui::buffer::Rect) -> String {
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
fn to_key_input(key: KeyEvent) -> KeyInput {
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

pub fn handle_key(editor: &mut Editor, key: KeyEvent) {
    match editor.mode {
        Mode::Normal => handle_normal(editor, key),
        Mode::Insert => handle_insert(editor, key),
        Mode::Command => handle_command(editor, key),
    }
}

fn handle_normal(editor: &mut Editor, key: KeyEvent) {
    let input = to_key_input(key);
    let action = editor.vim.feed(input);
    execute_action(editor, action);
}

fn handle_insert(editor: &mut Editor, key: KeyEvent) {
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

fn handle_command(editor: &mut Editor, key: KeyEvent) {
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
        KeyCode::Char(c) => {
            editor.command_buffer.push(c);
        }
        _ => {}
    }
}

fn execute_command(editor: &mut Editor, cmd: &str) {
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
                let name = editor.document.path.as_ref().map_or_else(
                    || "[scratch]".to_owned(),
                    |p| p.display().to_string(),
                );
                editor.status_message = Some(format!("\"{name}\" written"));
            }
            Err(e) => {
                editor.status_message = Some(format!("Error: {e}"));
            }
        },
        "wq" | "x" => {
            if let Err(e) = editor.document.save() {
                editor.status_message = Some(format!("Error: {e}"));
            } else {
                editor.should_quit = true;
            }
        }
        _ => {
            editor.status_message =
                Some(format!("Not an editor command: {cmd}"));
        }
    }
}

// ── Action dispatch ───────────────────────────────

#[allow(clippy::needless_pass_by_value)]
fn execute_action(editor: &mut Editor, action: Action) {
    match action {
        Action::Move(motion, count) => {
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
            editor.enter_command();
        }
        Action::Special(cmd, count) => {
            execute_special(editor, cmd, count);
        }
        Action::None => {}
    }
}

// ── Motion execution ──────────────────────────────

fn cursor_pos(editor: &Editor) -> usize {
    editor
        .document
        .line_col_to_char(editor.view.cursor_line, editor.view.cursor_col)
}

fn set_cursor_from_pos(editor: &mut Editor, pos: usize) {
    let (line, col) = editor.document.char_to_line_col(pos);
    editor.view.cursor_line = line;
    editor.view.set_col(col);
    editor.view.ensure_cursor_visible();
}

#[allow(clippy::too_many_lines)]
fn resolve_motion(editor: &Editor, motion: Motion, count: usize) -> usize {
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

fn normal_max_col(editor: &Editor, line: usize) -> usize {
    let len = editor.document.line_len(line);
    if len == 0 {
        0
    } else {
        len - 1
    }
}

fn execute_motion(editor: &mut Editor, motion: Motion, count: usize) {
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

fn motion_range(
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

fn line_range(editor: &Editor, count: usize) -> (usize, usize) {
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

fn apply_operator(
    editor: &mut Editor,
    op: Operator,
    start: usize,
    end: usize,
    mt: MotionType,
) {
    if start >= end {
        return;
    }
    let text: String =
        editor.document.text.slice(start..end).chars().collect();

    match op {
        Operator::Delete => {
            let reg = editor.yank_register;
            editor.registers.write(reg, text);
            apply_delete(editor, start, end, mt);
        }
        Operator::Change => {
            let reg = editor.yank_register;
            editor.registers.write(reg, text);
            apply_delete(editor, start, end, mt);
            editor.mode = Mode::Insert;
        }
        Operator::Yank => {
            let reg = editor.yank_register;
            editor.registers.write(reg, text);
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

fn apply_delete(
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

fn change_lines(editor: &mut Editor, count: usize) {
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
            let reg = editor.yank_register;
            editor.registers.write(reg, text);
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
        let reg = editor.yank_register;
        editor.registers.write(reg, text);

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
enum CaseOp {
    Lower,
    Upper,
    Toggle,
}

fn apply_case(editor: &mut Editor, start: usize, end: usize, op: CaseOp) {
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

fn apply_indent(editor: &mut Editor, start: usize, end: usize, indent: bool) {
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

fn execute_insert(editor: &mut Editor, variant: InsertVariant) {
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
fn execute_special(editor: &mut Editor, cmd: SpecialCommand, count: usize) {
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
                let reg = editor.yank_register;
                editor.registers.write(reg, text);
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
                let reg = editor.yank_register;
                editor.registers.write(reg, text);
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
                let reg = editor.yank_register;
                editor.registers.write(reg, text);
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
            let text: String =
                editor.document.text.slice(start..end).chars().collect();
            let reg = editor.yank_register;
            editor.registers.write(reg, text);
        }
        SpecialCommand::IndentLine => {
            let (start, end) = line_range(editor, count);
            apply_indent(editor, start, end, true);
        }
        SpecialCommand::DedentLine => {
            let (start, end) = line_range(editor, count);
            apply_indent(editor, start, end, false);
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
        SpecialCommand::DotRepeat => {
            // TODO: dot repeat (needs last-action
            // recording)
        }
    }
}

// ── Paste ─────────────────────────────────────────

fn paste(editor: &mut Editor, before: bool, count: usize) {
    let reg = editor.yank_register;
    let Some(text) = editor.registers.read(reg).map(ToString::to_string)
    else {
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

fn undo(editor: &mut Editor) {
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

fn redo(editor: &mut Editor) {
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

fn join_line(editor: &mut Editor) {
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

fn insert_char(editor: &mut Editor, c: char) {
    let pos = cursor_pos(editor);
    let mut s = String::new();
    s.push(c);
    let txn = Transaction::insert(pos, &s);
    if editor.document.apply_transaction(&txn).is_ok() {
        editor.view.cursor_col += 1;
        editor.view.desired_col = editor.view.cursor_col;
    }
}

fn insert_newline(editor: &mut Editor) {
    let pos = cursor_pos(editor);
    let txn = Transaction::insert(pos, "\n");
    if editor.document.apply_transaction(&txn).is_ok() {
        editor.view.cursor_line += 1;
        editor.view.cursor_col = 0;
        editor.view.desired_col = 0;
        editor.view.ensure_cursor_visible();
    }
}

fn delete_char_before_cursor(editor: &mut Editor) {
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

fn delete_char_at_cursor(editor: &mut Editor, count: usize) {
    let line_len = editor.current_line_len();
    if line_len == 0 {
        return;
    }
    let pos = cursor_pos(editor);
    let del_count = count.min(line_len - editor.view.cursor_col);
    if del_count == 0 {
        return;
    }
    let txn = Transaction::delete(pos, del_count);
    let inv = txn.invert(&editor.document.text);
    if editor.document.apply_transaction(&txn).is_ok() {
        editor.history.commit(txn, inv);
        editor.clamp_cursor_col();
    }
}

fn delete_word_back(editor: &mut Editor) {
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

fn delete_to_line_start(editor: &mut Editor) {
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

fn open_line_below(editor: &mut Editor) {
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

fn open_line_above(editor: &mut Editor) {
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
