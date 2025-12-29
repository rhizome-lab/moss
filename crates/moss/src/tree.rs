//! Directory tree visualization.
//!
//! Git-aware tree display using the `ignore` crate for gitignore support.

use crate::parsers::Parsers;
use crate::skeleton::{SkeletonExtractor, SkeletonSymbol};
use ignore::WalkBuilder;
use moss_languages::support_for_path;
use nu_ansi_term::Color::{LightCyan, LightGreen, LightMagenta, Red, White as LightGray, Yellow};
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
    /// Grammar name for syntax highlighting (e.g., "rust", "python")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grammar: Option<String>,
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
            grammar: None,
        }
    }

    /// Add multiple children.
    pub fn with_children(mut self, children: Vec<ViewNode>) -> Self {
        self.children = children;
        self
    }

    /// Set grammar for syntax highlighting.
    pub fn with_grammar(mut self, grammar: impl Into<String>) -> Self {
        self.grammar = Some(grammar.into());
        self
    }
}

/// How to display docstrings in formatted output.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum DocstringDisplay {
    /// No docstrings (skeleton mode).
    None,
    /// Show summary only - first paragraph, up to double blank line.
    #[default]
    Summary,
    /// Show full docstrings (--docs mode).
    Full,
}

/// Options for formatting ViewNodes.
#[derive(Clone, Default)]
pub struct FormatOptions {
    /// How to display docstrings.
    pub docstrings: DocstringDisplay,
    /// Maximum depth to display (None = unlimited).
    pub max_depth: Option<usize>,
    /// Show line numbers for symbols.
    pub line_numbers: bool,
    /// Skip the root node and only show children (useful for file views).
    pub skip_root: bool,
    /// Minimal mode: plain indentation, elide keywords (LLM-optimized).
    pub minimal: bool,
    /// Use ANSI colors in output (respects NO_COLOR and config).
    pub use_colors: bool,
}

/// Extract docstring summary (everything up to double blank line).
///
/// The double blank line convention: `\n\n\n` separates summary from extended docs.
/// Single blank lines (`\n\n`) are normal paragraph breaks within the summary.
/// If no double blank, the entire docstring is considered the summary.
pub fn docstring_summary(doc: &str) -> &str {
    if let Some(pos) = doc.find("\n\n\n") {
        doc[..pos].trim()
    } else {
        doc.trim()
    }
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

        // Add docstring based on display mode
        if let Some(doc) = &node.docstring {
            format_docstring(doc, &node.name, "    ", options.docstrings, &mut lines);
        }
    }

    // Render children
    let prefix = if options.skip_root { "" } else { "" };
    format_children(&node.children, prefix, &mut lines, options, 0);

    lines
}

/// Format a docstring according to the display mode.
fn format_docstring(
    doc: &str,
    name: &str,
    prefix: &str,
    mode: DocstringDisplay,
    lines: &mut Vec<String>,
) {
    match mode {
        DocstringDisplay::None => {}
        DocstringDisplay::Summary => {
            let summary = docstring_summary(doc);
            if summary.is_empty()
                || is_useless_docstring(name, summary.lines().next().unwrap_or(""))
            {
                return;
            }
            // Show entire summary paragraph
            let summary_lines: Vec<&str> = summary.lines().collect();
            if summary_lines.len() == 1 {
                lines.push(format!("{}\"\"\"{}\"\"\"", prefix, summary_lines[0]));
            } else {
                lines.push(format!("{}\"\"\"", prefix));
                for line in summary_lines {
                    lines.push(format!("{}{}", prefix, line));
                }
                lines.push(format!("{}\"\"\"", prefix));
            }
        }
        DocstringDisplay::Full => {
            let trimmed = doc.trim();
            if trimmed.is_empty()
                || is_useless_docstring(name, trimmed.lines().next().unwrap_or(""))
            {
                return;
            }
            // Multi-line docstring
            let doc_lines: Vec<&str> = trimmed.lines().collect();
            if doc_lines.len() == 1 {
                lines.push(format!("{}\"\"\"{}\"\"\"", prefix, doc_lines[0]));
            } else {
                lines.push(format!("{}\"\"\"", prefix));
                for line in doc_lines {
                    lines.push(format!("{}{}", prefix, line));
                }
                lines.push(format!("{}\"\"\"", prefix));
            }
        }
    }
}

