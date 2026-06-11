use std::path::PathBuf;

use crossterm::event::{Event, KeyCode, KeyModifiers};

use ms_tui::buffer::{Buffer, Rect};
use ms_view::command::Action;
use ms_view::editor::Editor;
use ms_view::mode::Mode;

use crate::commands;
use crate::compositor::{
    Callback, Component, Context, CursorKind, EventResult, Position,
};
use crate::ui::file_picker;
use crate::ui::prompt::Prompt;

const GUTTER_WIDTH: u16 = 6;

/// The base editor layer — always at the bottom of
/// the compositor stack.
#[derive(Debug, Default)]
pub struct EditorView {
    /// True after Space is pressed, waiting for the
    /// leader-key follow-up (e.g. `f` for file picker).
    pending_space: bool,
}

impl EditorView {
    pub const fn new() -> Self {
        Self { pending_space: false }
    }
}

impl Component for EditorView {
    fn handle_event(
        &mut self,
        event: &Event,
        ctx: &mut Context,
    ) -> EventResult {
        let Event::Key(key) = event else {
            return EventResult::Ignored(None);
        };

        match ctx.editor.mode {
            Mode::Normal => self.handle_normal(key, ctx),
            Mode::Insert => {
                commands::handle_insert(ctx.editor, *key);
                EventResult::Consumed(None)
            }
            Mode::Command => {
                // Prompt handles this — safety fallback.
                commands::handle_command(ctx.editor, *key);
                EventResult::Consumed(None)
            }
            Mode::Visual { .. } => {
                ctx.editor.status_message = None;
                commands::handle_visual(ctx.editor, *key);
                EventResult::Consumed(None)
            }
        }
    }

    fn render(&mut self, area: Rect, surface: &mut Buffer, ctx: &mut Context) {
        let theme = &ctx.editor.theme;
        let line_num_style = theme.resolve("ui.linenr");
        let text_style = theme.resolve("ui.text");
        let cursor_ln_style = theme.resolve("ui.linenr.selected");
        let status_style = theme.resolve("ui.statusline");

        let text_height = area.height.saturating_sub(1);

        // ── Lines ──
        for row in 0..text_height {
            let doc_line = ctx.editor.view.scroll_offset + row as usize;

            if let Some(line) = ctx.editor.document.line(doc_line) {
                let num_str = format!("{:>4} ", doc_line + 1);
                let ln_style = if doc_line == ctx.editor.view.cursor_line {
                    cursor_ln_style
                } else {
                    line_num_style
                };
                surface.put_str(0, row, &num_str, ln_style);

                let line_text: String =
                    line.chars().take_while(|c| *c != '\n').collect();
                let max_cols = area.width.saturating_sub(GUTTER_WIDTH);
                let truncated: String =
                    line_text.chars().take(max_cols as usize).collect();
                surface.put_str(GUTTER_WIDTH, row, &truncated, text_style);
            } else {
                surface.put_str(0, row, "~", line_num_style);
            }
        }

        // ── Selection highlight ──
        if let Mode::Visual { kind, anchor } = ctx.editor.mode {
            render_selection(
                area,
                surface,
                ctx.editor,
                kind,
                anchor,
                text_height,
            );
        }

        // ── Status bar ──
        let status_row = area.height - 1;
        let status_text = commands::build_status_line(ctx.editor, area);
        surface.put_str(0, status_row, &status_text, status_style);
    }

    fn cursor(
        &self,
        _area: Rect,
        editor: &Editor,
    ) -> (Option<Position>, CursorKind) {
        let col = GUTTER_WIDTH + editor.view.cursor_col as u16;
        let row = editor.view.cursor_screen_row();
        let kind = match editor.mode {
            Mode::Normal | Mode::Visual { .. } => CursorKind::Block,
            Mode::Insert | Mode::Command => CursorKind::Bar,
        };
        (Some(Position { col, row }), kind)
    }
}

/// Overlay the `ui.selection` background over the
/// visually selected cells.
fn render_selection(
    area: Rect,
    surface: &mut Buffer,
    editor: &Editor,
    kind: ms_view::mode::VisualKind,
    anchor: usize,
    text_height: u16,
) {
    let sel_style = editor.theme.resolve("ui.selection");
    let (start, end, mt) = commands::visual_range(editor, kind, anchor);
    let text = &editor.document.text;
    let max_width = area.width.saturating_sub(GUTTER_WIDTH);

    for row in 0..text_height {
        let doc_line = editor.view.scroll_offset + row as usize;
        if editor.document.line(doc_line).is_none() {
            break;
        }
        let line_start = text.line_to_char(doc_line);
        let line_len = editor.document.line_len(doc_line);
        let line_end = line_start + line_len;
        if end <= line_start || start > line_end {
            continue;
        }

        let (from, to) = match mt {
            ms_view::command::MotionType::Linewise => (0, line_len.max(1)),
            ms_view::command::MotionType::Charwise => {
                let from = start.max(line_start) - line_start;
                // When the selection continues past
                // this line, include one cell for the
                // newline (like vim).
                let to = if end > line_end {
                    line_len + 1
                } else {
                    end - line_start
                };
                (from, to.max(from + 1))
            }
        };

        let width =
            u16::try_from(to - from).unwrap_or(u16::MAX).min(max_width);
        let x = GUTTER_WIDTH
            .saturating_add(u16::try_from(from).unwrap_or(u16::MAX));
        surface.set_style(x, row, width, sel_style);
    }
}

impl EditorView {
    /// Handle a keypress in Normal mode, including the
    /// Space-leader prefix for file picker etc.
    fn handle_normal(
        &mut self,
        key: &crossterm::event::KeyEvent,
        ctx: &mut Context,
    ) -> EventResult {
        // ── Space-leader sequences ──
        if self.pending_space {
            self.pending_space = false;
            if key.code == KeyCode::Char('p') {
                return self.open_file_picker();
            }
            // Unknown leader combo — feed Space then
            // this key to VimMachine.
            let space =
                commands::to_key_input(crossterm::event::KeyEvent::new(
                    KeyCode::Char(' '),
                    KeyModifiers::NONE,
                ));
            ctx.editor.vim.feed(space);
        }

        if key.code == KeyCode::Char(' ')
            && key.modifiers == KeyModifiers::NONE
        {
            self.pending_space = true;
            return EventResult::Consumed(None);
        }

        // ── Normal VimMachine dispatch ──
        let input = commands::to_key_input(*key);
        let action = ctx.editor.vim.feed(input);

        if matches!(action, Action::EnterCommand) {
            ctx.editor.mode = Mode::Command;
            let cb: Callback = Box::new(|compositor, _ctx| {
                compositor.push(Box::new(Prompt::command()));
            });
            return EventResult::Consumed(Some(cb));
        }

        ctx.editor.status_message = None;
        commands::execute_action(ctx.editor, action);
        EventResult::Consumed(None)
    }

    /// Push the file picker onto the compositor.
    #[allow(clippy::unused_self)]
    fn open_file_picker(&self) -> EventResult {
        let cb: Callback = Box::new(|compositor, _ctx| {
            let root =
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let picker = file_picker::file_picker(&root);
            compositor.push(Box::new(picker));
        });
        EventResult::Consumed(Some(cb))
    }
}
