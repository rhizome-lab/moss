//! Directory tree visualization.
//!
//! Git-aware tree display using the `ignore` crate for gitignore support.

use crate::skeleton::{SkeletonExtractor, SkeletonSymbol};
use ignore::WalkBuilder;
use moss_languages::support_for_path;
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

/// Unified node for viewing directories, files, and symbols.
///
/// This is the common abstraction for `moss view` - directories contain files,
/// files contain symbols, symbols can contain nested symbols.
#[derive(Debug, Clone, Serialize)]
pub struct ViewNode {
    /// Display name (filename, symbol name)
    pub name: String,
    /// Node type
    pub kind: ViewNodeKind,
    /// Full path from root (e.g., "src/main.rs" or "src/main.rs/Foo/bar")
    pub path: String,
    /// Child nodes
    pub children: Vec<ViewNode>,

    /// Signature (for symbols: "def foo(x: int) -> str")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// Docstring (for symbols)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docstring: Option<String>,
    /// Line range in file (start, end)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_range: Option<(usize, usize)>,
}

/// Type of node in the view tree.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ViewNodeKind {
    Directory,
    File,
    /// Symbol with its kind (class, function, method, etc.)
    #[serde(rename = "symbol")]
    Symbol(String),
}

impl ViewNode {
    /// Create a file node.
    pub fn file(name: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: ViewNodeKind::File,
            path: path.into(),
            children: Vec::new(),
            signature: None,
            docstring: None,
            line_range: None,
        }
    }

    /// Add multiple children.
    pub fn with_children(mut self, children: Vec<ViewNode>) -> Self {
        self.children = children;
        self
    }
}

/// Options for formatting ViewNodes.
#[derive(Clone, Default)]
pub struct FormatOptions {
    /// Include docstrings in output.
    pub docstrings: bool,
    /// Maximum depth to display (None = unlimited).
    pub max_depth: Option<usize>,
    /// Show line numbers for symbols.
    pub line_numbers: bool,
    /// Skip the root node and only show children (useful for file views).
    pub skip_root: bool,
}

/// Format a ViewNode as text output.
///
/// Handles all node types (directory, file, symbol) with consistent tree-style formatting.
pub fn format_view_node(node: &ViewNode, options: &FormatOptions) -> Vec<String> {
    let mut lines = Vec::new();

    if !options.skip_root {
        // Root line: name with optional signature and line numbers
        let root_line = format_node_line(node, options);
        lines.push(root_line);

        // Add docstring if requested
        if options.docstrings {
            if let Some(doc) = &node.docstring {
                let first_line = doc.lines().next().unwrap_or("").trim();
                if !first_line.is_empty() && !is_useless_docstring(&node.name, first_line) {
                    lines.push(format!("    \"\"\"{}\"\"\"", first_line));
                }
            }
        }
    }

    // Render children
    let prefix = if options.skip_root { "" } else { "" };
    format_children(&node.children, prefix, &mut lines, options, 0);

    lines
}

/// Format a single node line with optional line numbers.
fn format_node_line(node: &ViewNode, options: &FormatOptions) -> String {
    let base = match &node.kind {
        ViewNodeKind::Symbol(_) => {
            if let Some(sig) = &node.signature {
                format!("{}:", sig)
            } else {
                format!("{}:", node.name)
            }
        }
        _ => node.name.clone(),
    };

    // Add line info for symbols if requested
    if options.line_numbers {
        if let Some((start, end)) = node.line_range {
            let size = end.saturating_sub(start) + 1;
            return format!("{} L{}-{} ({} lines)", base, start, end, size);
        }
    }

    base
}

/// Check if a docstring is useless (just repeats the name).
fn is_useless_docstring(name: &str, doc_line: &str) -> bool {
    let doc_lower = doc_line.to_lowercase();
    let name_lower = name.to_lowercase();

    // Docstring is just the name, optionally with "function" or "class" etc.
    if doc_lower == name_lower
        || doc_lower == format!("{} function", name_lower)
        || doc_lower == format!("{} method", name_lower)
        || doc_lower == format!("{} class", name_lower)
        || doc_lower == format!("the {} function", name_lower)
        || doc_lower == format!("the {} method", name_lower)
        || doc_lower == format!("a {} function", name_lower)
    {
        return true;
    }

    // Very short and starts with the name
    doc_lower.len() < name_lower.len() + 10 && doc_lower.starts_with(&name_lower)
}

