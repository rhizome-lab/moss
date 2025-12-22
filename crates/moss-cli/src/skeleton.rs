//! AST-based code skeleton extraction.
//!
//! Extracts function/class signatures with optional docstrings.

use moss_core::{Language, Parsers};
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
                if !first_line.is_empty() {
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

pub struct SkeletonExtractor {
    parsers: Parsers,
}

impl SkeletonExtractor {
    pub fn new() -> Self {
        Self {
            parsers: Parsers::new(),
        }
    }

    pub fn extract(&mut self, path: &Path, content: &str) -> SkeletonResult {
        let lang = Language::from_path(path);
        let symbols = match lang {
            Some(Language::Python) => self.extract_python(content),
            Some(Language::Rust) => self.extract_rust(content),
            Some(Language::Markdown) => self.extract_markdown(content),
            Some(Language::JavaScript) | Some(Language::Tsx) => self.extract_javascript(content),
            Some(Language::TypeScript) => self.extract_typescript(content),
            Some(Language::Go) => self.extract_go(content),
            Some(Language::Java) => self.extract_java(content),
            Some(Language::C) => self.extract_c(content),
            Some(Language::Cpp) => self.extract_cpp(content),
            Some(Language::Ruby) => self.extract_ruby(content),
            _ => Vec::new(),
        };

        SkeletonResult {
            symbols,
            file_path: path.to_string_lossy().to_string(),
        }
    }

    fn extract_python(&mut self, content: &str) -> Vec<SkeletonSymbol> {
        let tree = match self.parsers.get(Language::Python).parse(content, None) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        Self::collect_python_symbols(&mut cursor, content, &mut symbols, false);
        symbols
    }

    fn collect_python_symbols(
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        symbols: &mut Vec<SkeletonSymbol>,
        in_class: bool,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            match kind {
                "function_definition" | "async_function_definition" => {
                    if let Some(sym) = Self::extract_python_function(&node, content, in_class) {
                        symbols.push(sym);
                    }
                }
                "class_definition" => {
                    if let Some(sym) = Self::extract_python_class(&node, content) {
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
                Self::collect_python_symbols(cursor, content, symbols, in_class);
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
    ) -> Option<SkeletonSymbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = content[name_node.byte_range()].to_string();

        // Skip private methods unless they're dunder methods
        if name.starts_with('_') && !name.starts_with("__") {
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
    ) -> Option<SkeletonSymbol> {
        let name_node = node.child_by_field_name("name")?;
        let name = content[name_node.byte_range()].to_string();

        // Skip private classes
        if name.starts_with('_') && !name.starts_with("__") {
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
                Self::collect_python_symbols(&mut cursor, content, &mut children, true);
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

        if first_child.kind() == "expression_statement" {
            let expr = first_child.child(0)?;
            if expr.kind() == "string" {
                let text = &content[expr.byte_range()];
                // Remove quotes and strip
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
            }
        }
        None
    }

    fn extract_rust(&mut self, content: &str) -> Vec<SkeletonSymbol> {
        let tree = match self.parsers.get(Language::Rust).parse(content, None) {
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
        let tree = match self.parsers.get(Language::Markdown).parse(content, None) {
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
        let tree = match self.parsers.get(Language::JavaScript).parse(content, None) {
            Some(t) => t,
            None => return Vec::new(),
        };
        Self::extract_js_like_symbols(&tree, content)
    }

    // TypeScript extraction
    fn extract_typescript(&mut self, content: &str) -> Vec<SkeletonSymbol> {
        let tree = match self.parsers.get(Language::TypeScript).parse(content, None) {
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
        let tree = match self.parsers.get(Language::Go).parse(content, None) {
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
        let tree = match self.parsers.get(Language::Java).parse(content, None) {
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
        let tree = match self.parsers.get(Language::C).parse(content, None) {
            Some(t) => t,
            None => return Vec::new(),
        };
        Self::extract_c_like_symbols(&tree, content)
    }

    // C++ extraction
    fn extract_cpp(&mut self, content: &str) -> Vec<SkeletonSymbol> {
        let tree = match self.parsers.get(Language::Cpp).parse(content, None) {
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
        let tree = match self.parsers.get(Language::Ruby).parse(content, None) {
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
def hello(name: str) -> str:
    """Say hello."""
    return f"Hello, {name}"
"#;
        let result = extractor.extract(&PathBuf::from("test.py"), content);
        let formatted = result.format(true);

        assert!(formatted.contains("def hello(name: str) -> str:"));
        assert!(formatted.contains("\"\"\"Say hello.\"\"\""));
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
}
