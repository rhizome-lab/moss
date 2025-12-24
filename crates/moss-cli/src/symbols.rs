use moss_core::{tree_sitter, Language, Parsers};
use moss_languages::{get_support, LanguageSupport, SymbolKind as LangSymbolKind};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub start_line: usize,
    pub end_line: usize,
    pub parent: Option<String>,
}

/// An import statement (from X import Y as Z)
#[derive(Debug, Clone)]
pub struct Import {
    /// The module being imported from (None for "import X")
    pub module: Option<String>,
    /// The name being imported
    pub name: String,
    /// Alias if present (from X import Y as Z -> alias = Z)
    pub alias: Option<String>,
    /// Line number
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Import variant reserved for import tracking
pub enum SymbolKind {
    Function,
    Class,
    Method,
    Variable,
    Import,
}

impl SymbolKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SymbolKind::Function => "function",
            SymbolKind::Class => "class",
            SymbolKind::Method => "method",
            SymbolKind::Variable => "variable",
            SymbolKind::Import => "import",
        }
    }
}

fn convert_symbol_kind(kind: LangSymbolKind) -> SymbolKind {
    match kind {
        LangSymbolKind::Function => SymbolKind::Function,
        LangSymbolKind::Class | LangSymbolKind::Struct | LangSymbolKind::Enum
        | LangSymbolKind::Interface | LangSymbolKind::Trait | LangSymbolKind::Type => SymbolKind::Class,
        LangSymbolKind::Method => SymbolKind::Method,
        LangSymbolKind::Variable | LangSymbolKind::Constant | LangSymbolKind::Module
        | LangSymbolKind::Heading => SymbolKind::Variable,
    }
}

pub struct SymbolParser {
    parsers: Parsers,
}

impl SymbolParser {
    pub fn new() -> Self {
        Self {
            parsers: Parsers::new(),
        }
    }

    pub fn parse_file(&self, path: &Path, content: &str) -> Vec<Symbol> {
        let lang = match Language::from_path(path) {
            Some(l) => l,
            None => return Vec::new(),
        };

        // Use trait-based extraction for all supported languages
        if let Some(support) = get_support(lang) {
            return self.parse_with_trait(lang, content, support);
        }

        Vec::new()
    }

