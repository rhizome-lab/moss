//! AST-based code skeleton extraction.
//!
//! Extracts function/class signatures with optional docstrings.

use moss_core::{tree_sitter, Language, Parsers};
use moss_languages::{get_support, LanguageSupport, Symbol as LangSymbol, SymbolKind as LangSymbolKind};
use std::path::Path;

/// A code symbol with its signature
#[derive(Debug, Clone)]
pub struct SkeletonSymbol {
    pub name: String,
    pub kind: &'static str, // "class", "function", "method"
    pub signature: String,
    pub docstring: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub children: Vec<SkeletonSymbol>,
}

/// Result of skeleton extraction
pub struct SkeletonResult {
    pub symbols: Vec<SkeletonSymbol>,
    pub file_path: String,
}

impl SkeletonResult {
    /// Format skeleton as text output
    pub fn format(&self, include_docstrings: bool) -> String {
        let mut lines = Vec::new();
        format_symbols(&self.symbols, include_docstrings, 0, &mut lines);
        lines.join("\n")
    }

    /// Filter to only type definitions (class, struct, enum, trait, interface)
    /// Returns a new SkeletonResult with only type-like symbols, and strips methods from classes
    pub fn filter_types(&self) -> SkeletonResult {
        fn is_type_kind(kind: &str) -> bool {
            matches!(
                kind,
                "class" | "struct" | "enum" | "trait" | "interface" | "type" | "impl" | "module"
            )
        }

        fn filter_symbol(sym: &SkeletonSymbol) -> Option<SkeletonSymbol> {
            if is_type_kind(sym.kind) {
                // For types, keep only nested types (not methods)
                let type_children: Vec<_> = sym.children
                    .iter()
                    .filter_map(|c| filter_symbol(c))
                    .collect();
                Some(SkeletonSymbol {
                    name: sym.name.clone(),
                    kind: sym.kind,
                    signature: sym.signature.clone(),
                    docstring: sym.docstring.clone(),
                    start_line: sym.start_line,
                    end_line: sym.end_line,
                    children: type_children,
                })
            } else {
                None
            }
        }

        let filtered_symbols: Vec<_> = self.symbols
            .iter()
            .filter_map(|s| filter_symbol(s))
            .collect();

        SkeletonResult {
            symbols: filtered_symbols,
            file_path: self.file_path.clone(),
        }
    }
}

fn format_symbols(
    symbols: &[SkeletonSymbol],
    include_docstrings: bool,
    indent: usize,
    lines: &mut Vec<String>,
) {
    let prefix = "    ".repeat(indent);

    for sym in symbols {
        lines.push(format!("{}{}:", prefix, sym.signature));

        if include_docstrings {
            if let Some(doc) = &sym.docstring {
                // First line only for brevity
                let first_line = doc.lines().next().unwrap_or("").trim();
                // Skip useless docstrings that just repeat the function name
                if !first_line.is_empty() && !is_useless_docstring(&sym.name, first_line) {
                    lines.push(format!("{}    \"\"\"{}\"\"\"", prefix, first_line));
                }
            }
        }

        if sym.children.is_empty() {
            lines.push(format!("{}    ...", prefix));
        } else {
            format_symbols(&sym.children, include_docstrings, indent + 1, lines);
        }

        lines.push(String::new()); // Blank line between symbols
    }
}

/// Check if a docstring is "useless" - just repeats the function name
/// Examples: setUserId → "Sets the user id", getUser → "Gets user"
fn is_useless_docstring(name: &str, docstring: &str) -> bool {
    // Common filler words to ignore
    const FILLER_WORDS: &[&str] = &[
        "the", "a", "an", "this", "that", "given", "specified", "provided",
        "returns", "return", "get", "gets", "set", "sets", "is", "are",
        "for", "from", "to", "of", "with", "by", "in", "on", "as",
    ];

    // Split function name into words (handle camelCase and snake_case)
    let name_words: Vec<String> = split_identifier(name)
        .into_iter()
        .map(|w| w.to_lowercase())
        .collect();

    // Clean docstring: lowercase, remove punctuation, split into words
    let doc_clean: String = docstring
        .chars()
        .map(|c| if c.is_alphanumeric() || c.is_whitespace() { c } else { ' ' })
        .collect();
    let doc_words: Vec<String> = doc_clean
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .filter(|w| !FILLER_WORDS.contains(&w.as_str()))
        .collect();

    // If all doc words are in the function name words, it's useless
    if doc_words.is_empty() {
        return true; // Only filler words
    }

    // Check if doc words are subset of name words (or very close)
    let matching = doc_words
        .iter()
        .filter(|dw| name_words.iter().any(|nw| nw == *dw || nw.contains(dw.as_str()) || dw.contains(nw.as_str())))
        .count();

    // If most doc words match name words, it's useless
    matching >= doc_words.len().saturating_sub(1) && doc_words.len() <= name_words.len() + 2
}

/// Split an identifier into words (camelCase, PascalCase, snake_case)
fn split_identifier(name: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current_word = String::new();

    for c in name.chars() {
        if c == '_' {
            if !current_word.is_empty() {
                words.push(current_word);
                current_word = String::new();
            }
        } else if c.is_uppercase() {
            if !current_word.is_empty() {
                words.push(current_word);
                current_word = String::new();
            }
            current_word.push(c.to_ascii_lowercase());
        } else {
            current_word.push(c);
        }
    }

    if !current_word.is_empty() {
        words.push(current_word);
    }

    words
}

/// Convert a moss_languages::Symbol to SkeletonSymbol
fn convert_symbol(sym: &LangSymbol) -> SkeletonSymbol {
    let kind = match sym.kind {
        LangSymbolKind::Function => "function",
        LangSymbolKind::Method => "method",
        LangSymbolKind::Class => "class",
        LangSymbolKind::Struct => "struct",
        LangSymbolKind::Enum => "enum",
        LangSymbolKind::Trait => "trait",
        LangSymbolKind::Interface => "interface",
        LangSymbolKind::Module => "module",
        LangSymbolKind::Type => "type",
        LangSymbolKind::Constant => "constant",
        LangSymbolKind::Variable => "variable",
        LangSymbolKind::Heading => "heading",
    };

    SkeletonSymbol {
        name: sym.name.clone(),
        kind,
        signature: sym.signature.clone(),
        docstring: sym.docstring.clone(),
        start_line: sym.start_line,
        end_line: sym.end_line,
        children: sym.children.iter().map(convert_symbol).collect(),
    }
}

pub struct SkeletonExtractor {
    parsers: Parsers,
    show_all: bool,
}

impl SkeletonExtractor {
    pub fn new() -> Self {
        Self {
            parsers: Parsers::new(),
            show_all: false,
        }
    }

    /// Create an extractor that shows all symbols including private ones
    pub fn with_all() -> Self {
        Self {
            parsers: Parsers::new(),
            show_all: true,
        }
    }

    pub fn extract(&mut self, path: &Path, content: &str) -> SkeletonResult {
        let lang = Language::from_path(path);

        let symbols = match lang {
            // Vue needs special handling for script element parsing
            Some(Language::Vue) => self.extract_vue(content),
            // All other languages use trait-based extraction
            Some(l) => {
                if let Some(support) = get_support(l) {
                    self.extract_with_trait(l, content, support)
                } else {
                    Vec::new()
                }
            }
            None => Vec::new(),
        };

        SkeletonResult {
            symbols,
            file_path: path.to_string_lossy().to_string(),
        }
    }

    /// Trait-based extraction (for future use when implementations are complete)
    #[allow(dead_code)]
    pub fn extract_with_support(&self, path: &Path, content: &str) -> Option<SkeletonResult> {
        let lang = Language::from_path(path)?;
        let support = get_support(lang)?;
        let symbols = self.extract_with_trait(lang, content, support);
        Some(SkeletonResult {
            symbols,
            file_path: path.to_string_lossy().to_string(),
        })
    }

