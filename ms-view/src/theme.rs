use std::collections::HashMap;

use ms_tui::style::{Color, Modifier, Style};

/// A named theme mapping scope strings to styles.
///
/// Scopes use dot-separated hierarchies (like TextMate/Helix):
/// `"keyword.control"` falls back to `"keyword"` if not found.
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    scopes: HashMap<String, Style>,
}

impl Theme {
    /// Create a theme from a name and scope map.
    pub fn new(
        name: impl Into<String>,
        scopes: HashMap<String, Style>,
    ) -> Self {
        Self { name: name.into(), scopes }
    }

    /// Resolve a scope to a style, walking the dot-fallback
    /// chain. E.g. `"keyword.control"` → `"keyword"` → default.
    pub fn resolve(&self, scope: &str) -> Style {
        let mut key = scope;
        loop {
            if let Some(style) = self.scopes.get(key) {
                return *style;
            }
            // Walk up: "a.b.c" → "a.b" → "a"
            match key.rfind('.') {
                Some(pos) => key = &key[..pos],
                None => return Style::default(),
            }
        }
    }

    /// Get a style by exact scope (no fallback).
    pub fn get(&self, scope: &str) -> Option<&Style> {
        self.scopes.get(scope)
    }

    /// The default fallback theme (VS Code dark).
    pub fn fallback() -> Self {
        vs_light()
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::fallback()
    }
}

// ── Helper ──────────────────────────────────────────

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

fn style_fg(color: Color) -> Style {
    Style::default().fg(color)
}

fn style_fg_bg(fg: Color, bg: Color) -> Style {
    Style::default().fg(fg).bg(bg)
}

const fn style_fg_mod(fg: Color, modifier: Modifier) -> Style {
    Style { fg: Some(fg), bg: None, modifier, underline_color: None }
}

// ── Built-in themes ─────────────────────────────────

/// Get a built-in theme by name.
pub fn builtin_theme(name: &str) -> Option<Theme> {
    match name {
        "vs_dark" | "dark" => Some(vs_dark()),
        "vs_light" | "light" => Some(vs_light()),
        _ => None,
    }
}

/// VS Code Dark theme — ported from user's nvim `vs_dark.vim`.
pub fn vs_dark() -> Theme {
    let mut s = HashMap::new();

    // ── UI ──
    s.insert(
        "ui.background".into(),
        style_fg_bg(rgb(0xD4, 0xD4, 0xD4), rgb(0x2F, 0x2F, 0x2F)),
    );
    s.insert("ui.text".into(), style_fg(rgb(0xD4, 0xD4, 0xD4)));
    s.insert("ui.linenr".into(), style_fg(rgb(0x6B, 0x6B, 0x6B)));
    s.insert("ui.linenr.selected".into(), style_fg(rgb(0xFF, 0xFF, 0xFF)));
    s.insert(
        "ui.cursorline".into(),
        Style::default().bg(rgb(0x2A, 0x2A, 0x2A)),
    );
    s.insert(
        "ui.statusline".into(),
        style_fg_bg(rgb(0xD4, 0xD4, 0xD4), rgb(0x2F, 0x2F, 0x2F)),
    );
    s.insert(
        "ui.statusline.inactive".into(),
        style_fg_bg(rgb(0x6B, 0x6B, 0x6B), rgb(0x2F, 0x2F, 0x2F)),
    );
    s.insert(
        "ui.selection".into(),
        Style::default().bg(rgb(0x3A, 0x3D, 0x41)),
    );
    s.insert(
        "ui.popup".into(),
        style_fg_bg(rgb(0xCC, 0xCC, 0xCC), rgb(0x25, 0x25, 0x26)),
    );
    s.insert(
        "ui.popup.selected".into(),
        style_fg_bg(rgb(0xFF, 0xFF, 0xFF), rgb(0x00, 0x78, 0xD4)),
    );
    s.insert("ui.separator".into(), style_fg(rgb(0x45, 0x45, 0x45)));
    s.insert("ui.virtual".into(), style_fg(rgb(0x2F, 0x2F, 0x2F)));
    s.insert("ui.match".into(), Style::default().bg(rgb(0x26, 0x4F, 0x78)));

    // ── Syntax ──
    s.insert(
        "comment".into(),
        style_fg_mod(
            rgb(0x82, 0x82, 0x82),
            Modifier { italic: true, ..Modifier::default() },
        ),
    );
    s.insert("keyword".into(), style_fg(rgb(0x56, 0x9C, 0xD6)));
    s.insert("operator".into(), style_fg(rgb(0xD4, 0xD4, 0xD4)));
    s.insert("function".into(), style_fg(rgb(0xD4, 0xD4, 0xD4)));
    s.insert("type".into(), style_fg(rgb(0x56, 0x9C, 0xD6)));
    s.insert("variable".into(), style_fg(rgb(0xD4, 0xD4, 0xD4)));
    s.insert("variable.builtin".into(), style_fg(rgb(0x56, 0x9C, 0xD6)));
    s.insert("constant".into(), style_fg(rgb(0x56, 0x9C, 0xD6)));
    s.insert("number".into(), style_fg(rgb(0xB5, 0xCE, 0xA8)));
    s.insert("string".into(), style_fg(rgb(0xCE, 0x91, 0x78)));
    s.insert("string.special".into(), style_fg(rgb(0xD1, 0x69, 0x69)));
    s.insert("tag".into(), style_fg(rgb(0x56, 0x9C, 0xD6)));
    s.insert("label".into(), style_fg(rgb(0x56, 0x9C, 0xD6)));
    s.insert("special".into(), style_fg(rgb(0x56, 0x9C, 0xD6)));
    s.insert("punctuation".into(), style_fg(rgb(0x80, 0x80, 0x80)));

    // ── Diagnostic ──
    s.insert("diagnostic.error".into(), style_fg(rgb(0xF4, 0x47, 0x47)));
    s.insert("diagnostic.warning".into(), style_fg(rgb(0xCC, 0xA7, 0x00)));
    s.insert("diagnostic.info".into(), style_fg(rgb(0xCC, 0xA7, 0x00)));
    s.insert("diagnostic.hint".into(), style_fg(rgb(0xCC, 0xA7, 0x00)));

    // ── Diff ──
    s.insert("diff.plus".into(), style_fg(rgb(0xB5, 0xCE, 0xA8)));
    s.insert("diff.minus".into(), style_fg(rgb(0xCE, 0x91, 0x78)));
    s.insert("diff.delta".into(), style_fg(rgb(0x56, 0x9C, 0xD6)));

    // ── Error / TODO ──
    s.insert(
        "error".into(),
        style_fg_mod(
            rgb(0xF4, 0x47, 0x47),
            Modifier { bold: true, ..Modifier::default() },
        ),
    );
    s.insert(
        "hint".into(),
        style_fg_mod(
            rgb(0xB5, 0xCE, 0xA8),
            Modifier { bold: true, ..Modifier::default() },
        ),
    );

    Theme::new("vs_dark", s)
}

