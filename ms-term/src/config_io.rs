//! Config file discovery and loading.
//!
//! Global: `$XDG_CONFIG_HOME/microscope/config.toml`
//! (or `~/.config/microscope/config.toml`).
//! Project-local: `.microscope.toml`, found by walking
//! up from the working directory. Local keys override
//! global ones (merged in `ms_view::config`).

use std::path::PathBuf;

use ms_view::config::{self, Config};
use ms_view::editor::Editor;
use ms_view::theme::builtin_theme;

/// Path of the global config file (may not exist).
pub fn global_config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join(".config"))
        })?;
    Some(base.join("microscope").join("config.toml"))
}

/// Find `.microscope.toml` walking up from the
/// working directory.
pub fn local_config_path() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let candidate = dir.join(".microscope.toml");
        if candidate.is_file() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Load and merge both config layers. On error the
/// defaults are returned together with a message.
pub fn load() -> (Config, Option<String>) {
    let global =
        global_config_path().and_then(|p| std::fs::read_to_string(p).ok());
    let local =
        local_config_path().and_then(|p| std::fs::read_to_string(p).ok());
    match config::load_merged(global.as_deref(), local.as_deref()) {
        Ok(config) => (config, None),
        Err(e) => (Config::default(), Some(format!("Config error: {e}"))),
    }
}

/// Apply a config to a running editor.
pub fn apply(editor: &mut Editor, config: Config) {
    if editor.theme.name != config.theme {
        if let Some(theme) = builtin_theme(&config.theme) {
            editor.theme = theme;
        } else {
            editor.status_message =
                Some(format!("Unknown theme: {}", config.theme));
        }
    }
    editor.view.scrolloff = config.scrolloff;
    editor.config = config;
}

/// Load both layers and apply (startup and
/// `:config-reload`).
pub fn load_and_apply(editor: &mut Editor) -> Option<String> {
    let (config, error) = load();
    apply(editor, config);
    error
}