fn format_children(
    children: &[ViewNode],
    prefix: &str,
    lines: &mut Vec<String>,
    options: &FormatOptions,
    depth: usize,
) {
    // Check depth limit
    if let Some(max) = options.max_depth {
        if depth >= max {
            return;
        }
    }

    let count = children.len();
    for (i, child) in children.iter().enumerate() {
        let is_last = i == count - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let child_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });

        // Format child line using shared formatter
        let child_line = format_node_line(child, options);
        lines.push(format!("{}{}{}", prefix, connector, child_line));

        // Add docstring if requested (for symbols)
        if options.docstrings {
            if let Some(doc) = &child.docstring {
                let first_line = doc.lines().next().unwrap_or("").trim();
                if !first_line.is_empty() && !is_useless_docstring(&child.name, first_line) {
                    lines.push(format!("{}    \"\"\"{}\"\"\"", child_prefix, first_line));
                }
            }
        }

        // Recurse into children
        if !child.children.is_empty() {
            format_children(&child.children, &child_prefix, lines, options, depth + 1);
        } else if matches!(&child.kind, ViewNodeKind::Symbol(_)) {
            // Symbols without children get "..." placeholder
            lines.push(format!("{}    ...", child_prefix));
        }
    }
}

/// Default boilerplate directories that don't count against depth limit.
/// These are common structural directories that add noise without information.
pub const DEFAULT_BOILERPLATE_DIRS: &[&str] =
    &["src", "lib", "pkg", "packages", "crates", "internal", "cmd"];

/// Options for tree generation
#[derive(Clone)]
pub struct TreeOptions {
    /// Maximum depth to traverse (None = unlimited)
    pub max_depth: Option<usize>,
    /// Collapse single-child directory chains (src/foo/bar/ → one line)
    pub collapse_single: bool,
    /// Directories that don't count against depth limit (smart depth)
    pub boilerplate_dirs: HashSet<String>,
    /// Include symbols inside files (requires depth > 1)
    pub include_symbols: bool,
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
            include_symbols: false,
        }
    }
}

/// A node in the internal file tree (used during construction).
#[derive(Default)]
struct InternalTreeNode {
    children: BTreeMap<String, InternalTreeNode>,
    is_dir: bool,
}

impl InternalTreeNode {
    fn add_path(
        &mut self,
        parts: &[&str],
        is_dir: bool,
        max_depth: Option<usize>,
        boilerplate_dirs: &HashSet<String>,
        effective_depth: usize,
    ) {
        if parts.is_empty() {
            return;
        }

        let name = parts[0];

        // Check if we've exceeded max depth (using effective depth that excludes boilerplate)
        // Boilerplate dirs themselves are always shown, but they don't increase depth
        if let Some(max) = max_depth {
            let is_boilerplate = boilerplate_dirs.contains(name);
            // Block if we're at max depth, unless this is a boilerplate dir (which gets a pass)
            if effective_depth >= max && !is_boilerplate {
                return;
            }
        }

        let child = self.children.entry(name.to_string()).or_default();

        if parts.len() == 1 {
            child.is_dir = is_dir;
        } else {
            child.is_dir = true; // intermediate nodes are directories
                                 // Boilerplate dirs don't count against depth
            let next_depth = if boilerplate_dirs.contains(name) {
                effective_depth
            } else {
                effective_depth + 1
            };
            child.add_path(&parts[1..], is_dir, max_depth, boilerplate_dirs, next_depth);
        }
    }
}

/// Generate a ViewNode tree for a directory.
///
/// Returns a unified ViewNode that can be formatted consistently with file and symbol views.
pub fn generate_view_tree(root: &Path, options: &TreeOptions) -> ViewNode {
    let root_name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());

    // Don't use WalkBuilder's max_depth - we handle it with smart depth (boilerplate awareness)
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .build();

    let mut tree = InternalTreeNode::default();
    tree.is_dir = true;

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
                tree.add_path(
                    &parts,
                    is_dir,
                    options.max_depth,
                    &options.boilerplate_dirs,
                    0,
                );
            }
        }
    }

    // Convert internal tree to ViewNode
    tree_node_to_view_node(&root_name, "", &tree, options, root)
}