/// VS Code Light theme — ported from user's nvim `vs_light.vim`.
pub fn vs_light() -> Theme {
    let mut s = HashMap::new();

    // ── UI ──
    s.insert(
        "ui.background".into(),
        style_fg_bg(rgb(0x00, 0x00, 0x00), rgb(0xF2, 0xF2, 0xF2)),
    );
    s.insert("ui.text".into(), style_fg(rgb(0x00, 0x00, 0x00)));
    s.insert("ui.linenr".into(), style_fg(rgb(0x6F, 0x6F, 0x6F)));
    s.insert("ui.linenr.selected".into(), style_fg(rgb(0x00, 0x00, 0x00)));
    s.insert(
        "ui.cursorline".into(),
        Style::default().bg(rgb(0xE5, 0xEB, 0xF1)),
    );
    s.insert(
        "ui.statusline".into(),
        style_fg_bg(rgb(0x00, 0x00, 0x00), rgb(0xF2, 0xF2, 0xF2)),
    );
    s.insert(
        "ui.statusline.inactive".into(),
        style_fg_bg(rgb(0xAA, 0xAA, 0xAA), rgb(0xF2, 0xF2, 0xF2)),
    );
    s.insert(
        "ui.selection".into(),
        Style::default().bg(rgb(0xAD, 0xD6, 0xFF)),
    );
    s.insert(
        "ui.popup".into(),
        style_fg_bg(rgb(0x00, 0x00, 0x00), rgb(0xF3, 0xF3, 0xF3)),
    );
    s.insert(
        "ui.popup.selected".into(),
        style_fg_bg(rgb(0xFF, 0xFF, 0xFF), rgb(0x00, 0x7A, 0xCC)),
    );
    s.insert("ui.separator".into(), style_fg(rgb(0xD4, 0xD4, 0xD4)));
    s.insert("ui.virtual".into(), style_fg(rgb(0xF2, 0xF2, 0xF2)));
    s.insert("ui.match".into(), Style::default().bg(rgb(0x90, 0xC2, 0xF9)));

    // ── Syntax ──
    s.insert(
        "comment".into(),
        style_fg_mod(
            rgb(0x82, 0x82, 0x82),
            Modifier { italic: true, ..Modifier::default() },
        ),
    );
    s.insert("keyword".into(), style_fg(rgb(0x00, 0x00, 0xFF)));
    s.insert("operator".into(), style_fg(rgb(0x00, 0x00, 0x00)));
    s.insert("function".into(), style_fg(rgb(0x04, 0x51, 0xA5)));
    s.insert("type".into(), style_fg(rgb(0x04, 0x51, 0xA5)));
    s.insert("variable".into(), style_fg(rgb(0x00, 0x00, 0x00)));
    s.insert("constant".into(), style_fg(rgb(0x00, 0x00, 0xFF)));
    s.insert("number".into(), style_fg(rgb(0x09, 0x86, 0x58)));
    s.insert("string".into(), style_fg(rgb(0xA3, 0x15, 0x15)));
    s.insert("string.special".into(), style_fg(rgb(0x81, 0x1F, 0x3F)));
    s.insert("tag".into(), style_fg(rgb(0x80, 0x00, 0x00)));
    s.insert("label".into(), style_fg(rgb(0x00, 0x00, 0xFF)));
    s.insert("special".into(), style_fg(rgb(0x80, 0x00, 0x00)));
    s.insert("punctuation".into(), style_fg(rgb(0x00, 0x00, 0x00)));

    // ── Diagnostic ──
    s.insert("diagnostic.error".into(), style_fg(rgb(0xFF, 0x00, 0x00)));
    s.insert("diagnostic.warning".into(), style_fg(rgb(0xBF, 0x88, 0x03)));
    s.insert("diagnostic.info".into(), style_fg(rgb(0x04, 0x51, 0xA5)));
    s.insert("diagnostic.hint".into(), style_fg(rgb(0x09, 0x86, 0x58)));

    // ── Diff ──
    s.insert("diff.plus".into(), style_fg(rgb(0x09, 0x86, 0x58)));
    s.insert("diff.minus".into(), style_fg(rgb(0xA3, 0x15, 0x15)));
    s.insert("diff.delta".into(), style_fg(rgb(0x04, 0x51, 0xA5)));

    // ── Error / TODO ──
    s.insert(
        "error".into(),
        style_fg_mod(
            rgb(0xFF, 0x00, 0x00),
            Modifier { bold: true, ..Modifier::default() },
        ),
    );
    s.insert(
        "hint".into(),
        style_fg_mod(
            rgb(0xA3, 0x15, 0x15),
            Modifier { bold: true, ..Modifier::default() },
        ),
    );

    Theme::new("vs_light", s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_exact_scope() {
        let theme = vs_dark();
        let style = theme.resolve("keyword");
        assert_eq!(style.fg, Some(rgb(0x56, 0x9C, 0xD6)));
    }

    #[test]
    fn resolve_dot_fallback() {
        let theme = vs_dark();
        // "keyword.control" not defined, falls back to "keyword"
        let style = theme.resolve("keyword.control");
        assert_eq!(style.fg, Some(rgb(0x56, 0x9C, 0xD6)));
    }

    #[test]
    fn resolve_missing_returns_default() {
        let theme = vs_dark();
        let style = theme.resolve("nonexistent");
        assert_eq!(style, Style::default());
    }

    #[test]
    fn resolve_deep_fallback() {
        let theme = vs_dark();
        // "variable.builtin" is defined, should use it
        let style = theme.resolve("variable.builtin");
        assert_eq!(style.fg, Some(rgb(0x56, 0x9C, 0xD6)));
        // "variable.parameter" not defined, falls to "variable"
        let var = theme.resolve("variable.parameter");
        assert_eq!(var.fg, Some(rgb(0xD4, 0xD4, 0xD4)));
    }

    #[test]
    fn vs_light_has_distinct_colors() {
        let dark = vs_dark();
        let light = vs_light();
        // Keywords are different between themes
        assert_ne!(dark.resolve("keyword").fg, light.resolve("keyword").fg,);
    }

    #[test]
    fn builtin_theme_lookup() {
        assert!(builtin_theme("vs_dark").is_some());
        assert!(builtin_theme("dark").is_some());
        assert!(builtin_theme("vs_light").is_some());
        assert!(builtin_theme("light").is_some());
        assert!(builtin_theme("monokai").is_none());
    }

    #[test]
    fn comment_is_italic() {
        let theme = vs_dark();
        let style = theme.resolve("comment");
        assert!(style.modifier.italic);
    }

    #[test]
    fn ui_scopes_have_bg() {
        let theme = vs_dark();
        let bg = theme.resolve("ui.background");
        assert!(bg.bg.is_some());
    }
}
