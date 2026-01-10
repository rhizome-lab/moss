//! Shared symbol extraction from source code.
//!
//! This module provides the core AST traversal logic used by both
//! skeleton.rs (for viewing) and symbols.rs (for indexing).

use crate::parsers;
use rhizome_moss_languages::{Language, Symbol, Visibility, support_for_grammar, support_for_path};
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

/// Resolver for cross-file interface method lookups.
/// Used to find interface/class method signatures from other files.
pub trait InterfaceResolver {
    /// Get method names for an interface/class by name.
    /// Returns None if the interface cannot be resolved (external, missing, etc.).
    fn resolve_interface_methods(&self, name: &str, current_file: &str) -> Option<Vec<String>>;
}

/// Resolver that uses FileIndex for cross-file interface lookups.
/// This is the fast path when an index is available.
pub struct IndexedResolver<'a> {
    index: &'a crate::index::FileIndex,
}

impl<'a> IndexedResolver<'a> {
    pub fn new(index: &'a crate::index::FileIndex) -> Self {
        Self { index }
    }
}

impl InterfaceResolver for IndexedResolver<'_> {
    fn resolve_interface_methods(&self, name: &str, current_file: &str) -> Option<Vec<String>> {
        let rt = tokio::runtime::Runtime::new().ok()?;

        // First try to resolve the import to find the source file
        if let Ok(Some((source_module, _original_name))) =
            rt.block_on(self.index.resolve_import(current_file, name))
        {
            // Convert module to file path and query type_methods
            // For now, try the source_module as a relative path
            let methods = rt
                .block_on(self.index.get_type_methods(&source_module, name))
                .ok()?;
            if !methods.is_empty() {
                return Some(methods);
            }
        }

        // Also check if the type is defined in any indexed file
        if let Ok(files) = rt.block_on(self.index.find_type_definitions(name)) {
            for file in files {
                if let Ok(methods) = rt.block_on(self.index.get_type_methods(&file, name)) {
                    if !methods.is_empty() {
                        return Some(methods);
                    }
                }
            }
        }

        None
    }
}

/// Resolver that parses files on-demand for cross-file interface lookups.
/// This is the fallback when no index is available.
pub struct OnDemandResolver<'a> {
    root: &'a std::path::Path,
}

impl<'a> OnDemandResolver<'a> {
    pub fn new(root: &'a std::path::Path) -> Self {
        Self { root }
    }
}

