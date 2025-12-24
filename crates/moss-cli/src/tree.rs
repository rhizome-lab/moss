//! Directory tree visualization.
//!
//! Git-aware tree display using the `ignore` crate for gitignore support.

use ignore::WalkBuilder;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

/// Default boilerplate directories that don't count against depth limit.
/// These are common structural directories that add noise without information.
pub const DEFAULT_BOILERPLATE_DIRS: &[&str] = &[
    "src",
    "lib",
    "pkg",
    "packages",
    "crates",
    "internal",
    "cmd",
];

/// Options for tree generation
#[derive(Clone)]
pub struct TreeOptions {
    /// Maximum depth to traverse (None = unlimited)
    pub max_depth: Option<usize>,
    /// Collapse single-child directory chains (src/foo/bar/ → one line)
    pub collapse_single: bool,
    /// Directories that don't count against depth limit (smart depth)
    pub boilerplate_dirs: HashSet<String>,
}

impl Default for TreeOptions {
    fn default() -> Self {
        Self {
            max_depth: None,
            collapse_single: true,
            boilerplate_dirs: DEFAULT_BOILERPLATE_DIRS
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }
}

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
pub fn generate_tree(root: &Path, options: &TreeOptions) -> TreeResult {
    let root_name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());

    let walker = WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .max_depth(options.max_depth)
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
    render_tree(&tree, "", &mut lines, options);

    TreeResult {
        root_name,
        lines,
        file_count,
        dir_count,
    }
}

/// Result of collapsing a chain of single-child directories
struct CollapsedChain<'a> {
    path: String,
    end_node: &'a TreeNode,
}

/// Collect a chain of single-child directories into a collapsed path
fn collect_single_chain<'a>(node: &'a TreeNode, name: &str) -> CollapsedChain<'a> {
    let mut current = node;
    let mut path = name.to_string();

    loop {
        // Only collapse if exactly one child and it's a directory
        if current.children.len() != 1 {
            break;
        }
        let (child_name, child_node) = current.children.iter().next().unwrap();
        if !child_node.is_dir {
            break;
        }
        // Append to path and continue down the chain
        path.push('/');
        path.push_str(child_name);
        current = child_node;
    }

    CollapsedChain { path, end_node: current }
}

fn render_tree(node: &TreeNode, prefix: &str, lines: &mut Vec<String>, options: &TreeOptions) {
    // Sort children: directories first, then alphabetically
    let mut children: Vec<_> = node.children.iter().collect();
    children.sort_by(
        |(a_name, a_node), (b_name, b_node)| match (b_node.is_dir, a_node.is_dir) {
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            _ => a_name.to_lowercase().cmp(&b_name.to_lowercase()),
        },
    );

    let count = children.len();
    for (i, (name, child)) in children.into_iter().enumerate() {
        let is_last = i == count - 1;
        let connector = if is_last { "└── " } else { "├── " };

        // Collapse single-child directory chains if enabled
        let (display_name, effective_child) = if options.collapse_single && child.is_dir {
            let chain = collect_single_chain(child, name);
            (chain.path, chain.end_node)
        } else {
            (name.clone(), child)
        };

        lines.push(format!("{}{}{}", prefix, connector, display_name));

        // Recurse into directories
        if effective_child.is_dir && !effective_child.children.is_empty() {
            let new_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });
            render_tree(effective_child, &new_prefix, lines, options);
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

        let result = generate_tree(dir.path(), &TreeOptions::default());

        assert!(result.file_count >= 3);
        assert!(result.dir_count >= 2);
        assert!(result.lines.len() > 1);
    }

    #[test]
    fn test_max_depth() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("a/b/c/d")).unwrap();
        fs::write(dir.path().join("a/b/c/d/file.txt"), "").unwrap();

        let result = generate_tree(
            dir.path(),
            &TreeOptions {
                max_depth: Some(2),
                ..Default::default()
            },
        );

        // Should stop at depth 2 (a, a/b)
        assert!(result.dir_count <= 2);
    }

    #[test]
    fn test_collapse_single_child() {
        let dir = tempdir().unwrap();
        // Create a/b/c chain with file at end
        fs::create_dir_all(dir.path().join("a/b/c")).unwrap();
        fs::write(dir.path().join("a/b/c/file.txt"), "").unwrap();

        // With collapse enabled (default)
        let result = generate_tree(dir.path(), &TreeOptions::default());
        // Should show "a/b/c" as single line, not 3 separate entries
        let tree_text = result.lines.join("\n");
        assert!(
            tree_text.contains("a/b/c"),
            "Should collapse single-child chain: {}",
            tree_text
        );

        // With collapse disabled
        let result_raw = generate_tree(
            dir.path(),
            &TreeOptions {
                collapse_single: false,
                ..Default::default()
            },
        );
        let raw_text = result_raw.lines.join("\n");
        // Should show separate entries
        assert!(
            !raw_text.contains("a/b/c"),
            "Should not collapse when disabled: {}",
            raw_text
        );
    }

    #[test]
    fn test_collapse_stops_at_multiple_children() {
        let dir = tempdir().unwrap();
        // Create a/b with two children under b
        fs::create_dir_all(dir.path().join("a/b/c")).unwrap();
        fs::create_dir_all(dir.path().join("a/b/d")).unwrap();
        fs::write(dir.path().join("a/b/c/file.txt"), "").unwrap();
        fs::write(dir.path().join("a/b/d/file.txt"), "").unwrap();

        let result = generate_tree(dir.path(), &TreeOptions::default());
        let tree_text = result.lines.join("\n");
        // Should collapse a/b but not further since b has 2 children
        assert!(
            tree_text.contains("a/b"),
            "Should collapse a/b: {}",
            tree_text
        );
        assert!(
            !tree_text.contains("a/b/c"),
            "Should not collapse past fork: {}",
            tree_text
        );
    }
}