/// Format a single node line with optional line numbers.
fn format_node_line(node: &ViewNode, options: &FormatOptions) -> String {
    let base = match &node.kind {
        ViewNodeKind::Symbol(_) => {
            if let Some(sig) = &node.signature {
                // Minimal: elide keywords; Pretty: highlight with AST
                let sig_display = if options.minimal {
                    elide_keywords(sig)
                } else if let Some(grammar) = &node.grammar {
                    highlight_source(sig, grammar, options.use_colors)
                } else {
                    sig.clone()
                };
                format!("{}:", sig_display)
            } else {
                format!("{}:", node.name)
            }
        }
        _ => node.name.clone(),
    };

    // Add line info for symbols if requested
    if options.line_numbers {
        if let Some((start, end)) = node.line_range {
            return format!("{} L{}-{}", base, start, end);
        }
    }

    base
}

/// Elide visibility and declaration keywords for minimal output.
fn elide_keywords(sig: &str) -> String {
    let mut s = sig.to_string();
    // Visibility
    for kw in [
        "pub ",
        "pub(crate) ",
        "pub(super) ",
        "pub(self) ",
        "private ",
    ] {
        s = s.replacen(kw, "", 1);
    }
    // Declaration keywords (keep the name/signature, remove the keyword)
    for kw in [
        "fn ",
        "async fn ",
        "const fn ",
        "unsafe fn ",
        "struct ",
        "enum ",
        "trait ",
        "impl ",
        "type ",
        "const ",
        "static ",
        "mod ",
        "class ",
        "def ",
        "async def ",
    ] {
        s = s.replacen(kw, "", 1);
    }
    s
}

