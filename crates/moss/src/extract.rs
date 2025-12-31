//! Shared symbol extraction from source code.
//!
//! This module provides the core AST traversal logic used by both
//! skeleton.rs (for viewing) and symbols.rs (for indexing).

use crate::parsers;
use moss_languages::{support_for_grammar, support_for_path, Language, Symbol, Visibility};
use std::path::Path;
use tree_sitter;

/// Result of extracting symbols from a file.
pub struct ExtractResult {
    /// Top-level symbols (nested structure preserved)
    pub symbols: Vec<Symbol>,
    /// File path for context
    pub file_path: String,
}

/// Options for symbol extraction.
#[derive(Clone)]
pub struct ExtractOptions {
    /// Include private/non-public symbols (default: true for code exploration)
    pub include_private: bool,
}

impl Default for ExtractOptions {
    fn default() -> Self {
        Self {
            // Default to including all symbols - moss is for code exploration,
            // not API documentation. This ensures trait impl methods are visible.
            include_private: true,
        }
    }
}

/// Shared symbol extractor using the Language trait.
pub struct Extractor {
    options: ExtractOptions,
}

impl Extractor {
    pub fn new() -> Self {
        Self {
            options: ExtractOptions::default(),
        }
    }

    pub fn with_options(options: ExtractOptions) -> Self {
        Self { options }
    }

    /// Extract symbols from a file.
    pub fn extract(&self, path: &Path, content: &str) -> ExtractResult {
        let symbols = match support_for_path(path) {
            Some(support) => self.extract_with_support(content, support),
            None => Vec::new(),
        };

        ExtractResult {
            symbols,
            file_path: path.to_string_lossy().to_string(),
        }
    }

