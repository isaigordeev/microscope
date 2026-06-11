//! Ex command line parser (`:s///`, `:1,5d`, ...).
//!
//! Pure parsing — no editor dependency. Addresses are
//! resolved against editor state by the executor.
//!
//! Patterns use Rust regex syntax (same engine as `/`
//! search, like Helix), not vim's magic mode. `\1`
//! backreferences in the replacement are translated.

/// One side of a line range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Address {
    /// 1-based line number.
    Line(usize),
    /// `.` — cursor line.
    Current,
    /// `$` — last line.
    Last,
    /// `'x` — mark (includes `'<` / `'>`).
    Mark(char),
}

/// A line range (`1,5`, `%`, `.,$`, `'<,'>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExRange {
    pub start: Address,
    pub end: Address,
}

/// A parsed ex command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExCommand {
    Empty,
    Quit {
        force: bool,
    },
    Write {
        path: Option<String>,
    },
    WriteQuit,
    Edit {
        path: String,
        force: bool,
    },
    Substitute {
        range: Option<ExRange>,
        pattern: String,
        replacement: String,
        global: bool,
        ignore_case: bool,
    },
    DeleteLines {
        range: Option<ExRange>,
    },
    /// Bare range/line number — move the cursor there.
    Goto(Address),
    /// `:set number` family. `Some(on)` or `None` for
    /// toggle (`number!`).
    SetNumber(Option<bool>),
    Theme(Option<String>),
    /// `:config-reload` — re-read config files.
    ConfigReload,
    /// `:config-open` — edit the global config file.
    ConfigOpen,
    Unknown(String),
}

/// Parse one ex command line (without the leading `:`).
#[must_use]
pub fn parse(input: &str) -> ExCommand {
    let input = input.trim();
    if input.is_empty() {
        return ExCommand::Empty;
    }

    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let range = parse_range(&chars, &mut i);

    while i < chars.len() && chars[i] == ' ' {
        i += 1;
    }
    if i >= chars.len() {
        // Bare range: go to its end address.
        return range.map_or(ExCommand::Empty, |r| ExCommand::Goto(r.end));
    }

    // Command word: alphabetic run (plus `-` for
    // helix-style commands like config-reload).
    let word_start = i;
    while i < chars.len()
        && (chars[i].is_ascii_alphabetic() || chars[i] == '-')
    {
        i += 1;
    }
    let word: String = chars[word_start..i].iter().collect();
    let force = i < chars.len() && chars[i] == '!';
    if force {
        i += 1;
    }
    let rest: String = chars[i..].iter().collect();
    let arg = rest.trim();

    match word.as_str() {
        "s" | "su" | "sub" | "substitute" => parse_substitute(range, &rest),
        "d" | "de" | "del" | "delete" => ExCommand::DeleteLines { range },
        "q" | "quit" => ExCommand::Quit { force },
        "w" | "write" => ExCommand::Write {
            path: (!arg.is_empty()).then(|| arg.to_owned()),
        },
        "wq" | "x" => ExCommand::WriteQuit,
        "e" | "ed" | "edit" => {
            if arg.is_empty() {
                ExCommand::Unknown(input.to_owned())
            } else {
                ExCommand::Edit { path: arg.to_owned(), force }
            }
        }
        "set" | "se" => parse_set(arg, input),
        "theme" => ExCommand::Theme((!arg.is_empty()).then(|| arg.to_owned())),
        "config-reload" => ExCommand::ConfigReload,
        "config-open" => ExCommand::ConfigOpen,
        _ => ExCommand::Unknown(input.to_owned()),
    }
}

fn parse_set(arg: &str, input: &str) -> ExCommand {
    match arg {
        "number" | "nu" => ExCommand::SetNumber(Some(true)),
        "nonumber" | "nonu" => ExCommand::SetNumber(Some(false)),
        "number!" | "nu!" => ExCommand::SetNumber(None),
        _ => ExCommand::Unknown(input.to_owned()),
    }
}

// ── Range parsing ─────────────────────────────────

