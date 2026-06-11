//! File tree sidebar (nvim-tree replacement).
//!
//! Pushed as a compositor layer over the editor; it
//! draws a left sidebar and consumes keys while open.
//! Bindings match the user's nvim-tree setup:
//! j/k move, Enter/l expand or open, h collapse or go
//! to parent, `C` root to node, `-` root to parent,
//! Ctrl-o back to the previous root, q/Esc/Ctrl-n
//! close.

use std::path::PathBuf;

use crossterm::event::{Event, KeyCode, KeyModifiers};

use ms_tree::Tree;
use ms_tui::buffer::{Buffer, Rect};
use ms_view::document::Document;

use crate::compositor::{Callback, Component, Context, EventResult};

const MAX_WIDTH: u16 = 40;

/// Sidebar file explorer component.
#[derive(Debug)]
pub struct FileTree {
    tree: Tree,
    selected: usize,
    scroll: usize,
    /// Previous roots for Ctrl-o back-navigation.
    root_history: Vec<PathBuf>,
}

impl FileTree {
    /// Open a tree rooted at the working directory.
    pub fn new() -> std::io::Result<Self> {
        let root = std::env::current_dir()?;
        let tree = Tree::new(&root)?;
        Ok(Self { tree, selected: 0, scroll: 0, root_history: Vec::new() })
    }

    fn width(area: Rect) -> u16 {
        MAX_WIDTH.min(area.width / 2)
    }

    fn move_selection(&mut self, delta: isize) {
        let len = self.tree.nodes().len();
        if len == 0 {
            return;
        }
        let max = len - 1;
        let new = self.selected.saturating_add_signed(delta).min(max);
        self.selected = new;
    }

    /// Enter/l: expand a directory or open a file.
    fn activate(&mut self, ctx: &mut Context) -> EventResult {
        let Some(node) = self.tree.nodes().get(self.selected) else {
            return EventResult::Consumed(None);
        };
        if node.is_dir {
            if let Err(e) = self.tree.toggle(self.selected) {
                ctx.editor.status_message = Some(format!("Error: {e}"));
            }
            return EventResult::Consumed(None);
        }
        match Document::open(&node.path) {
            Ok(doc) => {
                ctx.editor.open_document(doc);
                ctx.editor.status_message = None;
                Self::close()
            }
            Err(e) => {
                ctx.editor.status_message = Some(format!("Error: {e}"));
                EventResult::Consumed(None)
            }
        }
    }

    /// h: collapse the directory, or jump to parent.
    fn collapse_or_parent(&mut self) {
        let Some(node) = self.tree.nodes().get(self.selected) else {
            return;
        };
        if node.is_dir && node.expanded {
            self.tree.collapse(self.selected);
        } else if let Some(parent) = self.tree.parent_of(self.selected) {
            self.selected = parent;
        }
    }

    /// C: re-root at the selected directory
    /// (remembering the old root).
    fn root_to_node(&mut self, ctx: &mut Context) {
        let Some(node) = self.tree.nodes().get(self.selected) else {
            return;
        };
        if !node.is_dir {
            return;
        }
        let target = node.path.clone();
        self.change_root(ctx, &target, true);
    }

    /// -: re-root at the parent of the current root.
    fn root_to_parent(&mut self, ctx: &mut Context) {
        let Some(parent) = self.tree.root().parent().map(PathBuf::from) else {
            return;
        };
        self.change_root(ctx, &parent, true);
    }

    /// Ctrl-o: return to the previous root.
    fn root_back(&mut self, ctx: &mut Context) {
        let Some(previous) = self.root_history.pop() else {
            ctx.editor.status_message =
                Some("tree: no previous root".to_owned());
            return;
        };
        self.change_root(ctx, &previous, false);
    }

    fn change_root(
        &mut self,
        ctx: &mut Context,
        root: &std::path::Path,
        push: bool,
    ) {
        let old = self.tree.root().to_path_buf();
        match self.tree.set_root(root) {
            Ok(()) => {
                if push {
                    self.root_history.push(old);
                }
                self.selected = 0;
                self.scroll = 0;
            }
            Err(e) => {
                ctx.editor.status_message = Some(format!("Error: {e}"));
            }
        }
    }