/// A span of text with highlight information.
struct HighlightSpan {
    start: usize,
    end: usize,
    kind: HighlightKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HighlightKind {
    Keyword,
    Type,
    Comment,
    String,
    Number,
    Constant, // true, false, nil, null, undefined, NaN, Infinity
    Attribute,
    FunctionName,
    Default,
}

/// Highlight source code using AST-based syntax highlighting.
pub fn highlight_source(sig: &str, grammar: &str, use_colors: bool) -> String {
    // If colors disabled, just return the signature as-is
    if !use_colors {
        return sig.to_string();
    }

    let parsers = Parsers::new();
    let tree = match parsers.parse_with_grammar(grammar, sig) {
        Some(t) => t,
        None => return sig.to_string(), // Fallback to unhighlighted
    };

    let mut spans: Vec<HighlightSpan> = Vec::new();
    collect_highlight_spans(tree.root_node(), &mut spans);

    // Sort spans by start position
    spans.sort_by_key(|s| s.start);

    // Remove overlapping spans - keep the first one encountered
    let mut filtered: Vec<HighlightSpan> = Vec::new();
    for span in spans {
        let overlaps = filtered
            .iter()
            .any(|existing| span.start < existing.end && span.end > existing.start);
        if !overlaps {
            filtered.push(span);
        }
    }

    // Build highlighted string
    let mut result = String::new();
    let mut pos = 0;

    for span in filtered {
        // Skip if we've passed this span already
        if span.start < pos {
            continue;
        }

        // Add unhighlighted text before this span
        if span.start > pos {
            result.push_str(&sig[pos..span.start]);
        }

        // Add highlighted span (Monokai-inspired colors)
        let text = &sig[span.start..span.end];
        let styled = match span.kind {
            HighlightKind::Keyword => Red.paint(text).to_string(), // Red for keywords
            HighlightKind::Type => LightCyan.paint(text).to_string(), // Light cyan for types
            HighlightKind::Comment => LightGray.paint(text).to_string(), // Grey for comments
            HighlightKind::String => LightGreen.paint(text).to_string(), // Light green for strings
            HighlightKind::Number => LightMagenta.paint(text).to_string(), // Magenta for numbers
            HighlightKind::Constant => LightMagenta.paint(text).to_string(), // Magenta for constants
            HighlightKind::Attribute => LightCyan.paint(text).to_string(),   // Cyan for attributes
            HighlightKind::FunctionName => Yellow.paint(text).to_string(),   // Yellow for functions
            HighlightKind::Default => text.to_string(),
        };
        result.push_str(&styled);
        pos = span.end;
    }

    // Add remaining text
    if pos < sig.len() {
        result.push_str(&sig[pos..]);
    }

    result
}

/// Collect highlight spans from AST nodes.
fn collect_highlight_spans(node: tree_sitter::Node, spans: &mut Vec<HighlightSpan>) {
    let kind = node.kind();
    let highlight = classify_node_kind(kind);

    // Comments, strings, attributes: highlight entire node (don't recurse into children)
    if matches!(
        highlight,
        HighlightKind::Comment | HighlightKind::String | HighlightKind::Attribute
    ) {
        spans.push(HighlightSpan {
            start: node.start_byte(),
            end: node.end_byte(),
            kind: highlight,
        });
        return; // Don't recurse - these are single units
    }

    // Only highlight leaf nodes (no children) to avoid duplication
    // This means keywords and simple type identifiers get highlighted,
    // but complex types like `Vec<String>` highlight `Vec` and `String` separately
    if highlight != HighlightKind::Default && node.child_count() == 0 {
        spans.push(HighlightSpan {
            start: node.start_byte(),
            end: node.end_byte(),
            kind: highlight,
        });
    }

    // Check for function/method names and calls
    if node.child_count() == 0 {
        if let Some(parent) = node.parent() {
            let parent_kind = parent.kind();

            // Function/method definitions
            if kind == "identifier"
                && matches!(
                    parent_kind,
                    "function_item"
                        | "function_signature_item"
                        | "function_definition"
                        | "method_definition"
                        | "function_declaration"
                )
            {
                spans.push(HighlightSpan {
                    start: node.start_byte(),
                    end: node.end_byte(),
                    kind: HighlightKind::FunctionName,
                });
            }

            // Function/macro calls: foo() - Rust: call_expression/macro_invocation, JS: call_expression, Python: call, Lua: function_call
            if kind == "identifier"
                && matches!(
                    parent_kind,
                    "call_expression" | "call" | "macro_invocation" | "function_call"
                )
            {
                spans.push(HighlightSpan {
                    start: node.start_byte(),
                    end: node.end_byte(),
                    kind: HighlightKind::FunctionName,
                });
            }

            // Scoped function calls: serde_json::to_value()
            // identifier → scoped_identifier → call_expression
            // Only highlight the last identifier (the function name, not module path)
            if kind == "identifier" && parent_kind == "scoped_identifier" {
                if let Some(grandparent) = parent.parent() {
                    if matches!(grandparent.kind(), "call_expression" | "call") {
                        // Check this is the last identifier (no :: after it)
                        let is_last = node.next_sibling().map_or(true, |s| s.kind() != "::");
                        if is_last {
                            spans.push(HighlightSpan {
                                start: node.start_byte(),
                                end: node.end_byte(),
                                kind: HighlightKind::FunctionName,
                            });
                        }
                    }
                }
            }

            // Method calls: bar.baz()
            // - Rust: field_identifier → field_expression → call_expression
            // - JS/TS: property_identifier → member_expression → call_expression
            // - Python: identifier → attribute → call
            // - Lua: identifier → dot_index_expression/method_index_expression → function_call
            //   For Lua, only the second identifier (after . or :) is the method name
            let is_method_id = matches!(kind, "field_identifier" | "property_identifier")
                || (kind == "identifier" && parent_kind == "attribute");
            let is_lua_method = kind == "identifier"
                && matches!(
                    parent_kind,
                    "dot_index_expression" | "method_index_expression"
                )
                && node
                    .prev_sibling()
                    .map_or(false, |s| matches!(s.kind(), "." | ":"));
            let is_method_parent = matches!(
                parent_kind,
                "field_expression"
                    | "member_expression"
                    | "attribute"
                    | "dot_index_expression"
                    | "method_index_expression"
            );
            if (is_method_id || is_lua_method) && is_method_parent {
                if let Some(grandparent) = parent.parent() {
                    if matches!(
                        grandparent.kind(),
                        "call_expression" | "call" | "function_call"
                    ) {
                        spans.push(HighlightSpan {
                            start: node.start_byte(),
                            end: node.end_byte(),
                            kind: HighlightKind::FunctionName,
                        });
                    }
                }
            }

            // Inside macro token_tree: heuristic for function/method calls
            // Pattern: identifier followed by token_tree starting with (
            // Or: identifier preceded by . (method call)
            if kind == "identifier" && parent_kind == "token_tree" {
                if let Some(next) = node.next_sibling() {
                    // Check if next sibling is () or token_tree starting with (
                    let next_kind = next.kind();
                    if next_kind == "token_tree" || next_kind == "(" {
                        spans.push(HighlightSpan {
                            start: node.start_byte(),
                            end: node.end_byte(),
                            kind: HighlightKind::FunctionName,
                        });
                    }
                }
            }
        }
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_highlight_spans(child, spans);
    }
}

/// Classify a node kind into a highlight category.
fn classify_node_kind(kind: &str) -> HighlightKind {
    match kind {
        // Comments (including doc comments)
        "comment"
        | "line_comment"
        | "block_comment"
        | "doc_comment"
        | "line_outer_doc_comment"
        | "line_inner_doc_comment"
        | "block_outer_doc_comment"
        | "block_inner_doc_comment" => HighlightKind::Comment,

        // Attributes / proc macros (#[derive(...)], @decorator, etc.)
        "attribute_item" | "inner_attribute_item" | "decorator" => HighlightKind::Attribute,

        // Strings (including template/interpolated strings)
        "string_literal"
        | "raw_string_literal"
        | "string"
        | "string_content"
        | "string_fragment"
        | "interpreted_string_literal"
        | "char_literal"
        | "template_string"
        | "template_literal" => HighlightKind::String,

        // Numbers
        "number"
        | "integer"
        | "float"
        | "integer_literal"
        | "float_literal"
        | "int_literal"
        | "imaginary_literal"
        | "rune_literal" => HighlightKind::Number,

        // Constants (booleans, nil/null, special values)
        "true" | "false" | "boolean_literal" | "nil" | "null" | "none" | "undefined" => {
            HighlightKind::Constant
        }

        // Types (check first - more specific)
        "type_identifier"
        | "primitive_type"
        | "generic_type"
        | "scoped_type_identifier"
        | "builtin_type" => HighlightKind::Type,

        // Keywords (cross-language)
        "fn" | "function" | "def" | "async" | "await" | "pub" | "struct" | "enum" | "trait"
        | "impl" | "type" | "const" | "static" | "let" | "mut" | "ref" | "class" | "interface"
        | "extends" | "implements" | "import" | "from" | "export" | "return" | "if" | "else"
        | "for" | "while" | "loop" | "match" | "where" | "self" | "Self" | "super" | "crate"
        | "mod" | "use" | "as" | "in" | "unsafe" | "extern" | "dyn"
        // Lua keywords
        | "local" | "end" | "then" | "do" | "elseif" | "repeat" | "until" | "and" | "or"
        | "not" | "break" | "goto"
        // Python keywords
        | "elif" | "except" | "finally" | "try" | "with" | "yield" | "lambda" | "pass"
        | "raise" | "assert" | "global" | "nonlocal" | "del" | "is"
        // JS/TS keywords
        | "var" | "new" | "this" | "throw" | "catch" | "switch" | "case" | "default"
        | "continue" | "debugger" | "delete" | "instanceof" | "typeof" | "void"
        // Go keywords
        | "package" | "func" | "defer" | "go" | "chan" | "select" | "fallthrough"
        | "range" | "map" => HighlightKind::Keyword,

        _ => HighlightKind::Default,
    }
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
        let _is_last = i == count - 1;

        // Always use plain indentation (no box-drawing chars)
        let child_prefix = format!("{}  ", prefix);

        // Format child line using shared formatter
        let child_line = format_node_line(child, options);
        lines.push(format!("{}{}", prefix, child_line));

        // Add docstring based on display mode
        if let Some(doc) = &child.docstring {
            let doc_prefix = format!("{}    ", child_prefix);
            format_docstring(doc, &child.name, &doc_prefix, options.docstrings, lines);
        }

        // Recurse into children
        if !child.children.is_empty() {
            format_children(&child.children, &child_prefix, lines, options, depth + 1);
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
        grammar: None,
    }
}

/// Extract symbols from a file and convert to ViewNodes.
fn extract_file_symbols(file_path: &Path, view_path: &str) -> Option<Vec<ViewNode>> {
    // Check if file has language support
    let support = support_for_path(file_path)?;
    let grammar = support.grammar_name().to_string();

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
        .map(|sym| skeleton_to_view_node(sym, view_path, &grammar))
        .collect();

    Some(children)
}

/// Convert a SkeletonSymbol to a ViewNode.
fn skeleton_to_view_node(sym: &SkeletonSymbol, parent_path: &str, grammar: &str) -> ViewNode {
    let path = format!("{}/{}", parent_path, sym.name);

    let children: Vec<ViewNode> = sym
        .children
        .iter()
        .map(|child| skeleton_to_view_node(child, &path, grammar))
        .collect();

    ViewNode {
        name: sym.name.clone(),
        kind: ViewNodeKind::Symbol(sym.kind.to_string()),
        path,
        children,
        signature: Some(sym.signature.clone()),
        docstring: sym.docstring.clone(),
        line_range: Some((sym.start_line, sym.end_line)),
        grammar: Some(grammar.to_string()),
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
