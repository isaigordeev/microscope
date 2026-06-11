//! Buffer picker (telescope-style `:Buffers`).

use std::path::PathBuf;

use ms_view::editor::Editor;

use crate::compositor::Context;

use super::picker::Picker;

/// One pickable buffer row.
#[derive(Debug, Clone)]
pub struct BufferItem {
    index: usize,
    label: String,
    path: Option<PathBuf>,
}

/// Build a picker over the open buffers.
pub fn buffer_picker(editor: &Editor) -> Picker<BufferItem> {
    let items: Vec<BufferItem> = editor
        .buffer_infos()
        .iter()
        .map(|b| {
            let name = b.path.as_ref().map_or_else(
                || "[scratch]".to_owned(),
                |p| p.display().to_string(),
            );
            let label = format!(
                "{}{}{} {name}",
                b.index + 1,
                if b.current { "%" } else { " " },
                if b.modified { "+" } else { " " },
            );
            BufferItem { index: b.index, label, path: b.path.clone() }
        })
        .collect();

    Picker::new(
        Box::new(|item: &BufferItem| item.label.clone()),
        Box::new(|ctx: &mut Context, item: &BufferItem| {
            ctx.editor.switch_buffer(item.index);
        }),
        items,
    )
    .with_preview(Box::new(|item: &BufferItem| item.path.clone()))
    .with_prompt_prefix("buffer> ".to_owned())
}
