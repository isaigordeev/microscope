use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use ms_core::history::History;
use ms_view::document::Document;
use ms_view::view::View;

use crate::compositor::Context;

use super::picker::Picker;

/// Build a file picker for the given workspace root.
pub fn file_picker(root: &Path) -> Picker<PathBuf> {
    let files = collect_files(root);
    let root_owned = root.to_path_buf();

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

/// Open a file, replacing the current document.
fn open_file(ctx: &mut Context, path: &Path) {
    match Document::open(path) {
        Ok(doc) => {
            let height = ctx.editor.view.height;
            ctx.editor.document = doc;
            ctx.editor.view = View::new(height);
            ctx.editor.history = History::new();
            ctx.editor.vim.reset();
            ctx.editor.status_message = None;
        }
        Err(e) => {
            ctx.editor.status_message =
                Some(format!("Error opening file: {e}"));
        }
    }
}
