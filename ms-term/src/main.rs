use std::path::Path;
use std::process;

use ms_view::document::Document;
use ms_view::editor::Editor;

mod application;

fn main() {
    let exit_code = main_impl();
    process::exit(exit_code);
}

#[allow(clippy::print_stderr)]
#[tokio::main]
async fn main_impl() -> i32 {
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
                return 1;
            }
        }
    } else {
        Document::scratch()
    };

    let editor = Editor::new(document, 24);

    let mut app = match application::Application::new(
        editor,
    ) {
        Ok(app) => app,
        Err(e) => {
            eprintln!("Error: {e}");
            return 1;
        }
    };

    let mut events = app.event_stream();

    if let Err(e) = app.run(&mut events).await {
        eprintln!("Error: {e}");
        return 1;
    }

    0
}