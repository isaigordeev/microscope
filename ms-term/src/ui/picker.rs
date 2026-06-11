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

/// Minimum picker width to show the preview panel.
const MIN_PREVIEW_WIDTH: u16 = 50;

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
/// with Up/Down, Enter to select, Esc to cancel. Renders
/// fzf-classic style: bottom-anchored list, prompt at the
/// very bottom, count line just above.
#[allow(missing_debug_implementations)]
pub struct Picker<T: Send + Sync + 'static> {
    matcher: Nucleo<T>,
    query: String,
    prev_query: String,
    /// Selected item index (within matched results).
    /// Cursor 0 is the best match — rendered at the bottom.
    cursor: u32,
    format_fn: FormatFn<T>,
    preview_fn: Option<PreviewFn<T>>,
    on_select: SelectFn<T>,
    preview_cache: Option<(PathBuf, Vec<String>)>,
    show_preview: bool,
    /// Static text shown before the query on the prompt line
    /// (e.g. `~/microscope/`).
    prompt_prefix: String,
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
            prompt_prefix: String::new(),
        }
    }

    /// Enable file preview.
    #[must_use]
    pub fn with_preview(mut self, f: PreviewFn<T>) -> Self {
        self.preview_fn = Some(f);
        self
    }

    /// Set a static prompt prefix shown before the query.
    #[must_use]
    pub fn with_prompt_prefix(mut self, prefix: String) -> Self {
        self.prompt_prefix = prefix;
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

/// Picker area: roughly half the terminal, centered.
fn picker_area(terminal: Rect) -> Rect {
    let w = (terminal.width).max(50).min(terminal.width);
    let h = (terminal.height / 2).max(10).min(terminal.height);
    let x = terminal.width.saturating_sub(w) / 2;
    let y = terminal.height.saturating_sub(h) / 2;
    Rect::new(x, y, w, h)
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
            KeyCode::Char('p') if ctrl => {
                self.move_down();
                EventResult::Consumed(None)
            }
            KeyCode::Char('n') if ctrl => {
                self.move_up();
                EventResult::Consumed(None)
            }
            KeyCode::Up | KeyCode::BackTab => {
                self.move_up();
                EventResult::Consumed(None)
            }
            KeyCode::Down | KeyCode::Tab => {
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
        let selected_style = theme
            .get("ui.picker.selected")
            .copied()
            .unwrap_or_else(|| theme.resolve("ui.popup.selected"));
        let sep_style = theme.resolve("ui.separator");
        let text_style = theme.resolve("ui.text");
        let count_style = theme.resolve("ui.picker.count");
        let prompt_style = theme.resolve("ui.picker.prompt");

        let outer = picker_area(area);

        fill_area(surface, outer, popup_style);
        draw_border(surface, outer, sep_style);

        let inner = Rect::new(
            outer.x + 1,
            outer.y + 1,
            outer.width.saturating_sub(2),
            outer.height.saturating_sub(2),
        );

        if inner.width < 4 || inner.height < 3 {
            return;
        }

        let show_preview = self.show_preview
            && self.preview_fn.is_some()
            && outer.width >= MIN_PREVIEW_WIDTH;

        if show_preview && self.preview_cache.is_none() {
            self.update_preview();
        }

        self.matcher.tick(10);
        let snapshot = self.matcher.snapshot();
        let total = snapshot.matched_item_count();

        let list_width =
            if show_preview { inner.width / 2 } else { inner.width };

        // Bottom two rows are reserved for count + prompt; items
        // fill what's left, growing upward from just above count.
        let prompt_y = inner.y + inner.height - 1;
        let count_y = prompt_y - 1;
        let items_h = u32::from(inner.height.saturating_sub(2));

        // ── Items (best match at bottom) ──
        if items_h > 0 {
            let page = self.cursor / items_h;
            let offset = page * items_h;
            let end = offset.saturating_add(items_h).min(total);

            for (i, item) in snapshot.matched_items(offset..end).enumerate() {
                let row = count_y.saturating_sub(1 + i as u16);
                let global = offset + i as u32;
                let is_selected = global == self.cursor;
                let style =
                    if is_selected { selected_style } else { popup_style };

                for x in inner.x..inner.x + list_width {
                    set_symbol(surface, x, row, " ", style);
                }

                let display = (self.format_fn)(item.data);
                let text = format!("  {display}");
                let max_chars = list_width.saturating_sub(1) as usize;
                let truncated: String = text.chars().take(max_chars).collect();
                surface.put_str(inner.x, row, &truncated, style);
            }
        }

        // ── Count line ──
        for x in inner.x..inner.x + list_width {
            set_symbol(surface, x, count_y, " ", popup_style);
        }
        let count_text = format!("  {}/{} (0)", total, snapshot.item_count());
        let max_count = list_width.saturating_sub(1) as usize;
        let count_truncated: String =
            count_text.chars().take(max_count).collect();
        surface.put_str(inner.x, count_y, &count_truncated, count_style);

        // ── Prompt line ──
        for x in inner.x..inner.x + list_width {
            set_symbol(surface, x, prompt_y, " ", popup_style);
        }
        let prompt_text = format!("  {}{}", self.prompt_prefix, self.query);
        let max_prompt = list_width.saturating_sub(1) as usize;
        let prompt_truncated: String =
            prompt_text.chars().take(max_prompt).collect();
        surface.put_str(inner.x, prompt_y, &prompt_truncated, prompt_style);

        // ── Preview panel ──
        if show_preview {
            let sep_x = inner.x + list_width;
            let preview_x = sep_x + 1;
            let preview_w = inner.width.saturating_sub(list_width + 1);

            for row in inner.y..inner.y + inner.height {
                set_symbol(surface, sep_x, row, "\u{2502}", sep_style);
            }

            if let Some((_, ref lines)) = self.preview_cache {
                for (i, line) in
                    lines.iter().take(inner.height as usize).enumerate()
                {
                    let row = inner.y + i as u16;
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
        let prefix_chars = self.prompt_prefix.chars().count() as u16;
        let query_chars = self.query.chars().count() as u16;
        let col = outer.x + 1 + 2 + prefix_chars + query_chars;
        let row = outer.y + outer.height.saturating_sub(2);
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

    set_symbol(surface, x1, y1, "\u{256d}", style);
    set_symbol(surface, x2, y1, "\u{256e}", style);
    set_symbol(surface, x1, y2, "\u{2570}", style);
    set_symbol(surface, x2, y2, "\u{256f}", style);

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