    fn close() -> EventResult {
        let cb: Callback = Box::new(|compositor, _ctx| {
            compositor.remove("filetree");
        });
        EventResult::Consumed(Some(cb))
    }
}

impl Component for FileTree {
    fn handle_event(
        &mut self,
        event: &Event,
        ctx: &mut Context,
    ) -> EventResult {
        let Event::Key(key) = event else {
            return EventResult::Ignored(None);
        };
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        match key.code {
            KeyCode::Char('n') if ctrl => Self::close(),
            KeyCode::Char('o') if ctrl => {
                self.root_back(ctx);
                EventResult::Consumed(None)
            }
            KeyCode::Char(']') if ctrl => {
                self.root_to_node(ctx);
                EventResult::Consumed(None)
            }
            KeyCode::Esc | KeyCode::Char('q') => Self::close(),
            KeyCode::Char('j') | KeyCode::Down => {
                self.move_selection(1);
                EventResult::Consumed(None)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.move_selection(-1);
                EventResult::Consumed(None)
            }
            KeyCode::Char('g') => {
                self.selected = 0;
                EventResult::Consumed(None)
            }
            KeyCode::Char('G') => {
                self.selected = self.tree.nodes().len().saturating_sub(1);
                EventResult::Consumed(None)
            }
            KeyCode::Enter | KeyCode::Char('l') => self.activate(ctx),
            KeyCode::Char('h') => {
                self.collapse_or_parent();
                EventResult::Consumed(None)
            }
            KeyCode::Char('C') => {
                self.root_to_node(ctx);
                EventResult::Consumed(None)
            }
            KeyCode::Char('-') => {
                self.root_to_parent(ctx);
                EventResult::Consumed(None)
            }
            _ => EventResult::Consumed(None),
        }
    }

    fn render(&mut self, area: Rect, surface: &mut Buffer, ctx: &mut Context) {
        let theme = &ctx.editor.theme;
        let bg = theme.resolve("ui.popup");
        let dir_style = bg.merge(theme.resolve("ui.directory"));
        let file_style = bg.merge(theme.resolve("ui.text"));
        let selected_style = theme.resolve("ui.picker.selected");
        let sep_style = theme.resolve("ui.separator");
        let header_style = bg.merge(theme.resolve("ui.linenr.selected"));

        let width = Self::width(area);
        let height = area.height.saturating_sub(1);
        let rows = height.saturating_sub(1) as usize;

        // Keep the selection in view.
        if self.selected < self.scroll {
            self.scroll = self.selected;
        }
        if rows > 0 && self.selected >= self.scroll + rows {
            self.scroll = self.selected + 1 - rows;
        }

        // Background panel + separator.
        for y in 0..height {
            surface.put_str(0, y, &" ".repeat(width as usize), bg);
            surface.put_str(width, y, "│", sep_style);
        }

        // Header: root path, $HOME collapsed to ~.
        let root = self.tree.root().display().to_string();
        let root = std::env::var("HOME").map_or_else(
            |_| root.clone(),
            |home| root.replacen(&home, "~", 1),
        );
        let header: String = root.chars().take(width as usize).collect();
        surface.put_str(0, 0, &header, header_style);

        // Rows.
        for (row, node) in
            self.tree.nodes().iter().enumerate().skip(self.scroll).take(rows)
        {
            let y = (row - self.scroll + 1) as u16;
            let marker = if node.is_dir {
                if node.expanded {
                    "▾ "
                } else {
                    "▸ "
                }
            } else {
                "  "
            };
            let text =
                format!("{}{marker}{}", "  ".repeat(node.depth), node.name(),);
            let text: String = text.chars().take(width as usize).collect();
            let style = if node.is_dir { dir_style } else { file_style };
            let style = if row == self.selected {
                style.merge(selected_style)
            } else {
                style
            };
            if row == self.selected {
                surface.put_str(0, y, &" ".repeat(width as usize), style);
            }
            surface.put_str(0, y, &text, style);
        }
    }

    fn id(&self) -> Option<&'static str> {
        Some("filetree")
    }
}