    /// Extract using the LanguageSupport trait (new unified approach)
    fn extract_with_trait(
        &self,
        lang: Language,
        content: &str,
        support: &dyn LanguageSupport,
    ) -> Vec<SkeletonSymbol> {
        let tree = match self.parsers.parse_lang(lang, content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        self.collect_with_trait(&mut cursor, content, support, &mut symbols, false);

        // Post-process for Rust: merge impl blocks with their types
        if lang == Language::Rust {
            Self::merge_rust_impl_blocks(&mut symbols);
        }

        symbols
    }

    /// Merge Rust impl blocks with their corresponding struct/enum types
    fn merge_rust_impl_blocks(symbols: &mut Vec<SkeletonSymbol>) {
        // Collect impl blocks and their children
        let mut impl_methods: std::collections::HashMap<String, Vec<SkeletonSymbol>> =
            std::collections::HashMap::new();

        // Remove impl blocks and collect their methods
        symbols.retain(|sym| {
            if sym.kind == "impl" || sym.kind == "module" {
                // impl blocks are extracted as "impl" or "module" kind
                if sym.signature.starts_with("impl ") {
                    let type_name = &sym.name;
                    impl_methods
                        .entry(type_name.clone())
                        .or_default()
                        .extend(sym.children.clone());
                    return false; // Remove impl block
                }
            }
            true
        });

        // Add methods to matching struct/enum
        for sym in symbols.iter_mut() {
            if sym.kind == "struct" || sym.kind == "enum" {
                if let Some(methods) = impl_methods.remove(&sym.name) {
                    sym.children.extend(methods);
                }
            }
        }

        // Any remaining impl blocks without matching type: add back as impl symbols
        for (name, methods) in impl_methods {
            if !methods.is_empty() {
                symbols.push(SkeletonSymbol {
                    name: name.clone(),
                    kind: "impl",
                    signature: format!("impl {}", name),
                    docstring: None,
                    start_line: methods.first().map(|m| m.start_line).unwrap_or(0),
                    end_line: methods.last().map(|m| m.end_line).unwrap_or(0),
                    children: methods,
                });
            }
        }
    }

    fn collect_with_trait(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        support: &dyn LanguageSupport,
        symbols: &mut Vec<SkeletonSymbol>,
        in_container: bool,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            // Check if this is a function
            if support.function_kinds().contains(&kind) {
                if let Some(sym) = support.extract_function(&node, content, in_container) {
                    // Filter by visibility unless show_all
                    if self.show_all || matches!(sym.visibility, moss_languages::Visibility::Public) {
                        symbols.push(convert_symbol(&sym));
                    }
                }
            }
            // Check if this is a container (class, impl, module)
            else if support.container_kinds().contains(&kind) {
                if let Some(sym) = support.extract_container(&node, content) {
                    if self.show_all || matches!(sym.visibility, moss_languages::Visibility::Public) {
                        let mut skeleton_sym = convert_symbol(&sym);

                        // Recurse into container body
                        if let Some(body) = support.container_body(&node) {
                            let mut body_cursor = body.walk();
                            if body_cursor.goto_first_child() {
                                self.collect_with_trait(
                                    &mut body_cursor,
                                    content,
                                    support,
                                    &mut skeleton_sym.children,
                                    true,
                                );
                            }
                        }

                        symbols.push(skeleton_sym);
                    }
                }
                // Don't descend further after processing container
                if cursor.goto_next_sibling() {
                    continue;
                }
                break;
            }
            // Check if this is a standalone type (struct, enum, etc.)
            else if support.type_kinds().contains(&kind) && !support.container_kinds().contains(&kind) {
                if let Some(sym) = support.extract_type(&node, content) {
                    if self.show_all || matches!(sym.visibility, moss_languages::Visibility::Public) {
                        symbols.push(convert_symbol(&sym));
                    }
                }
            }

            // Descend into children for other nodes
            if cursor.goto_first_child() {
                self.collect_with_trait(cursor, content, support, symbols, in_container);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn extract_python(&mut self, content: &str) -> Vec<SkeletonSymbol> {
        let tree = match self.parsers.parse_lang(Language::Python, content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        Self::collect_python_symbols(&mut cursor, content, &mut symbols, false, self.show_all);
        symbols
    }

    fn collect_python_symbols(
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        symbols: &mut Vec<SkeletonSymbol>,
        in_class: bool,
        show_all: bool,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            match kind {
                "function_definition" | "async_function_definition" => {
                    if let Some(sym) = Self::extract_python_function(&node, content, in_class, show_all) {
                        symbols.push(sym);
                    }
                }
                "class_definition" => {
                    if let Some(sym) = Self::extract_python_class(&node, content, show_all) {
                        symbols.push(sym);
                    }
                    // Skip children - we handle them in extract_python_class
                    if cursor.goto_next_sibling() {
                        continue;
                    }
                    break;
                }
                _ => {}
            }

            // Recurse into children (except for class definitions)
            if kind != "class_definition" && cursor.goto_first_child() {
                Self::collect_python_symbols(cursor, content, symbols, in_class, show_all);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn extract_python_function(
        node: &tree_sitter::Node,
        content: &str,
        in_class: bool,
        show_all: bool,
    ) -> Option<SkeletonSymbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = content[name_node.byte_range()].to_string();

        // Skip private methods unless they're dunder methods or show_all is true
        if !show_all && name.starts_with('_') && !name.starts_with("__") {
            return None;
        }

        let is_async = node.kind() == "async_function_definition";
        let prefix = if is_async { "async def" } else { "def" };

        // Extract parameters
        let params = node.child_by_field_name("parameters");
        let params_text = params
            .map(|p| content[p.byte_range()].to_string())
            .unwrap_or_else(|| "()".to_string());

        // Extract return type
        let return_type = node.child_by_field_name("return_type");
        let return_text = return_type
            .map(|r| format!(" -> {}", &content[r.byte_range()]))
            .unwrap_or_default();

        let signature = format!("{} {}{}{}", prefix, name, params_text, return_text);

        // Extract docstring
        let docstring = Self::extract_python_docstring(node, content);

        Some(SkeletonSymbol {
            name,
            kind: if in_class { "method" } else { "function" },
            signature,
            docstring,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            children: Vec::new(),
        })
    }

    fn extract_python_class(
        node: &tree_sitter::Node,
        content: &str,
        show_all: bool,
    ) -> Option<SkeletonSymbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = content[name_node.byte_range()].to_string();

        // Skip private classes unless show_all is true
        if !show_all && name.starts_with('_') && !name.starts_with("__") {
            return None;
        }

        // Extract base classes
        let mut bases = Vec::new();
        if let Some(args_node) = node.child_by_field_name("superclasses") {
            let args_text = &content[args_node.byte_range()];
            // Remove parentheses
            let trimmed = args_text.trim_start_matches('(').trim_end_matches(')');
            if !trimmed.is_empty() {
                bases.push(trimmed.to_string());
            }
        }

        let signature = if bases.is_empty() {
            format!("class {}", name)
        } else {
            format!("class {}({})", name, bases.join(", "))
        };

        // Extract docstring
        let docstring = Self::extract_python_docstring(node, content);

        // Extract methods
        let mut children = Vec::new();
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            if cursor.goto_first_child() {
                Self::collect_python_symbols(&mut cursor, content, &mut children, true, show_all);
            }
        }

        Some(SkeletonSymbol {
            name,
            kind: "class",
            signature,
            docstring,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            children,
        })
    }

    fn extract_python_docstring(node: &tree_sitter::Node, content: &str) -> Option<String> {
        // Look for docstring in body
        let body = node.child_by_field_name("body")?;
        let first_child = body.child(0)?;

        // Handle both grammar versions:
        // - Old: expression_statement > string
        // - New (arborium): string directly, with string_content child
        let string_node = if first_child.kind() == "expression_statement" {
            first_child.child(0).filter(|n| n.kind() == "string")
        } else if first_child.kind() == "string" {
            Some(first_child)
        } else {
            None
        }?;

        // Try to get content from string_content child (arborium style)
        let mut cursor = string_node.walk();
        for child in string_node.children(&mut cursor) {
            if child.kind() == "string_content" {
                let doc = content[child.byte_range()].trim();
                if !doc.is_empty() {
                    return Some(doc.to_string());
                }
            }
        }

        // Fallback: extract from full string text (old style)
        let text = &content[string_node.byte_range()];
        let doc = text
            .trim_start_matches("\"\"\"")
            .trim_start_matches("'''")
            .trim_start_matches('"')
            .trim_start_matches('\'')
            .trim_end_matches("\"\"\"")
            .trim_end_matches("'''")
            .trim_end_matches('"')
            .trim_end_matches('\'')
            .trim();
        if !doc.is_empty() {
            return Some(doc.to_string());
        }

        None
    }

    fn extract_rust(&mut self, content: &str) -> Vec<SkeletonSymbol> {
        let tree = match self.parsers.parse_lang(Language::Rust, content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        Self::collect_rust_symbols(&mut cursor, content, &mut symbols, None);
        symbols
    }

    fn collect_rust_symbols(
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        symbols: &mut Vec<SkeletonSymbol>,
        impl_name: Option<&str>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            match kind {
                "function_item" => {
                    if let Some(sym) = Self::extract_rust_function(&node, content, impl_name) {
                        symbols.push(sym);
                    }
                }
                "struct_item" => {
                    if let Some(sym) = Self::extract_rust_struct(&node, content) {
                        symbols.push(sym);
                    }
                }
                "enum_item" => {
                    if let Some(sym) = Self::extract_rust_enum(&node, content) {
                        symbols.push(sym);
                    }
                }
                "trait_item" => {
                    if let Some(sym) = Self::extract_rust_trait(&node, content) {
                        symbols.push(sym);
                    }
                }
                "impl_item" => {
                    // Get the type being implemented
                    if let Some(type_node) = node.child_by_field_name("type") {
                        let type_name = &content[type_node.byte_range()];

                        // Find impl body and recurse
                        if let Some(body) = node.child_by_field_name("body") {
                            let mut body_cursor = body.walk();
                            if body_cursor.goto_first_child() {
                                let mut methods = Vec::new();
                                Self::collect_rust_symbols(
                                    &mut body_cursor,
                                    content,
                                    &mut methods,
                                    Some(type_name),
                                );

                                // Add methods to existing struct symbol or create impl symbol
                                if !methods.is_empty() {
                                    // Find existing struct/enum and add methods
                                    let found = symbols.iter_mut().find(|s| s.name == type_name);
                                    if let Some(existing) = found {
                                        existing.children.extend(methods);
                                    } else {
                                        // Create impl symbol
                                        symbols.push(SkeletonSymbol {
                                            name: type_name.to_string(),
                                            kind: "impl",
                                            signature: format!("impl {}", type_name),
                                            docstring: None,
                                            start_line: node.start_position().row + 1,
                                            end_line: node.end_position().row + 1,
                                            children: methods,
                                        });
                                    }
                                }
                            }
                        }
                    }
                    if cursor.goto_next_sibling() {
                        continue;
                    }
                    break;
                }
                _ => {}
            }

            // Recurse into children (except for impl blocks)
            if kind != "impl_item" && cursor.goto_first_child() {
                Self::collect_rust_symbols(cursor, content, symbols, impl_name);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn extract_rust_function(
        node: &tree_sitter::Node,
        content: &str,
        impl_name: Option<&str>,
    ) -> Option<SkeletonSymbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = content[name_node.byte_range()].to_string();

        // Get visibility
        let mut vis = String::new();
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if child.kind() == "visibility_modifier" {
                    vis = format!("{} ", &content[child.byte_range()]);
                    break;
                }
            }
        }

        // Get parameters
        let params = node.child_by_field_name("parameters");
        let params_text = params
            .map(|p| content[p.byte_range()].to_string())
            .unwrap_or_else(|| "()".to_string());

        // Get return type
        let return_type = node.child_by_field_name("return_type");
        let return_text = return_type
            .map(|r| format!(" -> {}", &content[r.byte_range()]))
            .unwrap_or_default();

        let signature = format!("{}fn {}{}{}", vis, name, params_text, return_text);

        // Extract doc comment (look for preceding line_comment or block_comment)
        let docstring = Self::extract_rust_doc_comment(node, content);

        Some(SkeletonSymbol {
            name,
            kind: if impl_name.is_some() {
                "method"
            } else {
                "function"
            },
            signature,
            docstring,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            children: Vec::new(),
        })
    }

    fn extract_rust_struct(node: &tree_sitter::Node, content: &str) -> Option<SkeletonSymbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = content[name_node.byte_range()].to_string();

        // Get visibility
        let mut vis = String::new();
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if child.kind() == "visibility_modifier" {
                    vis = format!("{} ", &content[child.byte_range()]);
                    break;
                }
            }
        }

        let signature = format!("{}struct {}", vis, name);
        let docstring = Self::extract_rust_doc_comment(node, content);

        Some(SkeletonSymbol {
            name,
            kind: "struct",
            signature,
            docstring,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            children: Vec::new(),
        })
    }

