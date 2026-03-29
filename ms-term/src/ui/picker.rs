use std::path::PathBuf;
use std::sync::Arc;

use crossterm::event::{Event, KeyCode, KeyModifiers};
use nucleo::pattern::{CaseMatching, Normalization};
use nucleo::{Config, Nucleo, Utf32String};

use ms_tui::buffer::{Buffer, Rect};
use ms_view::editor::Editor;

use crate::compositor::{
    Callback, Component, Context, CursorKind, EventResult, Position,
};

/// Minimum terminal width to show the preview panel.
const MIN_PREVIEW_WIDTH: u16 = 72;

/// Maximum lines to load for file preview.
const MAX_PREVIEW_LINES: usize = 100;

/// Maximum bytes to read for file preview.
const MAX_PREVIEW_BYTES: u64 = 10_240;

type FormatFn<T> = Box<dyn Fn(&T) -> String>;
type SelectFn<T> = Box<dyn Fn(&mut Context, &T)>;
type PreviewFn<T> = Box<dyn Fn(&T) -> Option<PathBuf>>;

/// A generic fuzzy picker overlay.
///
/// Pushes as a compositor layer. Type to filter, navigate
/// with Up/Down, Enter to select, Esc to cancel.
#[allow(missing_debug_implementations)]
pub struct Picker<T: Send + Sync + 'static> {
    /// Nucleo fuzzy matcher instance.
    matcher: Nucleo<T>,
    /// User query string.
    query: String,
    /// Previous query (for append optimization).
    prev_query: String,
    /// Selected item index (within matched results).
    cursor: u32,
    /// Format an item for display.
    format_fn: FormatFn<T>,
    /// Extract a preview file path from an item.
    preview_fn: Option<PreviewFn<T>>,
    /// Callback when user selects an item.
    on_select: SelectFn<T>,
    /// Cached file preview content.
    preview_cache: Option<(PathBuf, Vec<String>)>,
    /// Whether preview panel is visible.
    show_preview: bool,
}

impl<T: Send + Sync + 'static> Picker<T> {
    /// Create a new picker with items.
    pub fn new(
        format_fn: FormatFn<T>,
        on_select: SelectFn<T>,
        items: Vec<T>,
    ) -> Self {
        let matcher = Nucleo::new(
            Config::DEFAULT,
            Arc::new(|| {}),
            None,
            1, // single match column
        );
        let injector = matcher.injector();
        for item in items {
            let text = Utf32String::from(format_fn(&item).as_str());
            injector.push(item, |_item, cols| {
                cols[0] = text;
            });
        }
        Self {
            matcher,
            query: String::new(),
            prev_query: String::new(),
            cursor: 0,
            format_fn,
            preview_fn: None,
            on_select,
            preview_cache: None,
            show_preview: true,
        }
    }

    /// Enable file preview.
    #[must_use]
    pub fn with_preview(mut self, f: PreviewFn<T>) -> Self {
        self.preview_fn = Some(f);
        self
    }

    /// Reparse the nucleo pattern after query changes.
    fn update_pattern(&mut self) {
        let is_append = self.query.starts_with(&self.prev_query);
        self.matcher.pattern.reparse(
            0,
            &self.query,
            CaseMatching::Smart,
            Normalization::Smart,
            is_append,
        );
        self.prev_query.clone_from(&self.query);
    }

    /// Get the currently selected item.
    fn selection(&self) -> Option<&T> {
        self.matcher
            .snapshot()
            .get_matched_item(self.cursor)
            .map(|item| item.data)
    }

    /// Total number of matched items.
    fn matched_count(&mut self) -> u32 {
        self.matcher.tick(10);
        self.matcher.snapshot().matched_item_count()
    }

    /// Move cursor down (wraps).
    fn move_down(&mut self) {
        let count = self.matched_count();
        if count > 0 {
            self.cursor = (self.cursor + 1) % count;
        }
        self.update_preview();
    }

    /// Move cursor up (wraps).
    fn move_up(&mut self) {
        let count = self.matched_count();
        if count > 0 {
            self.cursor = (self.cursor + count.saturating_sub(1)) % count;
        }
        self.update_preview();
    }

    /// Move cursor down by a page.
    fn page_down(&mut self, page: u32) {
        let count = self.matched_count();
        if count > 0 {
            self.cursor =
                self.cursor.saturating_add(page).min(count.saturating_sub(1));
        }
        self.update_preview();
    }

    /// Move cursor up by a page.
    fn page_up(&mut self, page: u32) {
        self.cursor = self.cursor.saturating_sub(page);
        self.update_preview();
    }

    /// Load or update the preview cache for the current
    /// selection.
    fn update_preview(&mut self) {
        let Some(ref preview_fn) = self.preview_fn else {
            return;
        };
        let Some(item) = self.selection() else {
            self.preview_cache = None;
            return;
        };
        let Some(path) = preview_fn(item) else {
            self.preview_cache = None;
            return;
        };

        // Skip if already cached for this path.
        if let Some((ref cached, _)) = self.preview_cache {
            if *cached == path {
                return;
            }
        }

        let lines = load_preview(&path);
        self.preview_cache = Some((path, lines));
    }
}

