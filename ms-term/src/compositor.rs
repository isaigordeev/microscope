use std::any::Any;

use crossterm::event::Event;

use ms_tui::buffer::{Buffer, Rect};
use ms_view::editor::Editor;

/// Shared context passed to every component.
#[derive(Debug)]
pub struct Context<'a> {
    pub editor: &'a mut Editor,
}

/// Result of event handling by a component.
#[allow(missing_debug_implementations)]
pub enum EventResult {
    /// Event was consumed; stop propagation.
    Consumed(Option<Callback>),
    /// Event was not handled; pass to next layer.
    Ignored(Option<Callback>),
}

/// Deferred mutation callback executed after event
/// propagation completes.
pub type Callback = Box<dyn FnOnce(&mut Compositor, &mut Context)>;

/// Screen position (col, row).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub col: u16,
    pub row: u16,
}

/// Cursor visual style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorKind {
    Block,
    Bar,
    Hidden,
}

// ── Component trait ─────────────────────────────────

/// A UI component that can handle events, render
/// itself, and report cursor position.
///
/// Modeled after Helix's `Component` trait.
pub trait Component: Any {
    /// Handle a terminal event. Return `Consumed` to
    /// stop propagation, `Ignored` to pass to the
    /// next layer.
    fn handle_event(
        &mut self,
        _event: &Event,
        _ctx: &mut Context,
    ) -> EventResult {
        EventResult::Ignored(None)
    }

    /// Draw onto the shared buffer.
    fn render(&mut self, area: Rect, surface: &mut Buffer, ctx: &mut Context);

    /// Report cursor position and style.
    fn cursor(
        &self,
        _area: Rect,
        _editor: &Editor,
    ) -> (Option<Position>, CursorKind) {
        (None, CursorKind::Hidden)
    }

    /// Hint: does this component need a redraw?
    fn should_update(&self) -> bool {
        true
    }

    /// Desired size for layout negotiation (popups).
    fn required_size(&mut self, _viewport: (u16, u16)) -> Option<(u16, u16)> {
        None
    }

    /// Optional identifier for finding/replacing layers.
    fn id(&self) -> Option<&'static str> {
        None
    }

    /// Type name for debugging.
    fn type_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
}

// ── Downcast support ────────────────────────────────

impl dyn Component {
    /// Downcast to a concrete component type.
    pub fn downcast_ref<T: Component>(&self) -> Option<&T> {
        (self as &dyn Any).downcast_ref()
    }

    /// Downcast to a concrete component type (mut).
    pub fn downcast_mut<T: Component>(&mut self) -> Option<&mut T> {
        (self as &mut dyn Any).downcast_mut()
    }
}

// ── Compositor ──────────────────────────────────────

/// Z-ordered stack of UI layers.
#[derive(Default)]
pub struct Compositor {
    layers: Vec<Box<dyn Component>>,
    area: Rect,
}

impl std::fmt::Debug for Compositor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Compositor")
            .field("layers", &self.layers.len())
            .field("area", &self.area)
            .finish()
    }
}

impl Compositor {
    pub fn new(area: Rect) -> Self {
        Self { layers: Vec::new(), area }
    }

    pub const fn area(&self) -> Rect {
        self.area
    }

    pub const fn resize(&mut self, area: Rect) {
        self.area = area;
    }

    /// Push a new layer on top.
    pub fn push(&mut self, layer: Box<dyn Component>) {
        self.layers.push(layer);
    }

    /// Remove the topmost layer.
    pub fn pop(&mut self) -> Option<Box<dyn Component>> {
        self.layers.pop()
    }

    /// Remove a layer by its `id()`.
    pub fn remove(&mut self, id: &str) {
        self.layers.retain(|c| c.id() != Some(id));
    }

    /// Replace an existing layer by id, or push if not
    /// found.
    pub fn replace_or_push(&mut self, id: &str, layer: Box<dyn Component>) {
        if let Some(pos) = self.layers.iter().position(|c| c.id() == Some(id))
        {
            self.layers[pos] = layer;
        } else {
            self.layers.push(layer);
        }
    }

    /// Find a component by type.
    pub fn find<T: Component>(&mut self) -> Option<&mut T> {
        self.layers.iter_mut().find_map(|c| c.downcast_mut::<T>())
    }

