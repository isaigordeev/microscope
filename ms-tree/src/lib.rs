//! Filesystem tree model (nvim-tree replacement).
//!
//! Flat-vector model: `nodes` holds the *visible* rows
//! in display order. Expanding a directory reads it
//! lazily (`std::fs::read_dir`) and splices the
//! children in after the node; collapsing removes all
//! deeper rows. Directories sort first, then files,
//! both case-insensitively (nvim-tree order).
//!
//! Pure model — rendering and key handling live in
//! ms-term. Per the user's nvim-tree config, dotfiles
//! and gitignored files are shown.

use std::io;
use std::path::{Path, PathBuf};

/// One visible row of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub path: PathBuf,
    pub is_dir: bool,
    /// Nesting depth below the root (root children
    /// are 0).
    pub depth: usize,
    pub expanded: bool,
}

impl Node {
    /// File or directory name for display.
    #[must_use]
    pub fn name(&self) -> String {
        self.path.file_name().map_or_else(
            || self.path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
    }
}

/// Filesystem tree rooted at a directory.
#[derive(Debug)]
pub struct Tree {
    root: PathBuf,
    nodes: Vec<Node>,
}

impl Tree {
    /// Build a tree with the root level expanded.
    ///
    /// # Errors
    /// Returns IO errors from reading the root
    /// directory.
    pub fn new(root: &Path) -> io::Result<Self> {
        let nodes = read_level(root, 0)?;
        Ok(Self { root: root.to_path_buf(), nodes })
    }

    /// Current root directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Visible rows in display order.
    #[must_use]
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// Expand or collapse the directory at `index`.
    /// No-op for files and out-of-range indices.
    ///
    /// # Errors
    /// Returns IO errors from reading the directory.
    pub fn toggle(&mut self, index: usize) -> io::Result<()> {
        let Some(node) = self.nodes.get(index) else {
            return Ok(());
        };
        if !node.is_dir {
            return Ok(());
        }
        if node.expanded {
            self.collapse(index);
        } else {
            self.expand(index)?;
        }
        Ok(())
    }

    fn expand(&mut self, index: usize) -> io::Result<()> {
        let (path, depth) = {
            let node = &self.nodes[index];
            (node.path.clone(), node.depth)
        };
        let children = read_level(&path, depth + 1)?;
        self.nodes[index].expanded = true;
        let at = index + 1;
        self.nodes.splice(at..at, children);
        Ok(())
    }

    /// Remove all rows deeper than the node at `index`.
    pub fn collapse(&mut self, index: usize) {
        let Some(node) = self.nodes.get(index) else {
            return;
        };
        if !node.is_dir || !node.expanded {
            return;
        }
        let depth = node.depth;
        let end = self.nodes[index + 1..]
            .iter()
            .position(|n| n.depth <= depth)
            .map_or(self.nodes.len(), |p| index + 1 + p);
        self.nodes.drain(index + 1..end);
        self.nodes[index].expanded = false;
    }

    /// Index of the parent row of `index`, if visible.
    #[must_use]
    pub fn parent_of(&self, index: usize) -> Option<usize> {
        let depth = self.nodes.get(index)?.depth;
        if depth == 0 {
            return None;
        }
        self.nodes[..index].iter().rposition(|n| n.depth < depth)
    }

    /// Re-root the tree (nvim-tree `C` / `-`).
    ///
    /// # Errors
    /// Returns IO errors from reading the new root.
    pub fn set_root(&mut self, root: &Path) -> io::Result<()> {
        self.nodes = read_level(root, 0)?;
        self.root = root.to_path_buf();
        Ok(())
    }
}

/// Read one directory level, dirs first then files,
/// case-insensitive alphabetical.
fn read_level(dir: &Path, depth: usize) -> io::Result<Vec<Node>> {
    let mut nodes: Vec<Node> = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|entry| {
            let path = entry.path();
            let is_dir = path.is_dir();
            Node { path, is_dir, depth, expanded: false }
        })
        .collect();
    nodes.sort_by(|a, b| {
        b.is_dir.cmp(&a.is_dir).then_with(|| name_key(a).cmp(&name_key(b)))
    });
    Ok(nodes)
}