/// Picker area within the terminal.
fn picker_area(terminal: Rect) -> Rect {
    let margin_x = 2;
    let margin_y = 1;
    let w = terminal.width.saturating_sub(margin_x * 2).max(10);
    let h = terminal.height.saturating_sub(margin_y * 2).max(5);
    Rect::new(margin_x, margin_y, w, h)
}

impl<T: Send + Sync + 'static> Component for Picker<T> {
    fn handle_event(
        &mut self,
        event: &Event,
        ctx: &mut Context,
    ) -> EventResult {
        let Event::Key(key) = event else {
            return EventResult::Consumed(None);
        };

        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        match key.code {
            KeyCode::Esc | KeyCode::Char('c')
                if key.code == KeyCode::Esc || ctrl =>
            {
                let cb: Callback = Box::new(pop_picker);
                EventResult::Consumed(Some(cb))
            }
            KeyCode::Enter => {
                if let Some(item) = self.selection() {
                    (self.on_select)(ctx, item);
                }
                let cb: Callback = Box::new(pop_picker);
                EventResult::Consumed(Some(cb))
            }
            KeyCode::Up | KeyCode::BackTab | KeyCode::Char('p')
                if key.code != KeyCode::Char('p') || ctrl =>
            {
                self.move_up();
                EventResult::Consumed(None)
            }
            KeyCode::Down | KeyCode::Tab | KeyCode::Char('n')
                if key.code != KeyCode::Char('n') || ctrl =>
            {
                self.move_down();
                EventResult::Consumed(None)
            }
            KeyCode::PageUp => {
                self.page_up(10);
                EventResult::Consumed(None)
            }
            KeyCode::PageDown => {
                self.page_down(10);
                EventResult::Consumed(None)
            }
            KeyCode::Char('t') if ctrl => {
                self.show_preview = !self.show_preview;
                EventResult::Consumed(None)
            }
            KeyCode::Backspace => {
                if self.query.is_empty() {
                    let cb: Callback = Box::new(pop_picker);
                    EventResult::Consumed(Some(cb))
                } else {
                    self.query.pop();
                    self.cursor = 0;
                    self.update_pattern();
                    self.update_preview();
                    EventResult::Consumed(None)
                }
            }
            KeyCode::Char(c) if !ctrl => {
                self.query.push(c);
                self.cursor = 0;
                self.update_pattern();
                self.update_preview();
                EventResult::Consumed(None)
            }
            _ => EventResult::Consumed(None),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn render(&mut self, area: Rect, surface: &mut Buffer, ctx: &mut Context) {
        let theme = &ctx.editor.theme;
        let popup_style = theme.resolve("ui.popup");
        let selected_style = theme.resolve("ui.popup.selected");
        let sep_style = theme.resolve("ui.separator");
        let text_style = theme.resolve("ui.text");

        let outer = picker_area(area);

        // Fill background.
        fill_area(surface, outer, popup_style);

        // Draw border.
        draw_border(surface, outer, sep_style);

        // Inner area (inside border).
        let inner = Rect::new(
            outer.x + 1,
            outer.y + 1,
            outer.width.saturating_sub(2),
            outer.height.saturating_sub(2),
        );

        if inner.width < 4 || inner.height < 3 {
            return;
        }

        // Determine list/preview split.
        let show_preview = self.show_preview
            && self.preview_fn.is_some()
            && outer.width >= MIN_PREVIEW_WIDTH;

        // Ensure preview is loaded before taking snapshot.
        if show_preview && self.preview_cache.is_none() {
            self.update_preview();
        }

        // Tick matcher and get fresh results.
        self.matcher.tick(10);
        let snapshot = self.matcher.snapshot();
        let total = snapshot.matched_item_count();

        // ── Prompt row ──
        let prompt_text = format!("> {}", self.query);
        surface.put_str(inner.x, inner.y, &prompt_text, popup_style);

        let count_text = format!("{total}/{}", snapshot.item_count());
        let count_x =
            (inner.x + inner.width).saturating_sub(count_text.len() as u16);
        surface.put_str(count_x, inner.y, &count_text, popup_style);

        // ── Separator ──
        let sep_y = inner.y + 1;
        for x in inner.x..inner.x + inner.width {
            set_symbol(surface, x, sep_y, "\u{2500}", sep_style);
        }

        // ── Content area ──
        let content_y = sep_y + 1;
        let content_h = inner.height.saturating_sub(2) as u32;

        if content_h == 0 {
            return;
        }

        let list_width =
            if show_preview { inner.width / 2 } else { inner.width };

        // Page-based scrolling (like Helix).
        let offset = self.cursor - (self.cursor % content_h.max(1));
        let end = offset.saturating_add(content_h).min(total);

        for (i, item) in snapshot.matched_items(offset..end).enumerate() {
            let row = content_y + i as u16;
            let is_selected = offset + i as u32 == self.cursor;
            let style = if is_selected { selected_style } else { popup_style };

            // Clear the row.
            for x in inner.x..inner.x + list_width {
                set_symbol(surface, x, row, " ", style);
            }

            let prefix = if is_selected { " > " } else { "   " };
            let display = (self.format_fn)(item.data);
            let text = format!("{prefix}{display}");
            let max_chars = list_width.saturating_sub(1) as usize;
            let truncated: String = text.chars().take(max_chars).collect();
            surface.put_str(inner.x, row, &truncated, style);
        }

        // ── Preview panel ──
        if show_preview {
            let sep_x = inner.x + list_width;
            let preview_x = sep_x + 1;
            let preview_w = inner.width.saturating_sub(list_width + 1);

            // Vertical separator.
            for row in content_y..content_y + content_h as u16 {
                set_symbol(surface, sep_x, row, "\u{2502}", sep_style);
            }

            if let Some((_, ref lines)) = self.preview_cache {
                for (i, line) in
                    lines.iter().take(content_h as usize).enumerate()
                {
                    let row = content_y + i as u16;
                    let truncated: String =
                        line.chars().take(preview_w as usize).collect();
                    surface.put_str(preview_x, row, &truncated, text_style);
                }
            }
        }
    }

    fn cursor(
        &self,
        area: Rect,
        _editor: &Editor,
    ) -> (Option<Position>, CursorKind) {
        let outer = picker_area(area);
        let col = outer.x + 1 + 2 + self.query.len() as u16;
        let row = outer.y + 1;
        (Some(Position { col, row }), CursorKind::Bar)
    }

    fn id(&self) -> Option<&'static str> {
        Some("picker")
    }
}