    /// Route an event through the layer stack (topmost
    /// first). Collects callbacks and executes them
    /// after propagation.
    pub fn handle_event(&mut self, event: &Event, ctx: &mut Context) {
        let mut callbacks: Vec<Callback> = Vec::new();

        // Reverse iterate — topmost layer gets first dibs
        for layer in self.layers.iter_mut().rev() {
            match layer.handle_event(event, ctx) {
                EventResult::Consumed(cb) => {
                    if let Some(cb) = cb {
                        callbacks.push(cb);
                    }
                    break;
                }
                EventResult::Ignored(cb) => {
                    if let Some(cb) = cb {
                        callbacks.push(cb);
                    }
                }
            }
        }

        for cb in callbacks {
            cb(self, ctx);
        }
    }

    /// Render all layers bottom-to-top (painter's
    /// algorithm).
    pub fn render(
        &mut self,
        area: Rect,
        surface: &mut Buffer,
        ctx: &mut Context,
    ) {
        for layer in &mut self.layers {
            layer.render(area, surface, ctx);
        }
    }

    /// Query layers for cursor position (topmost with a
    /// cursor wins).
    pub fn cursor(
        &self,
        area: Rect,
        editor: &Editor,
    ) -> (Option<Position>, CursorKind) {
        for layer in self.layers.iter().rev() {
            let (pos, kind) = layer.cursor(area, editor);
            if pos.is_some() {
                return (pos, kind);
            }
        }
        (None, CursorKind::Hidden)
    }

    /// Number of layers (for testing).
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }
}