    fn extract_with_support(&self, content: &str, support: &dyn Language) -> Vec<Symbol> {
        let tree = match parsers::parse_with_grammar(support.grammar_name(), content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        self.collect_symbols(&mut cursor, content, support, &mut symbols, false);

        // Post-process for Rust: merge impl blocks with their types
        if support.grammar_name() == "rust" {
            Self::merge_rust_impl_blocks(&mut symbols);
        }

        // Post-process for Markdown: fix section ranges
        if support.grammar_name() == "markdown" {
            Self::fix_markdown_section_ranges(&mut symbols, content);
        }

        symbols
    }

    fn collect_symbols(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        support: &dyn Language,
        symbols: &mut Vec<Symbol>,
        in_container: bool,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            // Check for embedded content (e.g., <script> in Vue/Svelte/HTML)
            if let Some(embedded) = support.embedded_content(&node, content) {
                if let Some(sub_lang) = support_for_grammar(embedded.grammar) {
                    if let Some(sub_tree) =
                        parsers::parse_with_grammar(embedded.grammar, &embedded.content)
                    {
                        let mut sub_symbols = Vec::new();
                        let sub_root = sub_tree.root_node();
                        let mut sub_cursor = sub_root.walk();
                        self.collect_symbols(
                            &mut sub_cursor,
                            &embedded.content,
                            sub_lang,
                            &mut sub_symbols,
                            false,
                        );

                        // Adjust line numbers for embedded content offset
                        for mut sym in sub_symbols {
                            adjust_lines(&mut sym, embedded.start_line - 1);
                            symbols.push(sym);
                        }
                    }
                }
                // Don't descend into embedded nodes - we've already processed them
                if cursor.goto_next_sibling() {
                    continue;
                }
                break;
            }

            // Check if this is a function
            if support.function_kinds().contains(&kind) {
                if let Some(sym) = support.extract_function(&node, content, in_container) {
                    if self.should_include(&sym) {
                        symbols.push(sym);
                    }
                }
            }
            // Check if this is a container (class, impl, module)
            else if support.container_kinds().contains(&kind) {
                if let Some(mut sym) = support.extract_container(&node, content) {
                    if self.should_include(&sym) {
                        // Recurse into container body
                        if let Some(body) = support.container_body(&node) {
                            let mut body_cursor = body.walk();
                            if body_cursor.goto_first_child() {
                                self.collect_symbols(
                                    &mut body_cursor,
                                    content,
                                    support,
                                    &mut sym.children,
                                    true,
                                );
                            }
                        }

                        // Propagate is_interface_impl to all children
                        if sym.is_interface_impl {
                            propagate_interface_impl(&mut sym.children);
                        }

                        symbols.push(sym);
                    }
                }
                // Don't descend further after processing container
                if cursor.goto_next_sibling() {
                    continue;
                }
                break;
            }
            // Check if this is a standalone type (struct, enum, etc.)
            else if support.type_kinds().contains(&kind)
                && !support.container_kinds().contains(&kind)
            {
                if let Some(sym) = support.extract_type(&node, content) {
                    if self.should_include(&sym) {
                        symbols.push(sym);
                    }
                }
            }

            // Descend into children for other nodes
            if cursor.goto_first_child() {
                self.collect_symbols(cursor, content, support, symbols, in_container);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn should_include(&self, sym: &Symbol) -> bool {
        self.options.include_private || matches!(sym.visibility, Visibility::Public)
    }

    /// Merge Rust impl blocks with their corresponding struct/enum types
    fn merge_rust_impl_blocks(symbols: &mut Vec<Symbol>) {
        use std::collections::HashMap;

        // Collect impl blocks and their children
        let mut impl_methods: HashMap<String, Vec<Symbol>> = HashMap::new();

        // Remove impl blocks and collect their methods
        symbols.retain(|sym| {
            if sym.signature.starts_with("impl ") {
                impl_methods
                    .entry(sym.name.clone())
                    .or_default()
                    .extend(sym.children.clone());
                return false;
            }
            true
        });

        // Add methods to matching struct/enum
        for sym in symbols.iter_mut() {
            if matches!(
                sym.kind,
                moss_languages::SymbolKind::Struct | moss_languages::SymbolKind::Enum
            ) {
                if let Some(methods) = impl_methods.remove(&sym.name) {
                    sym.children.extend(methods);
                }
            }
        }

        // Any remaining impl blocks without matching type: add back
        for (name, methods) in impl_methods {
            if !methods.is_empty() {
                symbols.push(Symbol {
                    name: name.clone(),
                    kind: moss_languages::SymbolKind::Module, // impl as module-like
                    signature: format!("impl {}", name),
                    docstring: None,
                    attributes: Vec::new(),
                    start_line: methods.first().map(|m| m.start_line).unwrap_or(0),
                    end_line: methods.last().map(|m| m.end_line).unwrap_or(0),
                    visibility: Visibility::Public,
                    children: methods,
                    is_interface_impl: false,
                });
            }
        }
    }

    /// Fix markdown section ranges and build hierarchy.
    fn fix_markdown_section_ranges(symbols: &mut Vec<Symbol>, content: &str) {
        if symbols.is_empty() {
            return;
        }

        let total_lines = content.lines().count();

        // Extract heading level from signature (e.g., "## Foo" -> 2)
        fn heading_level(sym: &Symbol) -> usize {
            sym.signature.chars().take_while(|&c| c == '#').count()
        }

        // First pass: fix end_line for each heading
        for i in 0..symbols.len() {
            let current_level = heading_level(&symbols[i]);
            let next_start = symbols[i + 1..]
                .iter()
                .find(|s| heading_level(s) <= current_level)
                .map(|s| s.start_line.saturating_sub(1))
                .unwrap_or(total_lines);
            symbols[i].end_line = next_start;
        }

        // Second pass: build hierarchy
        let flat: Vec<Symbol> = std::mem::take(symbols);
        let mut stack: Vec<(usize, Symbol)> = Vec::new();

        for sym in flat {
            let level = heading_level(&sym);

            // Pop symbols that are same or lower level (higher number = lower in hierarchy)
            while let Some((parent_level, _)) = stack.last() {
                if *parent_level >= level {
                    let (_, completed) = stack.pop().unwrap();
                    if let Some((_, parent)) = stack.last_mut() {
                        parent.children.push(completed);
                    } else {
                        symbols.push(completed);
                    }
                } else {
                    break;
                }
            }
            stack.push((level, sym));
        }

        // Flush remaining stack
        while let Some((_, completed)) = stack.pop() {
            if let Some((_, parent)) = stack.last_mut() {
                parent.children.push(completed);
            } else {
                symbols.push(completed);
            }
        }
    }
}

/// Recursively mark all children as interface implementations.
fn propagate_interface_impl(symbols: &mut [Symbol]) {
    for sym in symbols {
        sym.is_interface_impl = true;
        propagate_interface_impl(&mut sym.children);
    }
}

/// Recursively adjust line numbers for symbols (used for embedded content).
fn adjust_lines(sym: &mut Symbol, offset: usize) {
    sym.start_line += offset;
    sym.end_line += offset;
    for child in &mut sym.children {
        adjust_lines(child, offset);
    }
}

/// Compute cyclomatic complexity for a function node.
pub fn compute_complexity(node: &tree_sitter::Node, support: &dyn Language) -> usize {
    let mut complexity = 1; // Base complexity
    let complexity_nodes = support.complexity_nodes();
    let mut cursor = node.walk();

    if !cursor.goto_first_child() {
        return complexity;
    }

    loop {
        if complexity_nodes.contains(&cursor.node().kind()) {
            complexity += 1;
        }

        if cursor.goto_first_child() {
            continue;
        }
        if cursor.goto_next_sibling() {
            continue;
        }
        loop {
            if !cursor.goto_parent() {
                return complexity;
            }
            if cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_extract_python() {
        let extractor = Extractor::new();
        let content = r#"
def foo(x: int) -> str:
    """Convert int to string."""
    return str(x)

class Bar:
    """A bar class."""
    def method(self):
        pass
"#;
        let result = extractor.extract(&PathBuf::from("test.py"), content);
        assert_eq!(result.symbols.len(), 2);

        let foo = &result.symbols[0];
        assert_eq!(foo.name, "foo");
        assert!(foo.signature.contains("def foo"));
        assert_eq!(foo.docstring.as_deref(), Some("Convert int to string."));

        let bar = &result.symbols[1];
        assert_eq!(bar.name, "Bar");
        assert_eq!(bar.children.len(), 1);
        assert_eq!(bar.children[0].name, "method");
    }

    #[test]
    fn test_extract_rust() {
        let extractor = Extractor::new();
        let content = r#"
/// A simple struct
pub struct Foo {
    x: i32,
}

impl Foo {
    /// Create a new Foo
    pub fn new(x: i32) -> Self {
        Self { x }
    }
}
"#;
        let result = extractor.extract(&PathBuf::from("test.rs"), content);

        // Should have struct with method from impl merged
        let foo = result.symbols.iter().find(|s| s.name == "Foo").unwrap();
        assert!(foo.signature.contains("pub struct Foo"));
        assert_eq!(foo.children.len(), 1);
        assert_eq!(foo.children[0].name, "new");
    }

    #[test]
    fn test_include_private() {
        let extractor = Extractor::with_options(ExtractOptions {
            include_private: true,
        });
        let content = r#"
fn private_fn() {}
pub fn public_fn() {}
"#;
        let result = extractor.extract(&PathBuf::from("test.rs"), content);
        let names: Vec<_> = result.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"private_fn"));
        assert!(names.contains(&"public_fn"));
    }
}