fn parse_range(chars: &[char], i: &mut usize) -> Option<ExRange> {
    if *i < chars.len() && chars[*i] == '%' {
        *i += 1;
        return Some(ExRange { start: Address::Line(1), end: Address::Last });
    }
    let start = parse_address(chars, i)?;
    if *i < chars.len() && chars[*i] == ',' {
        *i += 1;
        let end = parse_address(chars, i).unwrap_or(start);
        Some(ExRange { start, end })
    } else {
        Some(ExRange { start, end: start })
    }
}

fn parse_address(chars: &[char], i: &mut usize) -> Option<Address> {
    match chars.get(*i)? {
        '.' => {
            *i += 1;
            Some(Address::Current)
        }
        '$' => {
            *i += 1;
            Some(Address::Last)
        }
        '\'' => {
            let mark = *chars.get(*i + 1)?;
            *i += 2;
            Some(Address::Mark(mark))
        }
        c if c.is_ascii_digit() => {
            let mut n = 0usize;
            while let Some(d) = chars.get(*i).and_then(|c| c.to_digit(10)) {
                n = n.saturating_mul(10).saturating_add(d as usize);
                *i += 1;
            }
            Some(Address::Line(n))
        }
        _ => None,
    }
}

// ── Substitute parsing ────────────────────────────