// ── Tests ───────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // -- Mock components for testing --

    struct ConsumeAll;
    impl Component for ConsumeAll {
        fn render(
            &mut self,
            _area: Rect,
            _surface: &mut Buffer,
            _ctx: &mut Context,
        ) {
        }
        fn handle_event(
            &mut self,
            _event: &Event,
            _ctx: &mut Context,
        ) -> EventResult {
            EventResult::Consumed(None)
        }
        fn id(&self) -> Option<&'static str> {
            Some("consume_all")
        }
    }

    struct IgnoreAll;
    impl Component for IgnoreAll {
        fn render(
            &mut self,
            _area: Rect,
            _surface: &mut Buffer,
            _ctx: &mut Context,
        ) {
        }
    }

    struct CursorAt {
        pos: Position,
        kind: CursorKind,
    }
    impl Component for CursorAt {
        fn render(
            &mut self,
            _area: Rect,
            _surface: &mut Buffer,
            _ctx: &mut Context,
        ) {
        }
        fn cursor(
            &self,
            _area: Rect,
            _editor: &Editor,
        ) -> (Option<Position>, CursorKind) {
            (Some(self.pos), self.kind)
        }
    }

    fn test_editor() -> Editor {
        use ms_view::document::Document;
        use ropey::Rope;
        let doc = Document {
            text: Rope::from("hello"),
            path: None,
            modified: false,
        };
        Editor::new(doc, 24)
    }

    fn area() -> Rect {
        Rect::new(0, 0, 80, 24)
    }

    #[test]
    fn push_pop_ordering() {
        let mut c = Compositor::new(area());
        assert_eq!(c.layer_count(), 0);

        c.push(Box::new(IgnoreAll));
        c.push(Box::new(ConsumeAll));
        assert_eq!(c.layer_count(), 2);

        c.pop();
        assert_eq!(c.layer_count(), 1);
    }

    #[test]
    fn remove_by_id() {
        let mut c = Compositor::new(area());
        c.push(Box::new(IgnoreAll));
        c.push(Box::new(ConsumeAll)); // id = "consume_all"
        c.push(Box::new(IgnoreAll));
        assert_eq!(c.layer_count(), 3);

        c.remove("consume_all");
        assert_eq!(c.layer_count(), 2);
    }

    #[test]
    fn replace_or_push_replaces() {
        let mut c = Compositor::new(area());
        c.push(Box::new(ConsumeAll));
        assert_eq!(c.layer_count(), 1);

        // Replace existing
        c.replace_or_push("consume_all", Box::new(ConsumeAll));
        assert_eq!(c.layer_count(), 1);

        // Push new (no matching id)
        c.replace_or_push("other", Box::new(IgnoreAll));
        assert_eq!(c.layer_count(), 2);
    }

    #[test]
    fn event_consumed_stops_propagation() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        use std::sync::{Arc, Mutex};

        let reached_bottom = Arc::new(Mutex::new(false));
        let reached_clone = Arc::clone(&reached_bottom);

        // Custom component that tracks if it was reached
        struct Tracker(Arc<Mutex<bool>>);
        impl Component for Tracker {
            fn render(&mut self, _: Rect, _: &mut Buffer, _: &mut Context) {}
            #[allow(clippy::unwrap_used)]
            fn handle_event(
                &mut self,
                _: &Event,
                _: &mut Context,
            ) -> EventResult {
                *self.0.lock().unwrap() = true;
                EventResult::Ignored(None)
            }
        }

        let mut c = Compositor::new(area());
        c.push(Box::new(Tracker(reached_clone)));
        c.push(Box::new(ConsumeAll)); // this consumes

        let mut editor = test_editor();
        let mut ctx = Context { editor: &mut editor };
        let event =
            Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));

        c.handle_event(&event, &mut ctx);

        #[allow(clippy::unwrap_used)]
        let reached = *reached_bottom.lock().unwrap();
        assert!(!reached, "bottom layer should not be reached");
    }

    #[test]
    fn event_ignored_falls_through() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        use std::sync::{Arc, Mutex};

        let reached = Arc::new(Mutex::new(false));
        let reached_clone = Arc::clone(&reached);

        struct Tracker(Arc<Mutex<bool>>);
        impl Component for Tracker {
            fn render(&mut self, _: Rect, _: &mut Buffer, _: &mut Context) {}
            #[allow(clippy::unwrap_used)]
            fn handle_event(
                &mut self,
                _: &Event,
                _: &mut Context,
            ) -> EventResult {
                *self.0.lock().unwrap() = true;
                EventResult::Consumed(None)
            }
        }

        let mut c = Compositor::new(area());
        c.push(Box::new(Tracker(reached_clone)));
        c.push(Box::new(IgnoreAll)); // ignores → falls through

        let mut editor = test_editor();
        let mut ctx = Context { editor: &mut editor };
        let event =
            Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));

        c.handle_event(&event, &mut ctx);

        #[allow(clippy::unwrap_used)]
        let was_reached = *reached.lock().unwrap();
        assert!(was_reached, "bottom layer should be reached");
    }

    #[test]
    fn cursor_topmost_wins() {
        let mut c = Compositor::new(area());
        let editor = test_editor();

        // Bottom layer: cursor at (5, 3)
        c.push(Box::new(CursorAt {
            pos: Position { col: 5, row: 3 },
            kind: CursorKind::Block,
        }));
        // Top layer: cursor at (10, 7)
        c.push(Box::new(CursorAt {
            pos: Position { col: 10, row: 7 },
            kind: CursorKind::Bar,
        }));

        let (pos, kind) = c.cursor(area(), &editor);
        assert_eq!(pos, Some(Position { col: 10, row: 7 }));
        assert_eq!(kind, CursorKind::Bar);
    }

    #[test]
    fn cursor_skips_hidden_layers() {
        let mut c = Compositor::new(area());
        let editor = test_editor();

        // Bottom: has cursor
        c.push(Box::new(CursorAt {
            pos: Position { col: 5, row: 3 },
            kind: CursorKind::Block,
        }));
        // Top: no cursor (IgnoreAll returns None)
        c.push(Box::new(IgnoreAll));

        let (pos, kind) = c.cursor(area(), &editor);
        assert_eq!(pos, Some(Position { col: 5, row: 3 }));
        assert_eq!(kind, CursorKind::Block);
    }

    #[test]
    fn callback_executes_after_propagation() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        // Component that pushes a new layer via callback
        struct Pusher;
        impl Component for Pusher {
            fn render(&mut self, _: Rect, _: &mut Buffer, _: &mut Context) {}
            fn handle_event(
                &mut self,
                _: &Event,
                _: &mut Context,
            ) -> EventResult {
                EventResult::Consumed(Some(Box::new(
                    |compositor: &mut Compositor, _ctx| {
                        compositor.push(Box::new(IgnoreAll));
                    },
                )))
            }
        }

        let mut c = Compositor::new(area());
        c.push(Box::new(Pusher));
        assert_eq!(c.layer_count(), 1);

        let mut editor = test_editor();
        let mut ctx = Context { editor: &mut editor };
        let event =
            Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));

        c.handle_event(&event, &mut ctx);
        assert_eq!(c.layer_count(), 2);
    }

    #[test]
    fn find_component_by_type() {
        let mut c = Compositor::new(area());
        c.push(Box::new(IgnoreAll));
        c.push(Box::new(ConsumeAll));

        assert!(c.find::<ConsumeAll>().is_some());
        assert!(c.find::<CursorAt>().is_none());
    }
}