/// Convert internal TreeNode to ViewNode recursively.
fn tree_node_to_view_node(
    name: &str,
    parent_path: &str,
    node: &InternalTreeNode,
    options: &TreeOptions,
    fs_root: &Path,
) -> ViewNode {
    let path = if parent_path.is_empty() {
        name.to_string()
    } else {
        format!("{}/{}", parent_path, name)
    };

    let kind = if node.is_dir {
        ViewNodeKind::Directory
    } else {
        ViewNodeKind::File
    };

    // Collect and sort children: directories first, then alphabetically
    let mut children_vec: Vec<_> = node.children.iter().collect();
    children_vec.sort_by(|(a_name, a_node), (b_name, b_node)| {
        match (b_node.is_dir, a_node.is_dir) {
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            _ => a_name.to_lowercase().cmp(&b_name.to_lowercase()),
        }
    });

    // Handle single-child chain collapsing
    let (final_name, final_path, mut children) = if options.collapse_single && node.is_dir {
        let chain = collect_single_chain_internal(node, name);
        let collapsed_path = if parent_path.is_empty() {
            chain.path.clone()
        } else {
            format!("{}/{}", parent_path, chain.path)
        };

        let child_nodes: Vec<ViewNode> = chain
            .end_node
            .children
            .iter()
            .map(|(child_name, child_node)| {
                tree_node_to_view_node(child_name, &collapsed_path, child_node, options, fs_root)
            })
            .collect();

        (chain.path, collapsed_path, child_nodes)
    } else {
        let child_nodes: Vec<ViewNode> = children_vec
            .into_iter()
            .map(|(child_name, child_node)| {
                tree_node_to_view_node(child_name, &path, child_node, options, fs_root)
            })
            .collect();

        (name.to_string(), path, child_nodes)
    };

    // For files, extract symbols if include_symbols is enabled
    if !node.is_dir && options.include_symbols {
        // Get the actual file path on disk
        let file_path = fs_root.parent().unwrap_or(fs_root).join(&final_path);
        if let Some(symbol_children) = extract_file_symbols(&file_path, &final_path) {
            children = symbol_children;
        }
    }

    ViewNode {
        name: final_name,
        kind,
        path: final_path,
        children,
        signature: None,
        docstring: None,
        line_range: None,
    }
}

/// Extract symbols from a file and convert to ViewNodes.
fn extract_file_symbols(file_path: &Path, view_path: &str) -> Option<Vec<ViewNode>> {
    // Check if file has language support
    let _support = support_for_path(file_path)?;

    // Read file content
    let content = std::fs::read_to_string(file_path).ok()?;

    // Extract skeleton
    let extractor = SkeletonExtractor::new();
    let result = extractor.extract(file_path, &content);

    if result.symbols.is_empty() {
        return None;
    }

    // Convert to ViewNodes
    let children: Vec<ViewNode> = result
        .symbols
        .iter()
        .map(|sym| skeleton_to_view_node(sym, view_path))
        .collect();

    Some(children)
}

/// Convert a SkeletonSymbol to a ViewNode.
fn skeleton_to_view_node(sym: &SkeletonSymbol, parent_path: &str) -> ViewNode {
    let path = format!("{}/{}", parent_path, sym.name);

    let children: Vec<ViewNode> = sym
        .children
        .iter()
        .map(|child| skeleton_to_view_node(child, &path))
        .collect();

    ViewNode {
        name: sym.name.clone(),
        kind: ViewNodeKind::Symbol(sym.kind.to_string()),
        path,
        children,
        signature: Some(sym.signature.clone()),
        docstring: sym.docstring.clone(),
        line_range: Some((sym.start_line, sym.end_line)),
    }
}

/// Collect a chain of single-child directories (for ViewNode conversion).
fn collect_single_chain_internal<'a>(
    node: &'a InternalTreeNode,
    name: &str,
) -> CollapsedChain<'a, InternalTreeNode> {
    let mut current = node;
    let mut path = name.to_string();

    loop {
        if current.children.len() != 1 {
            break;
        }
        let (child_name, child_node) = current.children.iter().next().unwrap();
        if !child_node.is_dir {
            break;
        }
        path.push('/');
        path.push_str(child_name);
        current = child_node;
    }

    CollapsedChain {
        path,
        end_node: current,
    }
}

/// Result of collapsing a chain of single-child directories.
struct CollapsedChain<'a, T> {
    path: String,
    end_node: &'a T,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_view_tree() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src/foo")).unwrap();
        fs::write(dir.path().join("src/main.rs"), "").unwrap();
        fs::write(dir.path().join("src/foo/bar.rs"), "").unwrap();
        fs::write(dir.path().join("README.md"), "").unwrap();

        let result = generate_view_tree(dir.path(), &TreeOptions::default());

        // Should have directory node with children
        assert_eq!(result.kind, ViewNodeKind::Directory);
        assert!(!result.children.is_empty());
    }

    #[test]
    fn test_view_tree_max_depth() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("alpha/beta/gamma")).unwrap();
        fs::write(dir.path().join("alpha/beta/gamma/file.txt"), "").unwrap();

        let result = generate_view_tree(
            dir.path(),
            &TreeOptions {
                max_depth: Some(2),
                collapse_single: false,
                boilerplate_dirs: HashSet::new(),
                include_symbols: false,
            },
        );

        // Should return a ViewNode structure
        assert_eq!(result.kind, ViewNodeKind::Directory);
    }
}
