//! Directory tree visualization.
//!
//! Git-aware tree display using the `ignore` crate for gitignore support.

use ignore::WalkBuilder;
use std::collections::BTreeMap;
use std::path::Path;

/// Result of tree generation
pub struct TreeResult {
    pub root_name: String,
    pub lines: Vec<String>,
    pub file_count: usize,
    pub dir_count: usize,
}

/// A node in the file tree
#[derive(Default)]
struct TreeNode {
    children: BTreeMap<String, TreeNode>,
    is_dir: bool,
}

impl TreeNode {
    fn add_path(&mut self, parts: &[&str], is_dir: bool) {
        if parts.is_empty() {
            return;
        }

        let name = parts[0];
        let child = self.children.entry(name.to_string()).or_default();

        if parts.len() == 1 {
            child.is_dir = is_dir;
        } else {
            child.is_dir = true; // intermediate nodes are directories
            child.add_path(&parts[1..], is_dir);
        }
    }
}

/// Generate a tree visualization for a directory
pub fn generate_tree(root: &Path, max_depth: Option<usize>, dirs_only: bool) -> TreeResult {
    let root_name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());

    let walker = WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .max_depth(max_depth)
        .build();

    let mut tree = TreeNode::default();
    tree.is_dir = true;

    let mut file_count = 0;
    let mut dir_count = 0;

    for entry in walker.flatten() {
        let path = entry.path();
        if path == root {
            continue;
        }

        if let Ok(rel) = path.strip_prefix(root) {
            let rel_str = rel.to_string_lossy();
            if rel_str.is_empty() {
                continue;
            }

            let is_dir = path.is_dir();

            // Skip files if dirs_only
            if dirs_only && !is_dir {
                continue;
            }

            let parts: Vec<&str> = rel_str.split('/').filter(|s| !s.is_empty()).collect();
            if !parts.is_empty() {
                tree.add_path(&parts, is_dir);

                if is_dir {
                    dir_count += 1;
                } else {
                    file_count += 1;
                }
            }
        }
    }

    let mut lines = vec![root_name.clone()];
    render_tree(&tree, "", &mut lines, dirs_only);

    TreeResult {
        root_name,
        lines,
        file_count,
        dir_count,
    }
}

fn render_tree(node: &TreeNode, prefix: &str, lines: &mut Vec<String>, dirs_only: bool) {
    // Sort children: directories first, then alphabetically
    let mut children: Vec<_> = node.children.iter().collect();
    children.sort_by(|(a_name, a_node), (b_name, b_node)| {
        match (b_node.is_dir, a_node.is_dir) {
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            _ => a_name.to_lowercase().cmp(&b_name.to_lowercase()),
        }
    });

    let count = children.len();
    for (i, (name, child)) in children.into_iter().enumerate() {
        let is_last = i == count - 1;
        let connector = if is_last { "└── " } else { "├── " };

        lines.push(format!("{}{}{}", prefix, connector, name));

        // Recurse into directories
        if child.is_dir && !child.children.is_empty() {
            let new_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });
            render_tree(child, &new_prefix, lines, dirs_only);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_basic_tree() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src/foo")).unwrap();
        fs::write(dir.path().join("src/main.rs"), "").unwrap();
        fs::write(dir.path().join("src/foo/bar.rs"), "").unwrap();
        fs::write(dir.path().join("README.md"), "").unwrap();

        let result = generate_tree(dir.path(), None, false);

        assert!(result.file_count >= 3);
        assert!(result.dir_count >= 2);
        assert!(result.lines.len() > 1);
    }

    #[test]
    fn test_dirs_only() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src/foo")).unwrap();
        fs::write(dir.path().join("src/main.rs"), "").unwrap();

        let result = generate_tree(dir.path(), None, true);

        // Should only count directories
        assert_eq!(result.file_count, 0);
        assert!(result.dir_count >= 2);
    }

    #[test]
    fn test_max_depth() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("a/b/c/d")).unwrap();
        fs::write(dir.path().join("a/b/c/d/file.txt"), "").unwrap();

        let result = generate_tree(dir.path(), Some(2), false);

        // Should stop at depth 2 (a, a/b)
        assert!(result.dir_count <= 2);
    }
}
