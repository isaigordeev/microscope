use std::path::Path;
use std::process;

use ms_view::document::Document;
use ms_view::editor::Editor;

mod application;

#[allow(clippy::print_stderr)]
fn main() {
    let args: Vec<String> = std::env::args().collect();

    let document = if args.len() > 1 {
        let path = Path::new(&args[1]);
        match Document::open(path) {
            Ok(doc) => doc,
            Err(e) => {
                eprintln!(
                    "Error opening {}: {e}",
                    path.display()
                );
                process::exit(1);
            }
        }
    } else {
        Document::scratch()
    };

    // Default height; will be resized on first render
    let editor = Editor::new(document, 24);

    if let Err(e) = application::run(editor) {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}