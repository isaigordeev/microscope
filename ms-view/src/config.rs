//! Editor configuration (Helix-style TOML).
//!
//! Two layers merge over built-in defaults:
//! `~/.config/microscope/config.toml` (global), then a
//! project-local `.microscope.toml`. Files are merged
//! at the TOML value level before typed
//! deserialization, so a partial local config
//! overrides individual keys only.
//!
//! File IO and path discovery live in ms-term; this
//! module is pure parsing.

use serde::Deserialize;

/// Editor settings (`config.toml` / `.microscope.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    /// Color theme name (`vs_dark`, `vs_light`).
    pub theme: String,
    /// Line numbers in the gutter (`:set number`).
    pub number: bool,
    /// Lines kept visible around the cursor when
    /// scrolling.
    pub scrolloff: usize,
    /// Spaces inserted by `>>` / removed by `<<`.
    pub indent_width: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "vs_dark".to_owned(),
            number: true,
            scrolloff: 4,
            indent_width: 4,
        }
    }
}

/// Parse a single config file.
///
/// # Errors
/// Returns the TOML error message for invalid syntax
/// or unknown keys.
pub fn parse(source: &str) -> Result<Config, String> {
    toml::from_str(source).map_err(|e| e.to_string())
}

/// Merge global and local sources over the defaults.
/// Either layer may be absent. Local keys win.
///
/// # Errors
/// Returns the first TOML error message encountered.
pub fn load_merged(
    global: Option<&str>,
    local: Option<&str>,
) -> Result<Config, String> {
    let parse_value =
        |src: &str| src.parse::<toml::Value>().map_err(|e| e.to_string());
    let merged = match (global, local) {
        (None, None) => return Ok(Config::default()),
        (Some(one), None) | (None, Some(one)) => parse_value(one)?,
        (Some(global), Some(local)) => {
            merge_values(parse_value(global)?, parse_value(local)?)
        }
    };
    merged.try_into().map_err(|e: toml::de::Error| e.to_string())
}

/// Merge two TOML values: `over` wins; tables merge
/// recursively (Helix's `merge_toml_values`).
fn merge_values(base: toml::Value, over: toml::Value) -> toml::Value {
    match (base, over) {
        (toml::Value::Table(mut base), toml::Value::Table(over)) => {
            for (key, over_value) in over {
                let merged = match base.remove(&key) {
                    Some(base_value) => merge_values(base_value, over_value),
                    None => over_value,
                };
                base.insert(key, merged);
            }
            toml::Value::Table(base)
        }
        (_, over) => over,
    }
}

// ── Tests ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let c = Config::default();
        assert_eq!(c.theme, "vs_dark");
        assert!(c.number);
        assert_eq!(c.scrolloff, 4);
        assert_eq!(c.indent_width, 4);
    }

    #[test]
    fn empty_source_is_defaults() {
        assert_eq!(parse(""), Ok(Config::default()));
    }

    #[test]
    fn partial_config_keeps_other_defaults() {
        let c = parse("theme = \"vs_light\"").unwrap_or_default();
        assert_eq!(c.theme, "vs_light");
        assert_eq!(c.scrolloff, 4);
    }

    #[test]
    fn kebab_case_keys() {
        let c = parse("indent-width = 2").unwrap_or_default();
        assert_eq!(c.indent_width, 2);
    }

    #[test]
    fn unknown_key_is_error() {
        assert!(parse("frobnicate = true").is_err());
    }

    #[test]
    fn invalid_toml_is_error() {
        assert!(parse("theme = ").is_err());
    }

    #[test]
    fn local_overrides_global_per_key() {
        let global = "theme = \"vs_light\"\nscrolloff = 8";
        let local = "scrolloff = 0";
        let c = load_merged(Some(global), Some(local)).unwrap_or_default();
        // local wins where set...
        assert_eq!(c.scrolloff, 0);
        // ...global survives where local is silent...
        assert_eq!(c.theme, "vs_light");
        // ...defaults fill the rest.
        assert!(c.number);
    }

    #[test]
    fn missing_layers_are_fine() {
        assert_eq!(load_merged(None, None), Ok(Config::default()));
        let c = load_merged(None, Some("number = false")).unwrap_or_default();
        assert!(!c.number);
    }
}
