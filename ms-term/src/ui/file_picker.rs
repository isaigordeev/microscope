use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use ms_view::document::Document;

use crate::compositor::Context;

use super::picker::Picker;

/// Build a file picker for the given workspace root.
pub fn file_picker(root: &Path) -> Picker<PathBuf> {
    let files = collect_files(root);
    let root_owned = root.to_path_buf();
    let prompt = prompt_prefix(root);

    Picker::new(
        Box::new(move |path: &PathBuf| {
            path.strip_prefix(&root_owned)
                .unwrap_or(path)
                .display()
                .to_string()
        }),
        Box::new(|ctx: &mut Context, path: &PathBuf| {
            open_file(ctx, path);
        }),
        files,
    )
    .with_preview(Box::new(|path: &PathBuf| Some(path.clone())))
    .with_prompt_prefix(prompt)
}

/// Build the fzf-style prompt prefix from the workspace root,
/// collapsing `$HOME` to `~` and ensuring a trailing slash.
fn prompt_prefix(root: &Path) -> String {
    let mut s = std::env::var("HOME").map_or_else(
        |_| root.display().to_string(),
        |home| {
            let home_path = PathBuf::from(home);
            root.strip_prefix(&home_path).map_or_else(
                |_| root.display().to_string(),
                |rel| {
                    if rel.as_os_str().is_empty() {
                        "~".to_owned()
                    } else {
                        format!("~/{}", rel.display())
                    }
                },
            )
        },
    );
    if !s.ends_with('/') {
        s.push('/');
    }
    s
}

/// Walk the directory collecting files, respecting
/// `.gitignore` and hidden file rules.
fn collect_files(root: &Path) -> Vec<PathBuf> {
    WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .sort_by_file_name(Ord::cmp)
        .build()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_some_and(|ft| ft.is_file()))
        .map(ignore::DirEntry::into_path)
        .collect()
}

/// Open a file as a new buffer (reuses an existing
/// one with the same path).
fn open_file(ctx: &mut Context, path: &Path) {
    match Document::open(path) {
        Ok(doc) => {
            ctx.editor.open_document(doc);
            ctx.editor.vim.reset();
            ctx.editor.status_message = None;
        }
        Err(e) => {
            ctx.editor.status_message =
                Some(format!("Error opening file: {e}"));
        }
    }
}
