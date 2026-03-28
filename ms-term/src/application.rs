use std::io;

use crossterm::cursor::SetCursorStyle;
use crossterm::event::{Event, EventStream, KeyEvent};
use crossterm::execute;
use futures_core::Stream;

use ms_tui::terminal::Terminal;
use ms_view::editor::Editor;

use crate::commands;
use crate::compositor::{Compositor, Context, CursorKind};
use crate::ui::editor::EditorView;

#[derive(Debug)]
pub struct Application {
    editor: Editor,
    terminal: Terminal,
    compositor: Compositor,
}

impl Application {
    /// # Errors
    /// Returns IO error if terminal cannot be created.
    pub fn new(editor: Editor) -> io::Result<Self> {
        let terminal = Terminal::stdout()?;
        let mut compositor = Compositor::new(terminal.area());
        compositor.push(Box::new(EditorView::new()));
        Ok(Self { editor, terminal, compositor })
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
    #[allow(clippy::future_not_send)]
    pub async fn run<S>(&mut self, input_stream: &mut S) -> io::Result<()>
    where
        S: Stream<Item = io::Result<Event>> + Unpin,
    {
        self.terminal.setup()?;
        self.editor.view.height =
            self.terminal.area().height.saturating_sub(1);
        self.compositor.resize(self.terminal.area());

        let result = self.event_loop(input_stream).await;

        self.terminal.teardown()?;
        result
    }

    #[allow(clippy::future_not_send)]
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
                ref event @ Event::Key(_) => {
                    let mut ctx = Context { editor: &mut self.editor };
                    self.compositor.handle_event(event, &mut ctx);
                }
                Event::Resize(_, _) => {
                    self.terminal.resize()?;
                    let area = self.terminal.area();
                    self.compositor.resize(area);
                    self.editor.view.height = area.height.saturating_sub(1);
                }
                _ => {}
            }

            self.render()?;
        }
    }

    fn render(&mut self) -> io::Result<()> {
        let area = self.terminal.area();
        self.terminal.buffer.clear();

        let mut ctx = Context { editor: &mut self.editor };
        self.compositor.render(area, &mut self.terminal.buffer, &mut ctx);

        self.terminal.flush()?;

        // Cursor
        let (pos, kind) = self.compositor.cursor(area, &self.editor);
        if let Some(pos) = pos {
            let cursor_style = match kind {
                CursorKind::Bar => SetCursorStyle::SteadyBar,
                CursorKind::Block | CursorKind::Hidden => {
                    SetCursorStyle::SteadyBlock
                }
            };
            execute!(io::stdout(), cursor_style)?;
            self.terminal.set_cursor(pos.col, pos.row)?;
        }

        Ok(())
    }
}

/// Public entry point for integration tests.
/// Dispatches key directly without compositor.
pub fn handle_key(editor: &mut Editor, key: KeyEvent) {
    commands::handle_key(editor, key);
}
