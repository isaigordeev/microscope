use std::io::{self, Stdout, Write};

use crate::backend::Backend;
use crate::buffer::{Buffer, Rect};

/// Terminal wrapper: manages the backend and frame buffer.
#[derive(Debug)]
pub struct Terminal<W: Write = Stdout> {
    pub backend: Backend<W>,
    pub buffer: Buffer,
}

impl Terminal<Stdout> {
    /// Create a terminal on stdout.
    ///
    /// # Errors
    /// Returns IO error if size query fails.
    pub fn stdout() -> io::Result<Self> {
        let backend = Backend::new(io::stdout());
        let (w, h) = backend.size()?;
        let area = Rect::new(0, 0, w, h);
        Ok(Self {
            backend,
            buffer: Buffer::new(area),
        })
    }
}

impl<W: Write> Terminal<W> {
    /// Enter raw mode and alternate screen.
    ///
    /// # Errors
    /// Returns IO error if terminal setup fails.
    pub fn setup(&mut self) -> io::Result<()> {
        self.backend.setup()
    }

    /// Leave raw mode and alternate screen.
    ///
    /// # Errors
    /// Returns IO error if terminal teardown fails.
    pub fn teardown(&mut self) -> io::Result<()> {
        self.backend.teardown()
    }

    /// Resize the frame buffer to match terminal size.
    ///
    /// # Errors
    /// Returns IO error if size query fails.
    pub fn resize(&mut self) -> io::Result<()> {
        let (w, h) = self.backend.size()?;
        let area = Rect::new(0, 0, w, h);
        self.buffer = Buffer::new(area);
        Ok(())
    }

    /// Clear the buffer and flush to terminal.
    ///
    /// # Errors
    /// Returns IO error if rendering fails.
    pub fn clear(&mut self) -> io::Result<()> {
        self.buffer.clear();
        self.backend.render(&self.buffer)
    }

    /// Flush the current buffer to the terminal.
    ///
    /// # Errors
    /// Returns IO error if rendering fails.
    pub fn flush(&mut self) -> io::Result<()> {
        self.backend.render(&self.buffer)?;
        self.buffer.clear();
        Ok(())
    }

    /// Position and show the cursor.
    ///
    /// # Errors
    /// Returns IO error if cursor positioning fails.
    pub fn set_cursor(
        &mut self,
        x: u16,
        y: u16,
    ) -> io::Result<()> {
        self.backend.set_cursor(x, y)
    }

    /// Get the current viewport area.
    pub const fn area(&self) -> Rect {
        self.buffer.area
    }
}