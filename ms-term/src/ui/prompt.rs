use crossterm::event::{Event, KeyCode};

use ms_tui::buffer::{Buffer, Rect};
use ms_view::editor::Editor;
use ms_view::mode::Mode;

use crate::commands;
use crate::compositor::{
    Callback, Component, Context, CursorKind, EventResult, Position,
};

/// Command-line prompt component (`:` commands).
/// Pushed as a layer on top of `EditorView`.
#[derive(Debug)]
pub struct Prompt {
    prefix: String,
    input: String,
}

impl Prompt {
    /// Create a command prompt (`:`).
    pub fn command() -> Self {
        Self { prefix: ":".to_owned(), input: String::new() }
    }
}

impl Component for Prompt {
    fn handle_event(
        &mut self,
        event: &Event,
        ctx: &mut Context,
    ) -> EventResult {
        let Event::Key(key) = event else {
            return EventResult::Ignored(None);
        };

        match key.code {
            KeyCode::Esc => {
                // Cancel — pop self, return to normal
                let cb: Callback = Box::new(pop_self);
                ctx.editor.mode = Mode::Normal;
                EventResult::Consumed(Some(cb))
            }
            KeyCode::Enter => {
                // Execute command, then pop
                let cmd = self.input.clone();
                let cb: Callback = Box::new(move |compositor, ctx| {
                    commands::execute_command(ctx.editor, &cmd);
                    pop_self(compositor, ctx);
                });
                ctx.editor.mode = Mode::Normal;
                EventResult::Consumed(Some(cb))
            }
            KeyCode::Backspace => {
                if self.input.is_empty() {
                    // Empty input + backspace = cancel
                    let cb: Callback = Box::new(pop_self);
                    ctx.editor.mode = Mode::Normal;
                    EventResult::Consumed(Some(cb))
                } else {
                    self.input.pop();
                    EventResult::Consumed(None)
                }
            }
            KeyCode::Char(c) => {
                self.input.push(c);
                EventResult::Consumed(None)
            }
            _ => EventResult::Consumed(None),
        }
    }

    fn render(&mut self, area: Rect, surface: &mut Buffer, ctx: &mut Context) {
        let status_row = area.height - 1;
        let text = format!("{}{}", self.prefix, self.input);
        let style = ctx.editor.theme.resolve("ui.statusline");
        surface.put_str(0, status_row, &text, style);
    }

    fn cursor(
        &self,
        area: Rect,
        _editor: &Editor,
    ) -> (Option<Position>, CursorKind) {
        let col = (self.prefix.len() + self.input.len()) as u16;
        let row = area.height - 1;
        (Some(Position { col, row }), CursorKind::Bar)
    }

    fn id(&self) -> Option<&'static str> {
        Some("prompt")
    }
}

fn pop_self(
    compositor: &mut crate::compositor::Compositor,
    _ctx: &mut Context,
) {
    compositor.remove("prompt");
}