/// Remove the picker layer from the compositor.
fn pop_picker(
    compositor: &mut crate::compositor::Compositor,
    _ctx: &mut Context,
) {
    compositor.remove("picker");
}

/// Fill an area with a single style.
fn fill_area(surface: &mut Buffer, area: Rect, style: ms_tui::style::Style) {
    for row in area.y..area.y + area.height {
        for col in area.x..area.x + area.width {
            if let Some(cell) = surface.cell_mut(col, row) {
                " ".clone_into(&mut cell.symbol);
                cell.style = style;
            }
        }
    }
}

/// Draw a box border around a rect.
fn draw_border(surface: &mut Buffer, area: Rect, style: ms_tui::style::Style) {
    let x1 = area.x;
    let y1 = area.y;
    let x2 = area.x + area.width.saturating_sub(1);
    let y2 = area.y + area.height.saturating_sub(1);

    set_symbol(surface, x1, y1, "\u{250c}", style);
    set_symbol(surface, x2, y1, "\u{2510}", style);
    set_symbol(surface, x1, y2, "\u{2514}", style);
    set_symbol(surface, x2, y2, "\u{2518}", style);

    for x in (x1 + 1)..x2 {
        set_symbol(surface, x, y1, "\u{2500}", style);
        set_symbol(surface, x, y2, "\u{2500}", style);
    }

    for y in (y1 + 1)..y2 {
        set_symbol(surface, x1, y, "\u{2502}", style);
        set_symbol(surface, x2, y, "\u{2502}", style);
    }
}

fn set_symbol(
    surface: &mut Buffer,
    x: u16,
    y: u16,
    symbol: &str,
    style: ms_tui::style::Style,
) {
    if let Some(cell) = surface.cell_mut(x, y) {
        symbol.clone_into(&mut cell.symbol);
        cell.style = style;
    }
}

/// Load a file preview: first N lines, capped by bytes.
fn load_preview(path: &std::path::Path) -> Vec<String> {
    use std::io::{BufRead, BufReader, Read};

    let Ok(file) = std::fs::File::open(path) else {
        return vec!["<cannot open>".to_owned()];
    };

    // Check for binary: read first 512 bytes.
    let mut header = [0u8; 512];
    let mut file = BufReader::new(file);
    let Ok(n) = file.by_ref().take(512).read(&mut header) else {
        return vec!["<read error>".to_owned()];
    };

    if n == 0 {
        return vec!["<empty>".to_owned()];
    }

    if header[..n].contains(&0) {
        return vec!["<binary>".to_owned()];
    }

    // Re-open to read lines from the start.
    let Ok(file) = std::fs::File::open(path) else {
        return vec!["<cannot open>".to_owned()];
    };
    let reader = BufReader::new(file.take(MAX_PREVIEW_BYTES));
    let mut lines = Vec::new();
    for line in reader.lines().take(MAX_PREVIEW_LINES) {
        match line {
            Ok(l) => lines.push(l),
            Err(_) => break,
        }
    }
    lines
}
