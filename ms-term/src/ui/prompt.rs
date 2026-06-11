use crossterm::event::Event;

use ms_tui::buffer::{Buffer, Rect};
use ms_view::editor::Editor;
use ms_view::mode::Mode;

use crate::commands;
use crate::compositor::{
    Component, Context, CursorKind, EventResult, Position,
};

/// What the prompt is for.
#[derive(Debug, Clone, Copy)]
enum PromptKind {
    Command,
    Search { backward: bool },
}

/// Command-line prompt component (`:` commands and
/// `/`/`?` search). Pushed as a layer on top of
/// `EditorView`.
#[derive(Debug)]
pub struct Prompt {
    kind: PromptKind,
    prefix: String,
}

impl Prompt {
    /// Create a command prompt (`:`).
    pub fn command() -> Self {
        Self { kind: PromptKind::Command, prefix: ":".to_owned() }
    }

    /// Create a search prompt (`/` or `?`).
    pub fn search(backward: bool) -> Self {
        Self {
            kind: PromptKind::Search { backward },
            prefix: if backward { "?" } else { "/" }.to_owned(),
        }
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

        // Prompt state lives on the editor
        // (command_buffer) so the headless path,
        // history cycling and incremental search all
        // share the logic.
        match self.kind {
            PromptKind::Search { backward } => {
                commands::handle_search_key(ctx.editor, *key, backward);
                if matches!(ctx.editor.mode, Mode::Search { .. }) {
                    return EventResult::Consumed(None);
                }
            }
            PromptKind::Command => {
                commands::handle_command(ctx.editor, *key);
                if matches!(ctx.editor.mode, Mode::Command) {
                    return EventResult::Consumed(None);
                }
            }
        }
        EventResult::Consumed(Some(Box::new(pop_self)))
    }

    fn render(&mut self, area: Rect, surface: &mut Buffer, ctx: &mut Context) {
        let status_row = area.height - 1;
        let text = format!("{}{}", self.prefix, ctx.editor.command_buffer);
        let style = ctx.editor.theme.resolve("ui.statusline");
        surface.put_str(0, status_row, &text, style);
    }

    fn cursor(
        &self,
        area: Rect,
        editor: &Editor,
    ) -> (Option<Position>, CursorKind) {
        let col =
            (self.prefix.len() + editor.command_buffer.chars().count()) as u16;
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