impl InterfaceResolver for OnDemandResolver<'_> {
    fn resolve_interface_methods(&self, name: &str, current_file: &str) -> Option<Vec<String>> {
        use rhizome_moss_languages::support_for_path;

        let current_path = std::path::Path::new(current_file);
        let current_dir = current_path.parent()?;

        // Try common patterns for interface files
        // This is a heuristic - we check nearby files that might contain the interface
        let candidates = [
            "types.ts",
            "interfaces.ts",
            "index.ts",
            "../types.ts",
            "../interfaces.ts",
            "../index.ts",
        ];

        for candidate in candidates {
            let candidate_path = if candidate.starts_with("..") {
                current_dir.parent()?.join(&candidate[3..])
            } else {
                current_dir.join(candidate)
            };

            // Try with root prefix
            let full_path = self.root.join(&candidate_path);
            if !full_path.exists() {
                continue;
            }

            let content = std::fs::read_to_string(&full_path).ok()?;
            // Verify it's a supported file type
            let _support = support_for_path(&full_path)?;

            // Parse the file and look for the interface
            let extractor = Extractor::new();
            // Don't use resolver here to avoid recursion
            let result = extractor.extract(&full_path, &content);

            for sym in &result.symbols {
                if sym.name == name
                    && matches!(
                        sym.kind,
                        rhizome_moss_languages::SymbolKind::Interface
                            | rhizome_moss_languages::SymbolKind::Class
                    )
                {
                    let methods: Vec<String> = sym
                        .children
                        .iter()
                        .filter(|c| {
                            matches!(
                                c.kind,
                                rhizome_moss_languages::SymbolKind::Method
                                    | rhizome_moss_languages::SymbolKind::Function
                            )
                        })
                        .map(|c| c.name.clone())
                        .collect();
                    if !methods.is_empty() {
                        return Some(methods);
                    }
                }
            }
        }

        None
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
        self.extract_with_resolver(path, content, None)
    }

    /// Extract symbols from a file with optional cross-file interface resolution.
    pub fn extract_with_resolver(
        &self,
        path: &Path,
        content: &str,
        resolver: Option<&dyn InterfaceResolver>,
    ) -> ExtractResult {
        let file_path = path.to_string_lossy().to_string();
        let symbols = match support_for_path(path) {
            Some(support) => self.extract_with_support(content, support, resolver, &file_path),
            None => Vec::new(),
        };

        ExtractResult { symbols, file_path }
    }

    fn extract_with_support(
        &self,
        content: &str,
        support: &dyn Language,
        resolver: Option<&dyn InterfaceResolver>,
        current_file: &str,
    ) -> Vec<Symbol> {
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

        // Post-process for TypeScript/JavaScript: mark interface implementations
        if support.grammar_name() == "typescript" || support.grammar_name() == "javascript" {
            Self::mark_interface_implementations(&mut symbols, resolver, current_file);
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
            if let Some(embedded) = support.embedded_content(&node, content)
                && let Some(sub_lang) = support_for_grammar(embedded.grammar)
                && let Some(sub_tree) =
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
                rhizome_moss_languages::SymbolKind::Struct
                    | rhizome_moss_languages::SymbolKind::Enum
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
                    kind: rhizome_moss_languages::SymbolKind::Module, // impl as module-like
                    signature: format!("impl {}", name),
                    docstring: None,
                    attributes: Vec::new(),
                    start_line: methods.first().map(|m| m.start_line).unwrap_or(0),
                    end_line: methods.last().map(|m| m.end_line).unwrap_or(0),
                    visibility: Visibility::Public,
                    children: methods,
                    is_interface_impl: false,
                    implements: Vec::new(),
                });
            }
        }
    }

    /// Mark methods that implement interfaces (for TypeScript/JavaScript).
    /// Builds a map of interface/class names to their method names,
    /// then marks matching methods in classes that extend/implement them.
    ///
    /// If a resolver is provided, it will be used to look up interface methods
    /// from other files when not found locally.
    fn mark_interface_implementations(
        symbols: &mut Vec<Symbol>,
        resolver: Option<&dyn InterfaceResolver>,
        current_file: &str,
    ) {
        use std::collections::{HashMap, HashSet};

        // First pass: collect method names from interfaces and classes in this file
        // (these could be parent classes that get extended)
        let mut type_methods: HashMap<String, HashSet<String>> = HashMap::new();

        fn collect_type_methods(
            symbols: &[Symbol],
            type_methods: &mut HashMap<String, HashSet<String>>,
        ) {
            for sym in symbols {
                if matches!(
                    sym.kind,
                    rhizome_moss_languages::SymbolKind::Interface
                        | rhizome_moss_languages::SymbolKind::Class
                ) {
                    let methods: HashSet<String> = sym
                        .children
                        .iter()
                        .filter(|c| {
                            matches!(
                                c.kind,
                                rhizome_moss_languages::SymbolKind::Method
                                    | rhizome_moss_languages::SymbolKind::Function
                            )
                        })
                        .map(|c| c.name.clone())
                        .collect();
                    if !methods.is_empty() {
                        type_methods.insert(sym.name.clone(), methods);
                    }
                }
                // Recurse into nested types
                collect_type_methods(&sym.children, type_methods);
            }
        }

        collect_type_methods(symbols, &mut type_methods);

        // Cache for cross-file resolved interfaces (avoid repeated lookups)
        let mut cross_file_cache: HashMap<String, Option<HashSet<String>>> = HashMap::new();

        // Second pass: mark methods in classes that implement/extend
        fn mark_methods(
            symbols: &mut [Symbol],
            type_methods: &HashMap<String, HashSet<String>>,
            resolver: Option<&dyn InterfaceResolver>,
            current_file: &str,
            cross_file_cache: &mut HashMap<String, Option<HashSet<String>>>,
        ) {
            for sym in symbols.iter_mut() {
                if !sym.implements.is_empty() {
                    // Collect all method names from all implemented interfaces/parents
                    let mut interface_methods: HashSet<String> = HashSet::new();

                    for parent_name in &sym.implements {
                        // Try same-file first
                        if let Some(methods) = type_methods.get(parent_name) {
                            interface_methods.extend(methods.clone());
                        } else if let Some(resolver) = resolver {
                            // Try cross-file resolution with caching
                            let cached = cross_file_cache
                                .entry(parent_name.clone())
                                .or_insert_with(|| {
                                    resolver
                                        .resolve_interface_methods(parent_name, current_file)
                                        .map(|v| v.into_iter().collect())
                                });
                            if let Some(methods) = cached {
                                interface_methods.extend(methods.clone());
                            }
                        }
                    }

                    // Mark matching methods
                    for child in &mut sym.children {
                        if matches!(
                            child.kind,
                            rhizome_moss_languages::SymbolKind::Method
                                | rhizome_moss_languages::SymbolKind::Function
                        ) && interface_methods.contains(&child.name)
                        {
                            child.is_interface_impl = true;
                        }
                    }
                }
                // Recurse
                mark_methods(
                    &mut sym.children,
                    type_methods,
                    resolver,
                    current_file,
                    cross_file_cache,
                );
            }
        }

        mark_methods(
            symbols,
            &type_methods,
            resolver,
            current_file,
            &mut cross_file_cache,
        );
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

    #[test]
    fn test_typescript_interface_impl_detection() {
        let extractor = Extractor::new();
        let content = r#"
interface IFoo {
  bar(): void;
  baz(): number;
}

class Foo implements IFoo {
  bar() {}
  baz() { return 1; }
  other() {}
}
"#;
        let result = extractor.extract(&PathBuf::from("test.ts"), content);

        // Should have interface and class
        assert_eq!(result.symbols.len(), 2);

        let interface = &result.symbols[0];
        assert_eq!(interface.name, "IFoo");
        assert_eq!(interface.children.len(), 2); // bar, baz

        let class = &result.symbols[1];
        assert_eq!(class.name, "Foo");
        assert_eq!(class.implements, vec!["IFoo"]);
        assert_eq!(class.children.len(), 3); // bar, baz, other

        // bar and baz should be marked as interface implementations
        let bar = class.children.iter().find(|c| c.name == "bar").unwrap();
        let baz = class.children.iter().find(|c| c.name == "baz").unwrap();
        let other = class.children.iter().find(|c| c.name == "other").unwrap();

        assert!(
            bar.is_interface_impl,
            "bar should be marked as interface impl"
        );
        assert!(
            baz.is_interface_impl,
            "baz should be marked as interface impl"
        );
        assert!(
            !other.is_interface_impl,
            "other should NOT be marked as interface impl"
        );
    }

    #[test]
    fn test_cross_file_interface_impl_with_mock_resolver() {
        // Mock resolver that returns methods for IRemote interface
        struct MockResolver;
        impl InterfaceResolver for MockResolver {
            fn resolve_interface_methods(
                &self,
                name: &str,
                _current_file: &str,
            ) -> Option<Vec<String>> {
                if name == "IRemote" {
                    Some(vec![
                        "remoteMethod".to_string(),
                        "anotherRemote".to_string(),
                    ])
                } else {
                    None
                }
            }
        }

        let extractor = Extractor::new();
        // Class implements IRemote which is NOT in this file
        let content = r#"
class Foo implements IRemote {
  remoteMethod() {}
  anotherRemote() { return 1; }
  localMethod() {}
}
"#;
        let resolver = MockResolver;
        let result =
            extractor.extract_with_resolver(&PathBuf::from("test.ts"), content, Some(&resolver));

        assert_eq!(result.symbols.len(), 1);

        let class = &result.symbols[0];
        assert_eq!(class.name, "Foo");
        assert_eq!(class.implements, vec!["IRemote"]);
        assert_eq!(class.children.len(), 3);

        // remoteMethod and anotherRemote should be marked as interface implementations
        let remote_method = class
            .children
            .iter()
            .find(|c| c.name == "remoteMethod")
            .unwrap();
        let another_remote = class
            .children
            .iter()
            .find(|c| c.name == "anotherRemote")
            .unwrap();
        let local_method = class
            .children
            .iter()
            .find(|c| c.name == "localMethod")
            .unwrap();

        assert!(
            remote_method.is_interface_impl,
            "remoteMethod should be marked as interface impl"
        );
        assert!(
            another_remote.is_interface_impl,
            "anotherRemote should be marked as interface impl"
        );
        assert!(
            !local_method.is_interface_impl,
            "localMethod should NOT be marked as interface impl"
        );
    }

    #[test]
    fn test_cross_file_resolver_not_found() {
        // Mock resolver that returns None (interface not found)
        struct NotFoundResolver;
        impl InterfaceResolver for NotFoundResolver {
            fn resolve_interface_methods(
                &self,
                _name: &str,
                _current_file: &str,
            ) -> Option<Vec<String>> {
                None
            }
        }

        let extractor = Extractor::new();
        let content = r#"
class Foo implements IUnknown {
  someMethod() {}
}
"#;
        let resolver = NotFoundResolver;
        let result =
            extractor.extract_with_resolver(&PathBuf::from("test.ts"), content, Some(&resolver));

        let class = &result.symbols[0];
        // When interface is not found, methods should NOT be marked as interface impl
        let some_method = class
            .children
            .iter()
            .find(|c| c.name == "someMethod")
            .unwrap();
        assert!(
            !some_method.is_interface_impl,
            "someMethod should NOT be marked when interface not found"
        );
    }
}