    fn parse_with_trait(&self, lang: Language, content: &str, support: &dyn LanguageSupport) -> Vec<Symbol> {
        let tree = match self.parsers.parse_lang(lang, content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let root = tree.root_node();
        let mut symbols = Vec::new();
        self.collect_with_trait(&mut root.walk(), content, support, &mut symbols, None);
        symbols
    }

    fn collect_with_trait(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        support: &dyn LanguageSupport,
        symbols: &mut Vec<Symbol>,
        parent: Option<&str>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            // Check for container (class, struct, etc.)
            if support.container_kinds().contains(&kind) {
                if let Some(sym) = support.extract_container(&node, content) {
                    let sym_name = sym.name.clone();
                    symbols.push(Symbol {
                        name: sym.name,
                        kind: convert_symbol_kind(sym.kind),
                        start_line: sym.start_line,
                        end_line: sym.end_line,
                        parent: parent.map(String::from),
                    });

                    // Recurse into container to find methods
                    if cursor.goto_first_child() {
                        self.collect_with_trait(cursor, content, support, symbols, Some(&sym_name));
                        cursor.goto_parent();
                    }
                    if cursor.goto_next_sibling() {
                        continue;
                    }
                    break;
                }
            }

            // Check for function
            if support.function_kinds().contains(&kind) {
                if let Some(sym) = support.extract_function(&node, content, parent.is_some()) {
                    symbols.push(Symbol {
                        name: sym.name,
                        kind: convert_symbol_kind(sym.kind),
                        start_line: sym.start_line,
                        end_line: sym.end_line,
                        parent: parent.map(String::from),
                    });
                }
            }

            // Check for type (struct, enum, interface - when not a container)
            if support.type_kinds().contains(&kind) && !support.container_kinds().contains(&kind) {
                if let Some(sym) = support.extract_type(&node, content) {
                    symbols.push(Symbol {
                        name: sym.name,
                        kind: convert_symbol_kind(sym.kind),
                        start_line: sym.start_line,
                        end_line: sym.end_line,
                        parent: parent.map(String::from),
                    });
                }
            }

            // Recurse into children (but not for containers, handled above)
            if !support.container_kinds().contains(&kind) && cursor.goto_first_child() {
                self.collect_with_trait(cursor, content, support, symbols, parent);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    /// Parse Python imports from a file
    pub fn parse_python_imports(&self, content: &str) -> Vec<Import> {
        let tree = match self.parsers.parse_lang(Language::Python, content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut imports = Vec::new();
        let root = tree.root_node();

        let mut cursor = root.walk();
        self.collect_python_imports(&mut cursor, content, &mut imports);

        imports
    }

    fn collect_python_imports(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        imports: &mut Vec<Import>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            match kind {
                // import os, import json as j
                "import_statement" => {
                    // Iterate through children looking for dotted_name or aliased_import
                    for i in 0..node.child_count() {
                        if let Some(child) = node.child(i) {
                            match child.kind() {
                                "dotted_name" => {
                                    let name = &content[child.byte_range()];
                                    imports.push(Import {
                                        module: None,
                                        name: name.to_string(),
                                        alias: None,
                                        line: child.start_position().row + 1,
                                    });
                                }
                                "aliased_import" => {
                                    let name = child
                                        .child_by_field_name("name")
                                        .map(|n| content[n.byte_range()].to_string());
                                    let alias = child
                                        .child_by_field_name("alias")
                                        .map(|n| content[n.byte_range()].to_string());
                                    if let Some(name) = name {
                                        imports.push(Import {
                                            module: None,
                                            name,
                                            alias,
                                            line: child.start_position().row + 1,
                                        });
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                // from pathlib import Path, from moss.gen import serialize as ser
                "import_from_statement" => {
                    // Get the module name
                    let module = node
                        .child_by_field_name("module_name")
                        .map(|n| content[n.byte_range()].to_string());

                    // Find import items
                    for i in 0..node.child_count() {
                        if let Some(child) = node.child(i) {
                            match child.kind() {
                                "dotted_name" | "identifier" => {
                                    // Skip the module name itself (already captured)
                                    if child.start_byte()
                                        > node
                                            .children(&mut node.walk())
                                            .find(|c| c.kind() == "import")
                                            .map(|c| c.end_byte())
                                            .unwrap_or(0)
                                    {
                                        let name = &content[child.byte_range()];
                                        imports.push(Import {
                                            module: module.clone(),
                                            name: name.to_string(),
                                            alias: None,
                                            line: child.start_position().row + 1,
                                        });
                                    }
                                }
                                "aliased_import" => {
                                    let name = child
                                        .child_by_field_name("name")
                                        .map(|n| content[n.byte_range()].to_string());
                                    let alias = child
                                        .child_by_field_name("alias")
                                        .map(|n| content[n.byte_range()].to_string());
                                    if let Some(name) = name {
                                        imports.push(Import {
                                            module: module.clone(),
                                            name,
                                            alias,
                                            line: child.start_position().row + 1,
                                        });
                                    }
                                }
                                "wildcard_import" => {
                                    imports.push(Import {
                                        module: module.clone(),
                                        name: "*".to_string(),
                                        alias: None,
                                        line: child.start_position().row + 1,
                                    });
                                }
                                _ => {}
                            }
                        }
                    }
                }
                _ => {}
            }

            // Recurse into children
            if cursor.goto_first_child() {
                self.collect_python_imports(cursor, content, imports);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    /// Find a symbol by name in a file
    pub fn find_symbol(&mut self, path: &Path, content: &str, name: &str) -> Option<Symbol> {
        let symbols = self.parse_file(path, content);
        symbols.into_iter().find(|s| s.name == name)
    }

    /// Extract the source code for a symbol
    pub fn extract_symbol_source(
        &mut self,
        path: &Path,
        content: &str,
        name: &str,
    ) -> Option<String> {
        let symbol = self.find_symbol(path, content, name)?;
        let lines: Vec<&str> = content.lines().collect();
        let start = symbol.start_line.saturating_sub(1);
        let end = symbol.end_line.min(lines.len());
        Some(lines[start..end].join("\n"))
    }

    /// Find callees (functions/methods called) within a symbol
    #[allow(dead_code)] // Call graph API - used by index
    pub fn find_callees(&mut self, path: &Path, content: &str, symbol_name: &str) -> Vec<String> {
        let symbol = match self.find_symbol(path, content, symbol_name) {
            Some(s) => s,
            None => return Vec::new(),
        };

        // Extract symbol source
        let lines: Vec<&str> = content.lines().collect();
        let start = symbol.start_line.saturating_sub(1);
        let end = symbol.end_line.min(lines.len());
        let source = lines[start..end].join("\n");

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match ext {
            "py" => self.find_python_calls(&source),
            "rs" => self.find_rust_calls(&source),
            _ => Vec::new(),
        }
    }

    fn find_python_calls(&self, source: &str) -> Vec<String> {
        let tree = match self.parsers.parse_lang(Language::Python, source) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut calls = std::collections::HashSet::new();
        let mut cursor = tree.root_node().walk();
        self.collect_python_calls(&mut cursor, source, &mut calls);

        let mut result: Vec<_> = calls.into_iter().collect();
        result.sort();
        result
    }

    fn collect_python_calls(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        calls: &mut std::collections::HashSet<String>,
    ) {
        loop {
            let node = cursor.node();

            if node.kind() == "call" {
                // Get the function being called
                if let Some(func_node) = node.child_by_field_name("function") {
                    let func_text = &content[func_node.byte_range()];
                    // Extract just the function name (last part if dotted)
                    let name = func_text.split('.').last().unwrap_or(func_text);
                    calls.insert(name.to_string());
                }
            }

            if cursor.goto_first_child() {
                self.collect_python_calls(cursor, content, calls);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    /// Find callees with line numbers (for call graph indexing)
    /// Returns: (callee_name, line, Option<qualifier>)
    /// For foo.bar(), returns ("bar", line, Some("foo"))
    /// For bar(), returns ("bar", line, None)
    #[allow(dead_code)] // Call graph API - used by index
    pub fn find_callees_with_lines(
        &mut self,
        path: &Path,
        content: &str,
        symbol_name: &str,
    ) -> Vec<(String, usize, Option<String>)> {
        let symbol = match self.find_symbol(path, content, symbol_name) {
            Some(s) => s,
            None => return Vec::new(),
        };
        self.find_callees_for_symbol(path, content, &symbol)
    }

    /// Find callees for a pre-parsed symbol (avoids re-parsing the file)
    /// Use this when you already have the Symbol from parse_file()
    pub fn find_callees_for_symbol(
        &mut self,
        path: &Path,
        content: &str,
        symbol: &Symbol,
    ) -> Vec<(String, usize, Option<String>)> {
        let lines: Vec<&str> = content.lines().collect();
        let start = symbol.start_line.saturating_sub(1);
        let end = symbol.end_line.min(lines.len());
        let source = lines[start..end].join("\n");

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match ext {
            "py" => self.find_python_calls_with_lines(&source, symbol.start_line),
            "rs" => self.find_rust_calls_with_lines(&source, symbol.start_line),
            "ts" | "tsx" => self.find_typescript_calls_with_lines(&source, symbol.start_line, ext == "tsx"),
            "js" | "mjs" | "cjs" => self.find_javascript_calls_with_lines(&source, symbol.start_line),
            "java" => self.find_java_calls_with_lines(&source, symbol.start_line),
            "go" => self.find_go_calls_with_lines(&source, symbol.start_line),
            _ => Vec::new(),
        }
    }

    fn find_python_calls_with_lines(
        &self,
        source: &str,
        base_line: usize,
    ) -> Vec<(String, usize, Option<String>)> {
        let tree = match self.parsers.parse_lang(Language::Python, source) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut calls = Vec::new();
        let mut cursor = tree.root_node().walk();
        self.collect_python_calls_with_lines(&mut cursor, source, base_line, &mut calls);
        calls
    }

    fn collect_python_calls_with_lines(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        base_line: usize,
        calls: &mut Vec<(String, usize, Option<String>)>,
    ) {
        loop {
            let node = cursor.node();

            if node.kind() == "call" {
                if let Some(func_node) = node.child_by_field_name("function") {
                    let func_text = &content[func_node.byte_range()];
                    let line = node.start_position().row + base_line;

                    // Parse qualifier.name or just name
                    if let Some(dot_pos) = func_text.rfind('.') {
                        let qualifier = &func_text[..dot_pos];
                        let name = &func_text[dot_pos + 1..];
                        calls.push((name.to_string(), line, Some(qualifier.to_string())));
                    } else {
                        calls.push((func_text.to_string(), line, None));
                    }
                }
            }

            if cursor.goto_first_child() {
                self.collect_python_calls_with_lines(cursor, content, base_line, calls);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn find_rust_calls_with_lines(
        &self,
        source: &str,
        base_line: usize,
    ) -> Vec<(String, usize, Option<String>)> {
        let tree = match self.parsers.parse_lang(Language::Rust, source) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut calls = Vec::new();
        let mut cursor = tree.root_node().walk();
        self.collect_rust_calls_with_lines(&mut cursor, source, base_line, &mut calls);
        calls
    }

    fn collect_rust_calls_with_lines(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        base_line: usize,
        calls: &mut Vec<(String, usize, Option<String>)>,
    ) {
        loop {
            let node = cursor.node();

            if node.kind() == "call_expression" {
                if let Some(func_node) = node.child_by_field_name("function") {
                    let func_text = &content[func_node.byte_range()];
                    let line = node.start_position().row + base_line;

                    // Parse qualifier::name, qualifier.name, or just name
                    // For Rust: foo::bar() or foo.bar() or bar()
                    if let Some(sep_pos) = func_text.rfind("::").or_else(|| func_text.rfind('.')) {
                        let sep_len = if func_text[sep_pos..].starts_with("::") {
                            2
                        } else {
                            1
                        };
                        let qualifier = &func_text[..sep_pos];
                        let name = &func_text[sep_pos + sep_len..];
                        calls.push((name.to_string(), line, Some(qualifier.to_string())));
                    } else {
                        calls.push((func_text.to_string(), line, None));
                    }
                }
            }

            if cursor.goto_first_child() {
                self.collect_rust_calls_with_lines(cursor, content, base_line, calls);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn find_typescript_calls_with_lines(
        &self,
        source: &str,
        base_line: usize,
        is_tsx: bool,
    ) -> Vec<(String, usize, Option<String>)> {
        let lang = if is_tsx {
            Language::Tsx
        } else {
            Language::TypeScript
        };
        let tree = match self.parsers.parse_lang(lang, source) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut calls = Vec::new();
        let mut cursor = tree.root_node().walk();
        self.collect_js_ts_calls_with_lines(&mut cursor, source, base_line, &mut calls);
        calls
    }

    fn find_javascript_calls_with_lines(
        &self,
        source: &str,
        base_line: usize,
    ) -> Vec<(String, usize, Option<String>)> {
        let tree = match self.parsers.parse_lang(Language::JavaScript, source) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut calls = Vec::new();
        let mut cursor = tree.root_node().walk();
        self.collect_js_ts_calls_with_lines(&mut cursor, source, base_line, &mut calls);
        calls
    }

    /// Shared implementation for JavaScript and TypeScript call extraction
    fn collect_js_ts_calls_with_lines(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        base_line: usize,
        calls: &mut Vec<(String, usize, Option<String>)>,
    ) {
        loop {
            let node = cursor.node();

            if node.kind() == "call_expression" {
                if let Some(func_node) = node.child_by_field_name("function") {
                    let func_text = &content[func_node.byte_range()];
                    let line = node.start_position().row + base_line;

                    // Parse qualifier.name or just name (e.g., obj.method(), func())
                    if let Some(dot_pos) = func_text.rfind('.') {
                        let qualifier = &func_text[..dot_pos];
                        let name = &func_text[dot_pos + 1..];
                        calls.push((name.to_string(), line, Some(qualifier.to_string())));
                    } else {
                        calls.push((func_text.to_string(), line, None));
                    }
                }
            }

            if cursor.goto_first_child() {
                self.collect_js_ts_calls_with_lines(cursor, content, base_line, calls);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn find_java_calls_with_lines(
        &self,
        source: &str,
        base_line: usize,
    ) -> Vec<(String, usize, Option<String>)> {
        let tree = match self.parsers.parse_lang(Language::Java, source) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut calls = Vec::new();
        let mut cursor = tree.root_node().walk();
        self.collect_java_calls_with_lines(&mut cursor, source, base_line, &mut calls);
        calls
    }

    fn collect_java_calls_with_lines(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        base_line: usize,
        calls: &mut Vec<(String, usize, Option<String>)>,
    ) {
        loop {
            let node = cursor.node();

            // Java uses "method_invocation" for method calls
            if node.kind() == "method_invocation" {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = &content[name_node.byte_range()];
                    let line = node.start_position().row + base_line;

                    // Get the object/qualifier if present
                    let qualifier = node
                        .child_by_field_name("object")
                        .map(|obj| content[obj.byte_range()].to_string());

                    calls.push((name.to_string(), line, qualifier));
                }
            }

            if cursor.goto_first_child() {
                self.collect_java_calls_with_lines(cursor, content, base_line, calls);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn find_go_calls_with_lines(
        &self,
        source: &str,
        base_line: usize,
    ) -> Vec<(String, usize, Option<String>)> {
        let tree = match self.parsers.parse_lang(Language::Go, source) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut calls = Vec::new();
        let mut cursor = tree.root_node().walk();
        self.collect_go_calls_with_lines(&mut cursor, source, base_line, &mut calls);
        calls
    }

    fn collect_go_calls_with_lines(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        base_line: usize,
        calls: &mut Vec<(String, usize, Option<String>)>,
    ) {
        loop {
            let node = cursor.node();

            if node.kind() == "call_expression" {
                if let Some(func_node) = node.child_by_field_name("function") {
                    let func_text = &content[func_node.byte_range()];
                    let line = node.start_position().row + base_line;

                    // Go uses . for method calls and package access: pkg.Func(), obj.Method()
                    if let Some(dot_pos) = func_text.rfind('.') {
                        let qualifier = &func_text[..dot_pos];
                        let name = &func_text[dot_pos + 1..];
                        calls.push((name.to_string(), line, Some(qualifier.to_string())));
                    } else {
                        calls.push((func_text.to_string(), line, None));
                    }
                }
            }

            if cursor.goto_first_child() {
                self.collect_go_calls_with_lines(cursor, content, base_line, calls);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn find_rust_calls(&self, source: &str) -> Vec<String> {
        let tree = match self.parsers.parse_lang(Language::Rust, source) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut calls = std::collections::HashSet::new();
        let mut cursor = tree.root_node().walk();
        self.collect_rust_calls(&mut cursor, source, &mut calls);

        let mut result: Vec<_> = calls.into_iter().collect();
        result.sort();
        result
    }

    fn collect_rust_calls(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        calls: &mut std::collections::HashSet<String>,
    ) {
        loop {
            let node = cursor.node();

            if node.kind() == "call_expression" {
                // Get the function being called
                if let Some(func_node) = node.child_by_field_name("function") {
                    let func_text = &content[func_node.byte_range()];
                    // Extract just the function name
                    let name = func_text
                        .split("::")
                        .last()
                        .unwrap_or(func_text)
                        .split('.')
                        .last()
                        .unwrap_or(func_text);
                    calls.insert(name.to_string());
                }
            }

            if cursor.goto_first_child() {
                self.collect_rust_calls(cursor, content, calls);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    /// Find callers (symbols that call a given function) across all files
    #[allow(dead_code)] // Call graph API - used by index
    pub fn find_callers(
        &mut self,
        root: &Path,
        files: &[(String, bool)],
        symbol_name: &str,
    ) -> Vec<(String, String)> {
        let mut callers = Vec::new();

        for (path, is_dir) in files {
            if *is_dir {
                continue;
            }
            if !path.ends_with(".py") && !path.ends_with(".rs") {
                continue;
            }

            let full_path = root.join(path);
            let content = match std::fs::read_to_string(&full_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let symbols = self.parse_file(&full_path, &content);
            for symbol in symbols {
                let callees = self.find_callees(&full_path, &content, &symbol.name);
                if callees.contains(&symbol_name.to_string()) {
                    callers.push((path.clone(), symbol.name.clone()));
                }
            }
        }

        callers
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_parse_python_function() {
        let mut parser = SymbolParser::new();
        let content = r#"
def foo():
    pass

def bar(x):
    return x
"#;
        let symbols = parser.parse_file(&PathBuf::from("test.py"), content);
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "foo");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
        assert_eq!(symbols[1].name, "bar");
    }

    #[test]
    fn test_parse_python_class() {
        let mut parser = SymbolParser::new();
        let content = r#"
class Foo:
    def method(self):
        pass
"#;
        let symbols = parser.parse_file(&PathBuf::from("test.py"), content);
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "Foo");
        assert_eq!(symbols[0].kind, SymbolKind::Class);
        assert_eq!(symbols[1].name, "method");
        assert_eq!(symbols[1].kind, SymbolKind::Method);
        assert_eq!(symbols[1].parent, Some("Foo".to_string()));
    }

    #[test]
    fn test_parse_rust_function() {
        let mut parser = SymbolParser::new();
        let content = r#"
fn foo() {}

fn bar(x: i32) -> i32 {
    x
}
"#;
        let symbols = parser.parse_file(&PathBuf::from("test.rs"), content);
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "foo");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_extract_symbol_source() {
        let mut parser = SymbolParser::new();
        let content = r#"def foo():
    return 42

def bar():
    pass"#;
        let source = parser.extract_symbol_source(&PathBuf::from("test.py"), content, "foo");
        assert!(source.is_some());
        assert!(source.unwrap().contains("return 42"));
    }
}