fn name_key(node: &Node) -> String {
    node.name().to_lowercase()
}

// ── Tests ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// `dir/` entries end with `/`, files don't.
    fn fixture(entries: &[&str]) -> tempfile::TempDir {
        let dir =
            tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        for entry in entries {
            let path = dir.path().join(entry.trim_end_matches('/'));
            if entry.ends_with('/') {
                std::fs::create_dir_all(&path)
                    .unwrap_or_else(|e| panic!("mkdir: {e}"));
            } else {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)
                        .unwrap_or_else(|e| panic!("mkdir: {e}"));
                }
                std::fs::write(&path, b"x")
                    .unwrap_or_else(|e| panic!("write: {e}"));
            }
        }
        dir
    }

    fn names(tree: &Tree) -> Vec<String> {
        tree.nodes().iter().map(Node::name).collect()
    }

    #[test]
    fn dirs_first_then_alpha() {
        let dir = fixture(&["zeta.txt", "Alpha.txt", "src/", "Beta/"]);
        let tree = Tree::new(dir.path()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(names(&tree), ["Beta", "src", "Alpha.txt", "zeta.txt"]);
    }

    #[test]
    fn expand_splices_children() {
        let dir = fixture(&["src/main.rs", "src/lib.rs", "readme.md"]);
        let mut tree = Tree::new(dir.path()).unwrap_or_else(|e| panic!("{e}"));
        tree.toggle(0).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(names(&tree), ["src", "lib.rs", "main.rs", "readme.md"]);
        assert_eq!(tree.nodes()[1].depth, 1);
        assert!(tree.nodes()[0].expanded);
    }

    #[test]
    fn collapse_removes_deeper_rows() {
        let dir = fixture(&["src/a/deep.rs", "src/top.rs", "readme.md"]);
        let mut tree = Tree::new(dir.path()).unwrap_or_else(|e| panic!("{e}"));
        tree.toggle(0).unwrap_or_else(|e| panic!("{e}")); // src
        tree.toggle(1).unwrap_or_else(|e| panic!("{e}")); // src/a
        assert_eq!(
            names(&tree),
            ["src", "a", "deep.rs", "top.rs", "readme.md"],
        );
        tree.toggle(0).unwrap_or_else(|e| panic!("{e}")); // collapse src
        assert_eq!(names(&tree), ["src", "readme.md"]);
    }

    #[test]
    fn toggle_file_is_noop() {
        let dir = fixture(&["a.txt"]);
        let mut tree = Tree::new(dir.path()).unwrap_or_else(|e| panic!("{e}"));
        tree.toggle(0).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(names(&tree), ["a.txt"]);
    }

    #[test]
    fn parent_of_nested_row() {
        let dir = fixture(&["src/main.rs", "readme.md"]);
        let mut tree = Tree::new(dir.path()).unwrap_or_else(|e| panic!("{e}"));
        tree.toggle(0).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(tree.parent_of(1), Some(0));
        assert_eq!(tree.parent_of(0), None);
    }

    #[test]
    fn set_root_descends() {
        let dir = fixture(&["src/main.rs", "readme.md"]);
        let mut tree = Tree::new(dir.path()).unwrap_or_else(|e| panic!("{e}"));
        tree.set_root(&dir.path().join("src"))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(names(&tree), ["main.rs"]);
        assert!(tree.root().ends_with("src"));
    }

    #[test]
    fn dotfiles_are_shown() {
        let dir = fixture(&[".hidden", "visible.txt"]);
        let tree = Tree::new(dir.path()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(names(&tree), [".hidden", "visible.txt"]);
    }
}
