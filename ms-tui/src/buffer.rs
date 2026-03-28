use crate::style::Style;

/// A single cell in the terminal frame buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub symbol: String,
    pub style: Style,
}

impl Default for Cell {
    fn default() -> Self {
        Self { symbol: " ".to_owned(), style: Style::default() }
    }
}

/// Rectangular area on screen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Rect {
    pub const fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self { x, y, width, height }
    }

    pub fn area(self) -> u32 {
        u32::from(self.width) * u32::from(self.height)
    }
}

/// Frame buffer: a grid of cells representing the terminal
/// screen.
#[derive(Debug)]
pub struct Buffer {
    pub area: Rect,
    cells: Vec<Cell>,
}

impl Buffer {
    pub fn new(area: Rect) -> Self {
        let size = area.area() as usize;
        Self { area, cells: vec![Cell::default(); size] }
    }

    pub fn cell_mut(&mut self, x: u16, y: u16) -> Option<&mut Cell> {
        if x < self.area.x
            || y < self.area.y
            || x >= self.area.x + self.area.width
            || y >= self.area.y + self.area.height
        {
            return None;
        }
        let idx = (y - self.area.y) as usize * self.area.width as usize
            + (x - self.area.x) as usize;
        self.cells.get_mut(idx)
    }

    /// Write a string at (x, y) with the given style.
    /// Advances x for each character. Does not wrap.
    pub fn put_str(&mut self, x: u16, y: u16, s: &str, style: Style) {
        let mut col = x;
        for ch in s.chars() {
            if let Some(cell) = self.cell_mut(col, y) {
                cell.symbol.clear();
                cell.symbol.push(ch);
                cell.style = style;
            }
            col = col.saturating_add(1);
        }
    }

    pub fn clear(&mut self) {
        for cell in &mut self.cells {
            cell.symbol.clear();
            cell.symbol.push(' ');
            cell.style = Style::default();
        }
    }

    /// Iterate all cells with (x, y) positions.
    pub fn iter(&self) -> impl Iterator<Item = (u16, u16, &Cell)> {
        let w = self.area.width;
        let x0 = self.area.x;
        let y0 = self.area.y;
        self.cells.iter().enumerate().map(move |(i, cell)| {
            let x = x0 + (i % w as usize) as u16;
            let y = y0 + (i / w as usize) as u16;
            (x, y, cell)
        })
    }
}
