use std::io::{self, Write};

use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::style::{
    Attribute, Print, SetAttribute, SetBackgroundColor,
    SetForegroundColor,
};
use crossterm::terminal::{
    self, Clear, ClearType, EnterAlternateScreen,
    LeaveAlternateScreen,
};
use crossterm::{execute, queue};

use crate::buffer::Buffer;
use crate::style::{Color, Style};

/// Crossterm-based terminal backend.
#[derive(Debug)]
pub struct Backend<W: Write> {
    writer: W,
}

impl<W: Write> Backend<W> {
    pub const fn new(writer: W) -> Self {
        Self { writer }
    }

    /// Enter raw mode + alternate screen.
    ///
    /// # Errors
    /// Returns IO error if terminal setup fails.
    pub fn setup(&mut self) -> io::Result<()> {
        terminal::enable_raw_mode()?;
        execute!(
            self.writer,
            EnterAlternateScreen,
            Hide,
            Clear(ClearType::All)
        )
    }

    /// Leave alternate screen + disable raw mode.
    ///
    /// # Errors
    /// Returns IO error if terminal teardown fails.
    pub fn teardown(&mut self) -> io::Result<()> {
        execute!(
            self.writer,
            Show,
            LeaveAlternateScreen
        )?;
        terminal::disable_raw_mode()
    }

    /// Get terminal size as (width, height).
    ///
    /// # Errors
    /// Returns IO error if size query fails.
    pub fn size(&self) -> io::Result<(u16, u16)> {
        terminal::size()
    }

    /// Flush the buffer to the terminal.
    ///
    /// # Errors
    /// Returns IO error if rendering fails.
    pub fn render(&mut self, buf: &Buffer) -> io::Result<()> {
        let mut last_style = Style::default();
        let mut last_x: u16 = u16::MAX;
        let mut last_y: u16 = u16::MAX;

        for (x, y, cell) in buf.iter() {
            // Only move cursor if not contiguous
            if y != last_y || x != last_x + 1 {
                queue!(self.writer, MoveTo(x, y))?;
            }

            if cell.style != last_style {
                self.apply_style(cell.style)?;
                last_style = cell.style;
            }

            queue!(self.writer, Print(&cell.symbol))?;
            last_x = x;
            last_y = y;
        }

        // Reset attributes at the end
        queue!(
            self.writer,
            SetAttribute(Attribute::Reset)
        )?;
        self.writer.flush()
    }

    /// Position the cursor at (x, y) and show it.
    ///
    /// # Errors
    /// Returns IO error if cursor positioning fails.
    pub fn set_cursor(
        &mut self,
        x: u16,
        y: u16,
    ) -> io::Result<()> {
        execute!(self.writer, MoveTo(x, y), Show)
    }

    fn apply_style(
        &mut self,
        style: Style,
    ) -> io::Result<()> {
        // Reset first to clear previous state
        queue!(
            self.writer,
            SetAttribute(Attribute::Reset)
        )?;

        if let Some(fg) = style.fg {
            queue!(
                self.writer,
                SetForegroundColor(to_crossterm_color(fg))
            )?;
        }
        if let Some(bg) = style.bg {
            queue!(
                self.writer,
                SetBackgroundColor(to_crossterm_color(bg))
            )?;
        }
        if style.modifier.bold {
            queue!(
                self.writer,
                SetAttribute(Attribute::Bold)
            )?;
        }
        if style.modifier.italic {
            queue!(
                self.writer,
                SetAttribute(Attribute::Italic)
            )?;
        }
        if style.modifier.underline {
            queue!(
                self.writer,
                SetAttribute(Attribute::Underlined)
            )?;
        }
        if style.modifier.dim {
            queue!(
                self.writer,
                SetAttribute(Attribute::Dim)
            )?;
        }
        if style.modifier.strikethrough {
            queue!(
                self.writer,
                SetAttribute(Attribute::CrossedOut)
            )?;
        }
        Ok(())
    }
}

const fn to_crossterm_color(
    color: Color,
) -> crossterm::style::Color {
    match color {
        Color::Rgb(r, g, b) => {
            crossterm::style::Color::Rgb { r, g, b }
        }
        Color::Indexed(i) => {
            crossterm::style::Color::AnsiValue(i)
        }
        Color::Reset => crossterm::style::Color::Reset,
    }
}