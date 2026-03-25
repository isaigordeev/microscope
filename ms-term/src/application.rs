use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent};

use ms_tui::style::{Color, Style};
use ms_tui::terminal::Terminal;
use ms_view::editor::Editor;

/// Gutter width: line numbers column.
const GUTTER_WIDTH: u16 = 6;

/// Run the main event loop.
///
/// # Errors
/// Returns IO error on terminal or rendering failure.
pub(super) fn run(mut editor: Editor) -> io::Result<()> {
    let mut terminal = Terminal::stdout()?;
    terminal.setup()?;

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

        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    handle_key(editor, key);
                }
                Event::Resize(_, _) => {
                    terminal.resize()?;
                    editor.view.height =
                        terminal.area().height;
                }
                _ => {}
            }
        }

        if editor.should_quit {
            return Ok(());
        }
    }
}

fn handle_key(editor: &mut Editor, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') => editor.should_quit = true,
        KeyCode::Char('j') | KeyCode::Down => {
            let max = editor.max_line();
            editor.view.move_down(max);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            editor.view.move_up();
        }
        KeyCode::Char('g') => {
            // gg: go to top (simplified: single g)
            editor.view.cursor_line = 0;
            editor.view.ensure_cursor_visible();
        }
        KeyCode::Char('G') => {
            editor.view.cursor_line = editor.max_line();
            editor.view.ensure_cursor_visible();
        }
        _ => {}
    }
}

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

    for row in 0..area.height {
        let doc_line =
            editor.view.scroll_offset + row as usize;

        if let Some(line) =
            editor.document.line(doc_line)
        {
            // Line number
            let num_str =
                format!("{:>4} ", doc_line + 1);
            let ln_style =
                if doc_line == editor.view.cursor_line {
                    cursor_line_num_style
                } else {
                    line_num_style
                };
            terminal.buffer.put_str(
                0,
                row,
                &num_str,
                ln_style,
            );

            // Text content
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
            // Past end of document: draw tilde
            terminal.buffer.put_str(
                0,
                row,
                "~",
                line_num_style,
            );
        }
    }

    terminal.flush()?;

    // Position cursor
    let cursor_x = GUTTER_WIDTH;
    let cursor_y = editor.view.cursor_screen_row();
    terminal.set_cursor(cursor_x, cursor_y)?;

    Ok(())
}