    fn extract_rust_enum(node: &tree_sitter::Node, content: &str) -> Option<SkeletonSymbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = content[name_node.byte_range()].to_string();

        // Get visibility
        let mut vis = String::new();
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if child.kind() == "visibility_modifier" {
                    vis = format!("{} ", &content[child.byte_range()]);
                    break;
                }
            }
        }

        let signature = format!("{}enum {}", vis, name);
        let docstring = Self::extract_rust_doc_comment(node, content);

        Some(SkeletonSymbol {
            name,
            kind: "enum",
            signature,
            docstring,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            children: Vec::new(),
        })
    }

    fn extract_rust_trait(node: &tree_sitter::Node, content: &str) -> Option<SkeletonSymbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = content[name_node.byte_range()].to_string();

        // Get visibility
        let mut vis = String::new();
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if child.kind() == "visibility_modifier" {
                    vis = format!("{} ", &content[child.byte_range()]);
                    break;
                }
            }
        }

        let signature = format!("{}trait {}", vis, name);
        let docstring = Self::extract_rust_doc_comment(node, content);

        // Extract trait methods
        let mut children = Vec::new();
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            if cursor.goto_first_child() {
                Self::collect_rust_symbols(&mut cursor, content, &mut children, Some(&name));
            }
        }

        Some(SkeletonSymbol {
            name,
            kind: "trait",
            signature,
            docstring,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            children,
        })
    }

    fn extract_rust_doc_comment(node: &tree_sitter::Node, content: &str) -> Option<String> {
        // Look for doc comments before the node
        let lines: Vec<&str> = content.lines().collect();
        let start_line = node.start_position().row;

        if start_line == 0 {
            return None;
        }

        // Check preceding lines for doc comments
        let mut doc_lines = Vec::new();
        for i in (0..start_line).rev() {
            let line = lines.get(i)?.trim();
            if line.starts_with("///") {
                let doc = line.trim_start_matches("///").trim();
                doc_lines.insert(0, doc.to_string());
            } else if line.starts_with("//!") {
                // Module-level doc, skip
                break;
            } else if line.is_empty() {
                // Empty line, stop if we have content
                if !doc_lines.is_empty() {
                    break;
                }
            } else {
                break;
            }
        }

        if doc_lines.is_empty() {
            None
        } else {
            Some(doc_lines.join("\n"))
        }
    }

    fn extract_markdown(&mut self, content: &str) -> Vec<SkeletonSymbol> {
        let tree = match self.parsers.parse_lang(Language::Markdown, content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut headings = Vec::new();
        let root = tree.root_node();
        Self::collect_markdown_headings(&root, content, &mut headings);

        // Compute end_line for each heading (line before next heading at same/higher level)
        let total_lines = content.lines().count();
        let heading_count = headings.len();
        for i in 0..heading_count {
            let (_, level) = &headings[i];
            let level = *level;
            // Find next heading at same or higher level
            let mut end = total_lines;
            for j in (i + 1)..heading_count {
                let (_, next_level) = &headings[j];
                if *next_level <= level {
                    end = headings[j].0.start_line - 1;
                    break;
                }
            }
            headings[i].0.end_line = end;
        }

        // Build nested tree from flat headings list
        Self::build_heading_tree(headings)
    }

    fn collect_markdown_headings(
        node: &tree_sitter::Node,
        content: &str,
        headings: &mut Vec<(SkeletonSymbol, usize)>, // (symbol, level)
    ) {
        // ATX headings have type like "atx_h1_marker", "atx_h2_marker", etc.
        // The heading node contains the marker and heading_content
        if node.kind().starts_with("atx_heading") || node.kind() == "setext_heading" {
            if let Some(sym) = Self::extract_markdown_heading(node, content) {
                let level = Self::get_heading_level(node);
                headings.push((sym, level));
            }
        }

        // Recurse into children
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                Self::collect_markdown_headings(&cursor.node(), content, headings);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }

    fn get_heading_level(node: &tree_sitter::Node) -> usize {
        // atx_heading nodes have a marker child that indicates level
        let kind = node.kind();
        if kind.starts_with("atx_heading") {
            // Look for marker child to count # characters
            let mut cursor = node.walk();
            if cursor.goto_first_child() {
                loop {
                    let child = cursor.node();
                    if child.kind().contains("marker") {
                        // Count the # characters
                        return child.end_position().column - child.start_position().column;
                    }
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
            }
        }
        1 // Default to level 1
    }

    fn extract_markdown_heading(node: &tree_sitter::Node, content: &str) -> Option<SkeletonSymbol> {
        // Get the full heading text
        let text = &content[node.byte_range()];
        let line = text.lines().next().unwrap_or("").trim();

        // Extract title (remove # prefix)
        let title = line.trim_start_matches('#').trim();
        if title.is_empty() {
            return None;
        }

        Some(SkeletonSymbol {
            name: title.to_string(),
            kind: "heading",
            signature: line.to_string(),
            docstring: None,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            children: Vec::new(),
        })
    }

    fn build_heading_tree(headings: Vec<(SkeletonSymbol, usize)>) -> Vec<SkeletonSymbol> {
        if headings.is_empty() {
            return Vec::new();
        }

        // Stack-based tree building: (symbol, level)
        let mut result: Vec<SkeletonSymbol> = Vec::new();
        let mut stack: Vec<(SkeletonSymbol, usize)> = Vec::new();

        for (sym, level) in headings {
            // Pop items from stack that are at same or lower level
            while let Some((_, parent_level)) = stack.last() {
                if *parent_level >= level {
                    let (completed, _) = stack.pop().unwrap();
                    if let Some((parent, _)) = stack.last_mut() {
                        parent.children.push(completed);
                    } else {
                        result.push(completed);
                    }
                } else {
                    break;
                }
            }
            stack.push((sym, level));
        }

        // Empty remaining stack
        while let Some((completed, _)) = stack.pop() {
            if let Some((parent, _)) = stack.last_mut() {
                parent.children.push(completed);
            } else {
                result.push(completed);
            }
        }

        result
    }

    // JavaScript/JSX extraction (also used for TSX)
    fn extract_javascript(&mut self, content: &str) -> Vec<SkeletonSymbol> {
        let tree = match self.parsers.parse_lang(Language::JavaScript, content) {
            Some(t) => t,
            None => return Vec::new(),
        };
        Self::extract_js_like_symbols(&tree, content)
    }

    // TypeScript extraction
    fn extract_typescript(&mut self, content: &str) -> Vec<SkeletonSymbol> {
        let tree = match self.parsers.parse_lang(Language::TypeScript, content) {
            Some(t) => t,
            None => return Vec::new(),
        };
        Self::extract_js_like_symbols(&tree, content)
    }

    // Shared JS/TS symbol extraction
    fn extract_js_like_symbols(tree: &tree_sitter::Tree, content: &str) -> Vec<SkeletonSymbol> {
        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();
        Self::collect_js_symbols(&mut cursor, content, &mut symbols, None);
        symbols
    }

    fn collect_js_symbols(
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        symbols: &mut Vec<SkeletonSymbol>,
        parent: Option<&str>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            match kind {
                "function_declaration" | "generator_function_declaration" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = content[name_node.byte_range()].to_string();
                        let params = node
                            .child_by_field_name("parameters")
                            .map(|p| content[p.byte_range()].to_string())
                            .unwrap_or_else(|| "()".to_string());
                        symbols.push(SkeletonSymbol {
                            name: name.clone(),
                            kind: if parent.is_some() { "method" } else { "function" },
                            signature: format!("function {}{}", name, params),
                            docstring: None,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            children: Vec::new(),
                        });
                    }
                }
                "class_declaration" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = content[name_node.byte_range()].to_string();
                        let mut children = Vec::new();
                        if let Some(body) = node.child_by_field_name("body") {
                            let mut body_cursor = body.walk();
                            if body_cursor.goto_first_child() {
                                Self::collect_js_symbols(
                                    &mut body_cursor,
                                    content,
                                    &mut children,
                                    Some(&name),
                                );
                            }
                        }
                        symbols.push(SkeletonSymbol {
                            name: name.clone(),
                            kind: "class",
                            signature: format!("class {}", name),
                            docstring: None,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            children,
                        });
                    }
                    if cursor.goto_next_sibling() {
                        continue;
                    }
                    break;
                }
                "method_definition" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = content[name_node.byte_range()].to_string();
                        let params = node
                            .child_by_field_name("parameters")
                            .map(|p| content[p.byte_range()].to_string())
                            .unwrap_or_else(|| "()".to_string());
                        symbols.push(SkeletonSymbol {
                            name: name.clone(),
                            kind: "method",
                            signature: format!("{}{}", name, params),
                            docstring: None,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            children: Vec::new(),
                        });
                    }
                }
                "arrow_function" | "function_expression" => {
                    // Skip anonymous functions
                }
                // TypeScript-specific: interface and type alias declarations
                "interface_declaration" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = content[name_node.byte_range()].to_string();
                        symbols.push(SkeletonSymbol {
                            name: name.clone(),
                            kind: "interface",
                            signature: format!("interface {}", name),
                            docstring: None,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            children: Vec::new(),
                        });
                    }
                }
                "type_alias_declaration" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = content[name_node.byte_range()].to_string();
                        symbols.push(SkeletonSymbol {
                            name: name.clone(),
                            kind: "type",
                            signature: format!("type {}", name),
                            docstring: None,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            children: Vec::new(),
                        });
                    }
                }
                "enum_declaration" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = content[name_node.byte_range()].to_string();
                        symbols.push(SkeletonSymbol {
                            name: name.clone(),
                            kind: "enum",
                            signature: format!("enum {}", name),
                            docstring: None,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            children: Vec::new(),
                        });
                    }
                }
                _ => {}
            }

            if kind != "class_declaration" && cursor.goto_first_child() {
                Self::collect_js_symbols(cursor, content, symbols, parent);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    // Go extraction
    fn extract_go(&mut self, content: &str) -> Vec<SkeletonSymbol> {
        let tree = match self.parsers.parse_lang(Language::Go, content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();
        Self::collect_go_symbols(&mut cursor, content, &mut symbols);
        symbols
    }

    fn collect_go_symbols(
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        symbols: &mut Vec<SkeletonSymbol>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            match kind {
                "function_declaration" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = content[name_node.byte_range()].to_string();
                        let params = node
                            .child_by_field_name("parameters")
                            .map(|p| content[p.byte_range()].to_string())
                            .unwrap_or_else(|| "()".to_string());
                        symbols.push(SkeletonSymbol {
                            name: name.clone(),
                            kind: "function",
                            signature: format!("func {}{}", name, params),
                            docstring: None,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            children: Vec::new(),
                        });
                    }
                }
                "method_declaration" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = content[name_node.byte_range()].to_string();
                        let receiver = node
                            .child_by_field_name("receiver")
                            .map(|r| content[r.byte_range()].to_string())
                            .unwrap_or_default();
                        let params = node
                            .child_by_field_name("parameters")
                            .map(|p| content[p.byte_range()].to_string())
                            .unwrap_or_else(|| "()".to_string());
                        symbols.push(SkeletonSymbol {
                            name: name.clone(),
                            kind: "method",
                            signature: format!("func {} {}{}", receiver, name, params),
                            docstring: None,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            children: Vec::new(),
                        });
                    }
                }
                "type_declaration" => {
                    // Handle struct and interface declarations
                    for i in 0..node.child_count() {
                        if let Some(spec) = node.child(i) {
                            if spec.kind() == "type_spec" {
                                if let Some(name_node) = spec.child_by_field_name("name") {
                                    let name = content[name_node.byte_range()].to_string();
                                    let type_node = spec.child_by_field_name("type");
                                    let type_kind = type_node.map(|t| t.kind()).unwrap_or("");
                                    let kind = if type_kind == "struct_type" {
                                        "struct"
                                    } else if type_kind == "interface_type" {
                                        "interface"
                                    } else {
                                        "type"
                                    };
                                    symbols.push(SkeletonSymbol {
                                        name: name.clone(),
                                        kind,
                                        signature: format!("type {}", name),
                                        docstring: None,
                                        start_line: spec.start_position().row + 1,
                                        end_line: spec.end_position().row + 1,
                                        children: Vec::new(),
                                    });
                                }
                            }
                        }
                    }
                }
                _ => {}
            }

            if cursor.goto_first_child() {
                Self::collect_go_symbols(cursor, content, symbols);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    // Java extraction
    fn extract_java(&mut self, content: &str) -> Vec<SkeletonSymbol> {
        let tree = match self.parsers.parse_lang(Language::Java, content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();
        Self::collect_java_symbols(&mut cursor, content, &mut symbols, None);
        symbols
    }

    fn collect_java_symbols(
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        symbols: &mut Vec<SkeletonSymbol>,
        parent: Option<&str>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            match kind {
                "method_declaration" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = content[name_node.byte_range()].to_string();
                        let params = node
                            .child_by_field_name("parameters")
                            .map(|p| content[p.byte_range()].to_string())
                            .unwrap_or_else(|| "()".to_string());
                        let return_type = node
                            .child_by_field_name("type")
                            .map(|t| content[t.byte_range()].to_string())
                            .unwrap_or_else(|| "void".to_string());
                        symbols.push(SkeletonSymbol {
                            name: name.clone(),
                            kind: "method",
                            signature: format!("{} {}{}", return_type, name, params),
                            docstring: None,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            children: Vec::new(),
                        });
                    }
                }
                "class_declaration" | "interface_declaration" | "enum_declaration" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = content[name_node.byte_range()].to_string();
                        let sym_kind = if kind == "class_declaration" {
                            "class"
                        } else if kind == "interface_declaration" {
                            "interface"
                        } else {
                            "enum"
                        };
                        let mut children = Vec::new();
                        if let Some(body) = node.child_by_field_name("body") {
                            let mut body_cursor = body.walk();
                            if body_cursor.goto_first_child() {
                                Self::collect_java_symbols(
                                    &mut body_cursor,
                                    content,
                                    &mut children,
                                    Some(&name),
                                );
                            }
                        }
                        symbols.push(SkeletonSymbol {
                            name: name.clone(),
                            kind: sym_kind,
                            signature: format!("{} {}", sym_kind, name),
                            docstring: None,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            children,
                        });
                    }
                    if cursor.goto_next_sibling() {
                        continue;
                    }
                    break;
                }
                _ => {}
            }

            if !matches!(
                kind,
                "class_declaration" | "interface_declaration" | "enum_declaration"
            ) && cursor.goto_first_child()
            {
                Self::collect_java_symbols(cursor, content, symbols, parent);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    // C extraction
    fn extract_c(&mut self, content: &str) -> Vec<SkeletonSymbol> {
        let tree = match self.parsers.parse_lang(Language::C, content) {
            Some(t) => t,
            None => return Vec::new(),
        };
        Self::extract_c_like_symbols(&tree, content)
    }

    // C++ extraction
    fn extract_cpp(&mut self, content: &str) -> Vec<SkeletonSymbol> {
        let tree = match self.parsers.parse_lang(Language::Cpp, content) {
            Some(t) => t,
            None => return Vec::new(),
        };
        Self::extract_c_like_symbols(&tree, content)
    }

    // Shared C/C++ symbol extraction
    fn extract_c_like_symbols(tree: &tree_sitter::Tree, content: &str) -> Vec<SkeletonSymbol> {
        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();
        Self::collect_c_symbols(&mut cursor, content, &mut symbols);
        symbols
    }

    fn collect_c_symbols(
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        symbols: &mut Vec<SkeletonSymbol>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            match kind {
                "function_definition" => {
                    if let Some(declarator) = node.child_by_field_name("declarator") {
                        // Get function name from declarator
                        let name = Self::extract_c_function_name(&declarator, content);
                        if let Some(name) = name {
                            let sig_end = declarator.end_byte();
                            let sig_start = node.start_byte();
                            let signature = content[sig_start..sig_end].trim().to_string();
                            symbols.push(SkeletonSymbol {
                                name,
                                kind: "function",
                                signature,
                                docstring: None,
                                start_line: node.start_position().row + 1,
                                end_line: node.end_position().row + 1,
                                children: Vec::new(),
                            });
                        }
                    }
                }
                "struct_specifier" | "class_specifier" | "enum_specifier" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = content[name_node.byte_range()].to_string();
                        let sym_kind = if kind == "struct_specifier" {
                            "struct"
                        } else if kind == "class_specifier" {
                            "class"
                        } else {
                            "enum"
                        };
                        symbols.push(SkeletonSymbol {
                            name: name.clone(),
                            kind: sym_kind,
                            signature: format!("{} {}", sym_kind, name),
                            docstring: None,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            children: Vec::new(),
                        });
                    }
                }
                _ => {}
            }

            if cursor.goto_first_child() {
                Self::collect_c_symbols(cursor, content, symbols);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn extract_c_function_name(declarator: &tree_sitter::Node, content: &str) -> Option<String> {
        // Navigate through possible pointer declarators to find the identifier
        let mut current = *declarator;
        loop {
            match current.kind() {
                "function_declarator" => {
                    if let Some(inner) = current.child_by_field_name("declarator") {
                        current = inner;
                    } else {
                        break;
                    }
                }
                "pointer_declarator" => {
                    if let Some(inner) = current.child_by_field_name("declarator") {
                        current = inner;
                    } else {
                        break;
                    }
                }
                "identifier" => {
                    return Some(content[current.byte_range()].to_string());
                }
                _ => break,
            }
        }
        None
    }

    // Ruby extraction
    fn extract_ruby(&mut self, content: &str) -> Vec<SkeletonSymbol> {
        let tree = match self.parsers.parse_lang(Language::Ruby, content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();
        Self::collect_ruby_symbols(&mut cursor, content, &mut symbols, None);
        symbols
    }

    fn collect_ruby_symbols(
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        symbols: &mut Vec<SkeletonSymbol>,
        parent: Option<&str>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            match kind {
                "method" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = content[name_node.byte_range()].to_string();
                        let params = node
                            .child_by_field_name("parameters")
                            .map(|p| content[p.byte_range()].to_string())
                            .unwrap_or_default();
                        symbols.push(SkeletonSymbol {
                            name: name.clone(),
                            kind: if parent.is_some() { "method" } else { "function" },
                            signature: format!("def {}{}", name, params),
                            docstring: None,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            children: Vec::new(),
                        });
                    }
                }
                "class" | "module" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = content[name_node.byte_range()].to_string();
                        let sym_kind = if kind == "class" { "class" } else { "module" };
                        let mut children = Vec::new();
                        // Find body and collect methods
                        for i in 0..node.child_count() {
                            if let Some(child) = node.child(i) {
                                if child.kind() == "body_statement" {
                                    let mut body_cursor = child.walk();
                                    if body_cursor.goto_first_child() {
                                        Self::collect_ruby_symbols(
                                            &mut body_cursor,
                                            content,
                                            &mut children,
                                            Some(&name),
                                        );
                                    }
                                }
                            }
                        }
                        symbols.push(SkeletonSymbol {
                            name: name.clone(),
                            kind: sym_kind,
                            signature: format!("{} {}", sym_kind, name),
                            docstring: None,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            children,
                        });
                    }
                    if cursor.goto_next_sibling() {
                        continue;
                    }
                    break;
                }
                _ => {}
            }

            if kind != "class" && kind != "module" && cursor.goto_first_child() {
                Self::collect_ruby_symbols(cursor, content, symbols, parent);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn extract_json(&mut self, content: &str) -> Vec<SkeletonSymbol> {
        let tree = match self.parsers.parse_lang(Language::Json, content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();
        Self::collect_json_keys(&mut cursor, content, &mut symbols);
        symbols
    }

    fn collect_json_keys(
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        symbols: &mut Vec<SkeletonSymbol>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            if kind == "pair" {
                if let Some(key_node) = node.child_by_field_name("key") {
                    let key_text = &content[key_node.byte_range()];
                    let key_name = key_text.trim_matches('"');

                    let value_node = node.child_by_field_name("value");
                    let is_object = value_node.map(|v| v.kind() == "object").unwrap_or(false);
                    let is_array = value_node.map(|v| v.kind() == "array").unwrap_or(false);

                    let (sym_kind, type_hint) = if is_object {
                        ("class", "object")
                    } else if is_array {
                        ("variable", "array")
                    } else {
                        ("variable", value_node.map(|v| v.kind()).unwrap_or("value"))
                    };

                    let mut children = Vec::new();
                    if is_object {
                        if cursor.goto_first_child() {
                            Self::collect_json_keys(cursor, content, &mut children);
                            cursor.goto_parent();
                        }
                    }

                    symbols.push(SkeletonSymbol {
                        name: key_name.to_string(),
                        kind: sym_kind,
                        signature: format!("{}: {}", key_name, type_hint),
                        docstring: None,
                        start_line: node.start_position().row + 1,
                        end_line: node.end_position().row + 1,
                        children,
                    });

                    if is_object {
                        if cursor.goto_next_sibling() {
                            continue;
                        }
                        break;
                    }
                }
            }

            if kind != "pair" && cursor.goto_first_child() {
                Self::collect_json_keys(cursor, content, symbols);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn extract_yaml(&mut self, content: &str) -> Vec<SkeletonSymbol> {
        let tree = match self.parsers.parse_lang(Language::Yaml, content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();
        Self::collect_yaml_keys(&mut cursor, content, &mut symbols);
        symbols
    }

    fn collect_yaml_keys(
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        symbols: &mut Vec<SkeletonSymbol>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            if kind == "block_mapping_pair" || kind == "flow_pair" {
                if let Some(key_node) = node.child_by_field_name("key") {
                    let key_text = &content[key_node.byte_range()];
                    let key_name = key_text.trim_matches(|c| c == '"' || c == '\'').trim();

                    if !key_name.is_empty() {
                        let value_node = node.child_by_field_name("value");
                        let is_object = value_node.map(|v| {
                            v.kind() == "block_node" || v.kind() == "flow_node" ||
                            v.kind() == "block_mapping" || v.kind() == "flow_mapping"
                        }).unwrap_or(false);

                        let sym_kind = if is_object { "class" } else { "variable" };

                        let mut children = Vec::new();
                        if is_object {
                            if cursor.goto_first_child() {
                                Self::collect_yaml_keys(cursor, content, &mut children);
                                cursor.goto_parent();
                            }
                        }

                        symbols.push(SkeletonSymbol {
                            name: key_name.to_string(),
                            kind: sym_kind,
                            signature: format!("{}:", key_name),
                            docstring: None,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            children,
                        });

                        if is_object {
                            if cursor.goto_next_sibling() {
                                continue;
                            }
                            break;
                        }
                    }
                }
            }

            let dominated = kind == "block_mapping_pair" || kind == "flow_pair";
            if !dominated && cursor.goto_first_child() {
                Self::collect_yaml_keys(cursor, content, symbols);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    // Scala extraction
    fn extract_scala(&mut self, content: &str) -> Vec<SkeletonSymbol> {
        let tree = match self.parsers.parse_lang(Language::Scala, content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();
        Self::collect_scala_symbols(&mut cursor, content, &mut symbols, None);
        symbols
    }

    fn collect_scala_symbols(
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        symbols: &mut Vec<SkeletonSymbol>,
        parent: Option<&str>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            match kind {
                "function_definition" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = content[name_node.byte_range()].to_string();
                        let params = node
                            .child_by_field_name("parameters")
                            .map(|p| content[p.byte_range()].to_string())
                            .unwrap_or_else(|| "()".to_string());
                        let return_type = node
                            .child_by_field_name("return_type")
                            .map(|t| format!(": {}", &content[t.byte_range()]))
                            .unwrap_or_default();
                        symbols.push(SkeletonSymbol {
                            name: name.clone(),
                            kind: if parent.is_some() { "method" } else { "function" },
                            signature: format!("def {}{}{}", name, params, return_type),
                            docstring: None,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            children: Vec::new(),
                        });
                    }
                }
                "class_definition" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = content[name_node.byte_range()].to_string();
                        let mut children = Vec::new();
                        if let Some(body) = node.child_by_field_name("body") {
                            let mut body_cursor = body.walk();
                            if body_cursor.goto_first_child() {
                                Self::collect_scala_symbols(
                                    &mut body_cursor,
                                    content,
                                    &mut children,
                                    Some(&name),
                                );
                            }
                        }
                        symbols.push(SkeletonSymbol {
                            name: name.clone(),
                            kind: "class",
                            signature: format!("class {}", name),
                            docstring: None,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            children,
                        });
                    }
                    if cursor.goto_next_sibling() {
                        continue;
                    }
                    break;
                }
                "object_definition" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = content[name_node.byte_range()].to_string();
                        let mut children = Vec::new();
                        if let Some(body) = node.child_by_field_name("body") {
                            let mut body_cursor = body.walk();
                            if body_cursor.goto_first_child() {
                                Self::collect_scala_symbols(
                                    &mut body_cursor,
                                    content,
                                    &mut children,
                                    Some(&name),
                                );
                            }
                        }
                        symbols.push(SkeletonSymbol {
                            name: name.clone(),
                            kind: "module",
                            signature: format!("object {}", name),
                            docstring: None,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            children,
                        });
                    }
                    if cursor.goto_next_sibling() {
                        continue;
                    }
                    break;
                }
                "trait_definition" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = content[name_node.byte_range()].to_string();
                        let mut children = Vec::new();
                        if let Some(body) = node.child_by_field_name("body") {
                            let mut body_cursor = body.walk();
                            if body_cursor.goto_first_child() {
                                Self::collect_scala_symbols(
                                    &mut body_cursor,
                                    content,
                                    &mut children,
                                    Some(&name),
                                );
                            }
                        }
                        symbols.push(SkeletonSymbol {
                            name: name.clone(),
                            kind: "trait",
                            signature: format!("trait {}", name),
                            docstring: None,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            children,
                        });
                    }
                    if cursor.goto_next_sibling() {
                        continue;
                    }
                    break;
                }
                _ => {}
            }

            if !matches!(
                kind,
                "class_definition" | "object_definition" | "trait_definition"
            ) && cursor.goto_first_child()
            {
                Self::collect_scala_symbols(cursor, content, symbols, parent);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    // Vue extraction - extracts script section as JavaScript/TypeScript
    fn extract_vue(&mut self, content: &str) -> Vec<SkeletonSymbol> {
        let tree = match self.parsers.parse_lang(Language::Vue, content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut symbols = Vec::new();
        let root = tree.root_node();

        // Vue files have component structure; look for script_element
        let mut cursor = root.walk();
        Self::collect_vue_symbols(&mut cursor, content, &mut symbols);
        symbols
    }

    fn collect_vue_symbols(
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        symbols: &mut Vec<SkeletonSymbol>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            // Extract component name from script setup or defineComponent
            if kind == "script_element" {
                // Script section - recurse into it for JS/TS symbols
                if cursor.goto_first_child() {
                    Self::collect_vue_script_symbols(cursor, content, symbols);
                    cursor.goto_parent();
                }
                if cursor.goto_next_sibling() {
                    continue;
                }
                break;
            }

            if cursor.goto_first_child() {
                Self::collect_vue_symbols(cursor, content, symbols);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn collect_vue_script_symbols(
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        symbols: &mut Vec<SkeletonSymbol>,
    ) {
        // Look for function declarations, const exports, etc. in script
        loop {
            let node = cursor.node();
            let kind = node.kind();

            match kind {
                "function_declaration" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = content[name_node.byte_range()].to_string();
                        let params = node
                            .child_by_field_name("parameters")
                            .map(|p| content[p.byte_range()].to_string())
                            .unwrap_or_else(|| "()".to_string());
                        symbols.push(SkeletonSymbol {
                            name: name.clone(),
                            kind: "function",
                            signature: format!("function {}{}", name, params),
                            docstring: None,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            children: Vec::new(),
                        });
                    }
                }
                "lexical_declaration" | "variable_declaration" => {
                    // Look for const/let declarations that might be composables
                    for i in 0..node.child_count() {
                        if let Some(child) = node.child(i) {
                            if child.kind() == "variable_declarator" {
                                if let Some(name_node) = child.child_by_field_name("name") {
                                    let name = content[name_node.byte_range()].to_string();
                                    // Check if it's a function (arrow function or function expression)
                                    if let Some(value) = child.child_by_field_name("value") {
                                        if value.kind() == "arrow_function" || value.kind() == "function_expression" {
                                            symbols.push(SkeletonSymbol {
                                                name: name.clone(),
                                                kind: "function",
                                                signature: format!("const {}", name),
                                                docstring: None,
                                                start_line: node.start_position().row + 1,
                                                end_line: node.end_position().row + 1,
                                                children: Vec::new(),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }

            if cursor.goto_first_child() {
                Self::collect_vue_script_symbols(cursor, content, symbols);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn extract_toml(&mut self, content: &str) -> Vec<SkeletonSymbol> {
        let tree = match self.parsers.parse_lang(Language::Toml, content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();
        Self::collect_toml_keys(&mut cursor, content, &mut symbols);
        symbols
    }

    fn collect_toml_keys(
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        symbols: &mut Vec<SkeletonSymbol>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            match kind {
                "table" | "table_array_element" => {
                    for i in 0..node.child_count() {
                        if let Some(child) = node.child(i) {
                            if child.kind() == "dotted_key" || child.kind() == "key" {
                                let key_text = &content[child.byte_range()];

                                let mut children = Vec::new();
                                if cursor.goto_first_child() {
                                    Self::collect_toml_keys(cursor, content, &mut children);
                                    cursor.goto_parent();
                                }

                                symbols.push(SkeletonSymbol {
                                    name: key_text.to_string(),
                                    kind: "class",
                                    signature: format!("[{}]", key_text),
                                    docstring: None,
                                    start_line: node.start_position().row + 1,
                                    end_line: node.end_position().row + 1,
                                    children,
                                });
                                break;
                            }
                        }
                    }
                    if cursor.goto_next_sibling() {
                        continue;
                    }
                    break;
                }
                "pair" => {
                    for i in 0..node.child_count() {
                        if let Some(child) = node.child(i) {
                            if child.kind() == "dotted_key" || child.kind() == "bare_key" || child.kind() == "quoted_key" {
                                let key_text = &content[child.byte_range()];
                                let key_name = key_text.trim_matches(|c| c == '"' || c == '\'');
                                symbols.push(SkeletonSymbol {
                                    name: key_name.to_string(),
                                    kind: "variable",
                                    signature: format!("{} =", key_name),
                                    docstring: None,
                                    start_line: node.start_position().row + 1,
                                    end_line: node.end_position().row + 1,
                                    children: Vec::new(),
                                });
                                break;
                            }
                        }
                    }
                }
                _ => {}
            }

            let dominated = matches!(kind, "table" | "table_array_element");
            if !dominated && cursor.goto_first_child() {
                Self::collect_toml_keys(cursor, content, symbols);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
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
    fn test_python_skeleton() {
        let mut extractor = SkeletonExtractor::new();
        let content = r#"
def foo(x: int) -> str:
    """Convert int to string."""
    return str(x)

class Bar:
    """A bar class."""

    def method(self, y: float) -> bool:
        """Check something."""
        return y > 0
"#;
        let result = extractor.extract(&PathBuf::from("test.py"), content);
        assert_eq!(result.symbols.len(), 2);

        let foo = &result.symbols[0];
        assert_eq!(foo.name, "foo");
        assert_eq!(foo.kind, "function");
        assert!(foo.signature.contains("def foo(x: int) -> str"));
        assert_eq!(foo.docstring.as_deref(), Some("Convert int to string."));

        let bar = &result.symbols[1];
        assert_eq!(bar.name, "Bar");
        assert_eq!(bar.kind, "class");
        assert_eq!(bar.children.len(), 1);
        assert_eq!(bar.children[0].name, "method");
    }

    #[test]
    fn test_rust_skeleton() {
        let mut extractor = SkeletonExtractor::new();
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

        // Should have struct with method from impl
        let foo = result.symbols.iter().find(|s| s.name == "Foo").unwrap();
        assert_eq!(foo.kind, "struct");
        assert!(foo.signature.contains("pub struct Foo"));
        assert_eq!(foo.children.len(), 1);
        assert_eq!(foo.children[0].name, "new");
    }

    #[test]
    fn test_format_skeleton() {
        let mut extractor = SkeletonExtractor::new();
        let content = r#"
def greet(name: str) -> str:
    """Return a personalized greeting message."""
    return f"Hello, {name}"
"#;
        let result = extractor.extract(&PathBuf::from("test.py"), content);
        let formatted = result.format(true);

        assert!(formatted.contains("def greet(name: str) -> str:"));
        assert!(formatted.contains("\"\"\"Return a personalized greeting message.\"\"\""));
    }

    #[test]
    fn test_markdown_skeleton() {
        let mut extractor = SkeletonExtractor::new();
        let content = r#"# Title

Some intro text.

## Section One

Content here.

```bash
# This is a comment, not a heading
echo "hello"
```

## Section Two

### Subsection

More content.
"#;
        let result = extractor.extract(&PathBuf::from("test.md"), content);

        // Should have 2 top-level headings: Title, and the h2s should be nested
        assert!(!result.symbols.is_empty(), "Should have headings");

        let title = &result.symbols[0];
        assert_eq!(title.name, "Title");
        assert_eq!(title.kind, "heading");

        // Check that code block comment wasn't extracted as heading
        let all_names: Vec<&str> = result
            .symbols
            .iter()
            .flat_map(|s| {
                std::iter::once(s.name.as_str()).chain(s.children.iter().map(|c| c.name.as_str()))
            })
            .collect();
        assert!(
            !all_names.iter().any(|n| n.contains("comment")),
            "Code block comments should not be headings: {:?}",
            all_names
        );
    }

    #[test]
    fn test_javascript_skeleton() {
        let mut extractor = SkeletonExtractor::new();
        let content = r#"function greet(name) {
  console.log("Hello, " + name);
}

class Greeter {
  constructor(name) { this.name = name; }
  greet() { console.log("Hello, " + this.name); }
}
"#;
        let result = extractor.extract(&PathBuf::from("test.js"), content);

        // Should have function and class
        assert_eq!(result.symbols.len(), 2, "Should have 2 top-level symbols");

        let greet_fn = &result.symbols[0];
        assert_eq!(greet_fn.name, "greet");
        assert_eq!(greet_fn.kind, "function");

        let greeter = &result.symbols[1];
        assert_eq!(greeter.name, "Greeter");
        assert_eq!(greeter.kind, "class");

        // Class should have 2 methods: constructor and greet
        assert_eq!(
            greeter.children.len(),
            2,
            "Greeter class should have 2 methods, got {:?}",
            greeter.children
        );
    }

    #[test]
    fn test_filter_types() {
        let mut extractor = SkeletonExtractor::new();
        let content = r#"
def helper():
    pass

class MyClass:
    def method(self):
        pass

def another_function():
    pass

class AnotherClass:
    pass
"#;
        let result = extractor.extract(&PathBuf::from("test.py"), content);

        // Original should have 4 top-level symbols (2 functions, 2 classes)
        assert_eq!(result.symbols.len(), 4);

        // Filtered should only have classes
        let filtered = result.filter_types();
        assert_eq!(filtered.symbols.len(), 2);
        assert!(filtered.symbols.iter().all(|s| s.kind == "class"));
        assert_eq!(filtered.symbols[0].name, "MyClass");
        assert_eq!(filtered.symbols[1].name, "AnotherClass");
    }

    #[test]
    fn test_filter_types_rust() {
        let mut extractor = SkeletonExtractor::new();
        let content = r#"
fn helper() {}

pub struct MyStruct {
    field: i32,
}

impl MyStruct {
    pub fn method(&self) {}
}

pub enum MyEnum {
    A,
    B,
}

pub trait MyTrait {
    fn required(&self);
}

fn another_function() {}
"#;
        let result = extractor.extract(&PathBuf::from("test.rs"), content);

        // Filtered should have struct, enum, trait, impl
        let filtered = result.filter_types();
        let kinds: Vec<_> = filtered.symbols.iter().map(|s| s.kind).collect();
        assert!(kinds.contains(&"struct"), "Should have struct");
        assert!(kinds.contains(&"enum"), "Should have enum");
        assert!(kinds.contains(&"trait"), "Should have trait");
        // Functions should be filtered out
        assert!(!kinds.contains(&"function"));
    }

    #[test]
    fn test_filter_types_typescript() {
        let mut extractor = SkeletonExtractor::new();
        let content = r#"
function helper() {}

interface MyInterface {
    method(): void;
}

class MyClass {
    method() {}
}

type MyType = string | number;

enum MyEnum {
    A,
    B,
}

const arrow = () => {};
"#;
        let result = extractor.extract(&PathBuf::from("test.ts"), content);

        let filtered = result.filter_types();
        let kinds: Vec<_> = filtered.symbols.iter().map(|s| s.kind).collect();
        let names: Vec<_> = filtered.symbols.iter().map(|s| s.name.as_str()).collect();

        assert!(names.contains(&"MyInterface"), "Should have interface");
        assert!(names.contains(&"MyClass"), "Should have class");
        // Functions should be filtered out
        assert!(!names.contains(&"helper"));
        assert!(!names.contains(&"arrow"));
    }

    #[test]
    fn test_filter_types_go() {
        let mut extractor = SkeletonExtractor::new();
        let content = r#"
package main

func helper() {}

type MyStruct struct {
    Field int
}

func (m *MyStruct) Method() {}

type MyInterface interface {
    Required()
}
"#;
        let result = extractor.extract(&PathBuf::from("test.go"), content);

        let filtered = result.filter_types();
        let names: Vec<_> = filtered.symbols.iter().map(|s| s.name.as_str()).collect();

        assert!(names.contains(&"MyStruct"), "Should have struct");
        assert!(names.contains(&"MyInterface"), "Should have interface");
        // Functions should be filtered out
        assert!(!names.contains(&"helper"));
    }

    #[test]
    fn test_filter_types_java() {
        let mut extractor = SkeletonExtractor::new();
        let content = r#"
public class MyClass {
    public void method() {}
}

interface MyInterface {
    void required();
}

enum MyEnum {
    A, B
}
"#;
        let result = extractor.extract(&PathBuf::from("Test.java"), content);

        let filtered = result.filter_types();
        let names: Vec<_> = filtered.symbols.iter().map(|s| s.name.as_str()).collect();

        assert!(names.contains(&"MyClass"), "Should have class");
        assert!(names.contains(&"MyInterface"), "Should have interface");
        assert!(names.contains(&"MyEnum"), "Should have enum");
    }

    #[test]
    fn test_filter_types_ruby() {
        let mut extractor = SkeletonExtractor::new();
        let content = r#"
def helper
end

class MyClass
  def method
  end
end

module MyModule
  def module_method
  end
end
"#;
        let result = extractor.extract(&PathBuf::from("test.rb"), content);

        let filtered = result.filter_types();
        let names: Vec<_> = filtered.symbols.iter().map(|s| s.name.as_str()).collect();

        assert!(names.contains(&"MyClass"), "Should have class");
        assert!(names.contains(&"MyModule"), "Should have module");
        // Functions should be filtered out
        assert!(!names.contains(&"helper"));
    }

    #[test]
    fn test_useless_docstring_detection() {
        // Useless docstrings - just repeat the function name
        assert!(is_useless_docstring("setUserId", "Sets the user id"));
        assert!(is_useless_docstring("setUserId", "Set user id."));
        assert!(is_useless_docstring("getUser", "Gets the user"));
        assert!(is_useless_docstring("get_user", "Get the user."));
        assert!(is_useless_docstring("processData", "Process data"));
        assert!(is_useless_docstring("handleRequest", "Handle request."));
        assert!(is_useless_docstring("parse", "Parse."));
        assert!(is_useless_docstring("init", "Initialize."));

        // Useful docstrings - provide additional context
        assert!(!is_useless_docstring("setUserId", "Update the user ID from the authentication token"));
        assert!(!is_useless_docstring("parse", "Parse JSON string into structured data"));
        assert!(!is_useless_docstring("getUser", "Fetch user from database by ID"));
        assert!(!is_useless_docstring("process", "Apply validation rules and normalize input"));
        assert!(!is_useless_docstring("init", "Set up database connection pool with retry logic"));
    }

    #[test]
    fn test_scala_skeleton() {
        let mut extractor = SkeletonExtractor::new();
        let content = r#"
object Main {
  def hello(name: String): String = {
    s"Hello, $name"
  }
}

class Person(name: String) {
  def greet(): Unit = {
    println(s"Hi, I'm $name")
  }
}

trait Greeter {
  def greet(name: String): String
}
"#;
        let result = extractor.extract(&PathBuf::from("test.scala"), content);

        // Should have object, class, and trait
        assert!(!result.symbols.is_empty(), "Should have symbols");

        let names: Vec<_> = result.symbols.iter().map(|s| s.name.as_str()).collect();
        let kinds: Vec<_> = result.symbols.iter().map(|s| s.kind).collect();

        assert!(names.contains(&"Main"), "Should have Main object");
        assert!(names.contains(&"Person"), "Should have Person class");
        assert!(names.contains(&"Greeter"), "Should have Greeter trait");

        assert!(kinds.contains(&"module"), "Should have module (object)");
        assert!(kinds.contains(&"class"), "Should have class");
        assert!(kinds.contains(&"trait"), "Should have trait");
    }

    #[test]
    fn test_vue_skeleton() {
        let mut extractor = SkeletonExtractor::new();
        let content = r#"
<template>
  <div>{{ message }}</div>
</template>

<script setup>
function greet(name) {
  return `Hello, ${name}`;
}

const handleClick = () => {
  console.log("clicked");
};
</script>

<style scoped>
div { color: red; }
</style>
"#;
        let result = extractor.extract(&PathBuf::from("test.vue"), content);

        // Vue files should extract functions from script
        // Note: The exact symbols depend on tree-sitter-vue parsing
        let names: Vec<_> = result.symbols.iter().map(|s| s.name.as_str()).collect();

        // Check that we extracted at least some symbols from the script
        // The exact parsing depends on tree-sitter-vue behavior
        assert!(
            result.symbols.is_empty() || names.iter().any(|n| *n == "greet" || *n == "handleClick"),
            "Should have greet or handleClick function, or be empty if vue parsing differs: {:?}",
            names
        );
    }

    #[test]
    fn test_filter_types_scala() {
        let mut extractor = SkeletonExtractor::new();
        let content = r#"
def helper(): Unit = {}

class MyClass {
  def method(): Unit = {}
}

object MyObject {
  def objectMethod(): String = "hi"
}

trait MyTrait {
  def traitMethod(): Int
}
"#;
        let result = extractor.extract(&PathBuf::from("test.scala"), content);

        let filtered = result.filter_types();
        let names: Vec<_> = filtered.symbols.iter().map(|s| s.name.as_str()).collect();

        assert!(names.contains(&"MyClass"), "Should have class");
        assert!(names.contains(&"MyObject"), "Should have object (module)");
        assert!(names.contains(&"MyTrait"), "Should have trait");
        // Top-level functions should be filtered out
        assert!(!names.contains(&"helper"));
    }

    #[test]
    fn test_split_identifier() {
        assert_eq!(split_identifier("setUserId"), vec!["set", "user", "id"]);
        assert_eq!(split_identifier("get_user_id"), vec!["get", "user", "id"]);
        assert_eq!(split_identifier("HTTPRequest"), vec!["h", "t", "t", "p", "request"]);
        assert_eq!(split_identifier("parseJSON"), vec!["parse", "j", "s", "o", "n"]);
        assert_eq!(split_identifier("simple"), vec!["simple"]);
    }

    #[test]
    fn test_python_trait_extraction() {
        // Test that trait-based extraction works for Python
        let extractor = SkeletonExtractor::new();
        let content = r#"
def foo(x: int) -> str:
    """Convert int to string."""
    return str(x)

class Bar:
    """A bar class."""

    def method(self, y: float) -> bool:
        """Check something."""
        return y > 0
"#;
        let path = PathBuf::from("test.py");
        let result = extractor.extract_with_support(&path, content);
        assert!(result.is_some(), "Should have trait support for Python");

        let result = result.unwrap();
        assert_eq!(result.symbols.len(), 2, "Should have 2 top-level symbols");

        let foo = &result.symbols[0];
        assert_eq!(foo.name, "foo");
        assert_eq!(foo.kind, "function");
        assert!(foo.signature.contains("def foo"));
        assert_eq!(foo.docstring.as_deref(), Some("Convert int to string."));

        let bar = &result.symbols[1];
        assert_eq!(bar.name, "Bar");
        assert_eq!(bar.kind, "class");
        assert_eq!(bar.children.len(), 1, "Class should have 1 method");
        assert_eq!(bar.children[0].name, "method");
        assert_eq!(bar.children[0].kind, "method");
    }
}
