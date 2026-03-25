pub use ropey::Rope;

use std::path::Path;

/// Load a file into a Rope.
/// Returns an empty Rope if the path doesn't exist.
///
/// # Errors
/// Returns `io::Error` if the file exists but cannot be read.
pub fn from_file(path: &Path) -> std::io::Result<Rope> {
    if path.exists() {
        Rope::from_reader(std::io::BufReader::new(
            std::fs::File::open(path)?,
        ))
    } else {
        Ok(Rope::new())
    }
}