/// Parse `s/pat/rep/flags` with any non-alphanumeric
/// separator. A backslash escapes the separator inside
/// pattern/replacement.
fn parse_substitute(range: Option<ExRange>, rest: &str) -> ExCommand {
    let mut chars = rest.chars();
    let Some(sep) = chars.next().filter(|c| !c.is_ascii_alphanumeric()) else {
        return ExCommand::Unknown(format!("s{rest}"));
    };

    let mut parts: Vec<String> = vec![String::new()];
    let mut escaped = false;
    for c in chars {
        if escaped {
            if c != sep {
                // Keep the backslash: it belongs to the
                // regex / backref syntax.
                if let Some(last) = parts.last_mut() {
                    last.push('\\');
                }
            }
            if let Some(last) = parts.last_mut() {
                last.push(c);
            }
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == sep {
            parts.push(String::new());
        } else if let Some(last) = parts.last_mut() {
            last.push(c);
        }
    }

    let pattern = parts.first().cloned().unwrap_or_default();
    let replacement = parts.get(1).cloned().unwrap_or_default();
    let flags = parts.get(2).cloned().unwrap_or_default();
    if pattern.is_empty() {
        return ExCommand::Unknown(format!("s{rest}"));
    }
    ExCommand::Substitute {
        range,
        pattern,
        replacement,
        global: flags.contains('g'),
        ignore_case: flags.contains('i'),
    }
}

/// Translate vim-style `\1`..`\9` backreferences to
/// the regex crate's `${1}` syntax. `$N` passes
/// through natively.
#[must_use]
pub fn translate_backrefs(replacement: &str) -> String {
    let mut out = String::with_capacity(replacement.len());
    let mut chars = replacement.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek() {
                Some(&d) if d.is_ascii_digit() => {
                    out.push_str("${");
                    out.push(d);
                    out.push('}');
                    chars.next();
                }
                Some(&n) => {
                    out.push(n);
                    chars.next();
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

// ── Tests ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty() {
        assert_eq!(parse(""), ExCommand::Empty);
        assert_eq!(parse("   "), ExCommand::Empty);
    }

    #[test]
    fn quit_variants() {
        assert_eq!(parse("q"), ExCommand::Quit { force: false });
        assert_eq!(parse("q!"), ExCommand::Quit { force: true });
        assert_eq!(parse("quit"), ExCommand::Quit { force: false });
    }

    #[test]
    fn write_with_path() {
        assert_eq!(parse("w"), ExCommand::Write { path: None });
        assert_eq!(
            parse("w foo.txt"),
            ExCommand::Write { path: Some("foo.txt".to_owned()) },
        );
    }

    #[test]
    fn edit_file() {
        assert_eq!(
            parse("e src/main.rs"),
            ExCommand::Edit { path: "src/main.rs".to_owned(), force: false },
        );
        assert_eq!(
            parse("e! foo"),
            ExCommand::Edit { path: "foo".to_owned(), force: true },
        );
    }

    #[test]
    fn substitute_simple() {
        assert_eq!(
            parse("s/foo/bar/"),
            ExCommand::Substitute {
                range: None,
                pattern: "foo".to_owned(),
                replacement: "bar".to_owned(),
                global: false,
                ignore_case: false,
            },
        );
    }

    #[test]
    fn substitute_percent_global() {
        assert_eq!(
            parse("%s/foo/bar/g"),
            ExCommand::Substitute {
                range: Some(ExRange {
                    start: Address::Line(1),
                    end: Address::Last,
                }),
                pattern: "foo".to_owned(),
                replacement: "bar".to_owned(),
                global: true,
                ignore_case: false,
            },
        );
    }

    #[test]
    fn substitute_without_trailing_sep() {
        assert_eq!(
            parse("s/foo"),
            ExCommand::Substitute {
                range: None,
                pattern: "foo".to_owned(),
                replacement: String::new(),
                global: false,
                ignore_case: false,
            },
        );
    }

    #[test]
    fn substitute_escaped_separator() {
        let ExCommand::Substitute { pattern, replacement, .. } =
            parse(r"s/a\/b/x\/y/")
        else {
            panic!("not a substitute");
        };
        assert_eq!(pattern, "a/b");
        assert_eq!(replacement, "x/y");
    }

    #[test]
    fn substitute_alt_separator() {
        let ExCommand::Substitute { pattern, replacement, .. } =
            parse("s#a/b#c#")
        else {
            panic!("not a substitute");
        };
        assert_eq!(pattern, "a/b");
        assert_eq!(replacement, "c");
    }

    #[test]
    fn substitute_range() {
        assert_eq!(
            parse("1,5s/a/b/"),
            ExCommand::Substitute {
                range: Some(ExRange {
                    start: Address::Line(1),
                    end: Address::Line(5),
                }),
                pattern: "a".to_owned(),
                replacement: "b".to_owned(),
                global: false,
                ignore_case: false,
            },
        );
    }

    #[test]
    fn visual_marks_range() {
        assert_eq!(
            parse("'<,'>d"),
            ExCommand::DeleteLines {
                range: Some(ExRange {
                    start: Address::Mark('<'),
                    end: Address::Mark('>'),
                }),
            },
        );
    }

    #[test]
    fn dot_dollar_range() {
        assert_eq!(
            parse(".,$d"),
            ExCommand::DeleteLines {
                range: Some(ExRange {
                    start: Address::Current,
                    end: Address::Last,
                }),
            },
        );
    }

    #[test]
    fn delete_lines() {
        assert_eq!(
            parse("1,2d"),
            ExCommand::DeleteLines {
                range: Some(ExRange {
                    start: Address::Line(1),
                    end: Address::Line(2),
                }),
            },
        );
    }

    #[test]
    fn bare_number_is_goto() {
        assert_eq!(parse("42"), ExCommand::Goto(Address::Line(42)));
    }

    #[test]
    fn bare_dollar_is_goto_last() {
        assert_eq!(parse("$"), ExCommand::Goto(Address::Last));
    }

    #[test]
    fn set_number_variants() {
        assert_eq!(parse("set number"), ExCommand::SetNumber(Some(true)));
        assert_eq!(parse("set nonu"), ExCommand::SetNumber(Some(false)));
        assert_eq!(parse("set number!"), ExCommand::SetNumber(None));
    }

    #[test]
    fn config_commands() {
        assert_eq!(parse("config-reload"), ExCommand::ConfigReload);
        assert_eq!(parse("config-open"), ExCommand::ConfigOpen);
    }

    #[test]
    fn unknown_command() {
        assert_eq!(
            parse("frobnicate"),
            ExCommand::Unknown("frobnicate".to_owned()),
        );
    }

    #[test]
    fn backref_translation() {
        assert_eq!(translate_backrefs(r"\2 \1"), "${2} ${1}");
        assert_eq!(translate_backrefs("$1"), "$1");
        assert_eq!(translate_backrefs(r"a\\b"), r"a\b");
    }
}
