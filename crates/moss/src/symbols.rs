use crate::extract::{ExtractOptions, Extractor};
use crate::parsers;
use moss_languages::{
    Language, Symbol as LangSymbol, SymbolKind, support_for_grammar, support_for_path,
};
use std::path::Path;
use tree_sitter;

/// A flattened symbol for indexing (parent reference instead of nested children)
#[derive(Debug, Clone)]
pub struct FlatSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub start_line: usize,
    pub end_line: usize,
    pub parent: Option<String>,
}

/// A flattened import for indexing (one entry per imported name)
#[derive(Debug, Clone)]
pub struct FlatImport {
    /// The module being imported from (None for "import X")
    pub module: Option<String>,
    /// The name being imported
    pub name: String,
    /// Alias if present (from X import Y as Z -> alias = Z)
    pub alias: Option<String>,
    /// Line number
    pub line: usize,
}

pub struct SymbolParser {
    extractor: Extractor,
    // Keep for import parsing and call graph analysis
}

impl SymbolParser {
    pub fn new() -> Self {
        Self {
            extractor: Extractor::with_options(ExtractOptions {
                include_private: true, // symbols.rs includes all symbols for indexing
            }),
        }
    }

    pub fn parse_file(&self, path: &Path, content: &str) -> Vec<FlatSymbol> {
        if support_for_path(path).is_none() {
            return Vec::new();
        }

        // Use shared extractor for symbol extraction
        let result = self.extractor.extract(path, content);

        // Flatten nested symbols
        let mut symbols = Vec::new();
        for sym in &result.symbols {
            self.flatten_symbol(sym, None, &mut symbols);
        }
        symbols
    }

    /// Flatten a nested symbol into the flat list with parent references
    fn flatten_symbol(
        &self,
        sym: &LangSymbol,
        parent: Option<&str>,
        symbols: &mut Vec<FlatSymbol>,
    ) {
        symbols.push(FlatSymbol {
            name: sym.name.clone(),
            kind: sym.kind,
            start_line: sym.start_line,
            end_line: sym.end_line,
            parent: parent.map(String::from),
        });

        // Recurse into children with current symbol as parent
        for child in &sym.children {
            self.flatten_symbol(child, Some(&sym.name), symbols);
        }
    }

    /// Parse imports from any supported language file using trait-based extraction.
    /// Returns a flattened list where each imported name gets its own FlatImport entry.
    pub fn parse_imports(&self, path: &Path, content: &str) -> Vec<FlatImport> {
        let support = match support_for_path(path) {
            Some(s) => s,
            None => return Vec::new(),
        };

        // Check if this language has import support
        if support.import_kinds().is_empty() {
            return Vec::new();
        }

        let tree = match parsers::parse_with_grammar(support.grammar_name(), content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut imports = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();
        self.collect_imports_with_trait(&mut cursor, content, support, &mut imports);
        imports
    }

    fn collect_imports_with_trait(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        support: &dyn Language,
        imports: &mut Vec<FlatImport>,
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
                        let mut sub_imports = Vec::new();
                        let sub_root = sub_tree.root_node();
                        let mut sub_cursor = sub_root.walk();
                        self.collect_imports_with_trait(
                            &mut sub_cursor,
                            &embedded.content,
                            sub_lang,
                            &mut sub_imports,
                        );

                        // Adjust line numbers for embedded content offset
                        for mut imp in sub_imports {
                            imp.line += embedded.start_line - 1;
                            imports.push(imp);
                        }
                    }
                }
                // Don't descend into embedded nodes - we've already processed them
                if cursor.goto_next_sibling() {
                    continue;
                }
                break;
            }

            // Check for import nodes
            if support.import_kinds().contains(&kind) {
                let lang_imports = support.extract_imports(&node, content);
                // Flatten: each name in the import becomes a separate FlatImport entry
                for lang_imp in lang_imports {
                    if lang_imp.is_wildcard {
                        imports.push(FlatImport {
                            module: Some(lang_imp.module.clone()),
                            name: "*".to_string(),
                            alias: lang_imp.alias.clone(),
                            line: lang_imp.line,
                        });
                    } else if lang_imp.names.is_empty() {
                        // import X (no specific names) - module is the imported thing
                        imports.push(FlatImport {
                            module: None,
                            name: lang_imp.module.clone(),
                            alias: lang_imp.alias.clone(),
                            line: lang_imp.line,
                        });
                    } else {
                        // from X import a, b, c - each name gets an entry
                        for name in &lang_imp.names {
                            imports.push(FlatImport {
                                module: Some(lang_imp.module.clone()),
                                name: name.clone(),
                                alias: None, // alias applies to whole import, not individual names
                                line: lang_imp.line,
                            });
                        }
                    }
                }
            }

            // Recurse into children
            if cursor.goto_first_child() {
                self.collect_imports_with_trait(cursor, content, support, imports);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    /// Find a symbol by name in a file
    pub fn find_symbol(&mut self, path: &Path, content: &str, name: &str) -> Option<FlatSymbol> {
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
        let tree = match parsers::parse_with_grammar("python", source) {
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
    /// Use this when you already have the FlatSymbol from parse_file()
    pub fn find_callees_for_symbol(
        &mut self,
        path: &Path,
        content: &str,
        symbol: &FlatSymbol,
    ) -> Vec<(String, usize, Option<String>)> {
        let lines: Vec<&str> = content.lines().collect();
        let start = symbol.start_line.saturating_sub(1);
        let end = symbol.end_line.min(lines.len());
        let source = lines[start..end].join("\n");

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match ext {
            "py" => self.find_python_calls_with_lines(&source, symbol.start_line),
            "rs" => self.find_rust_calls_with_lines(&source, symbol.start_line),
            "ts" | "tsx" => {
                self.find_typescript_calls_with_lines(&source, symbol.start_line, ext == "tsx")
            }
            "js" | "mjs" | "cjs" => {
                self.find_javascript_calls_with_lines(&source, symbol.start_line)
            }
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
        let tree = match parsers::parse_with_grammar("python", source) {
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
        let tree = match parsers::parse_with_grammar("rust", source) {
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
        let grammar = if is_tsx { "tsx" } else { "typescript" };
        let tree = match parsers::parse_with_grammar(grammar, source) {
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
        let tree = match parsers::parse_with_grammar("javascript", source) {
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
        let tree = match parsers::parse_with_grammar("java", source) {
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
        let tree = match parsers::parse_with_grammar("go", source) {
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
        let tree = match parsers::parse_with_grammar("rust", source) {
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
                let callees = self.find_callees_for_symbol(&full_path, &content, &symbol);
                // Check if any callee matches, considering qualifiers
                let is_caller = callees.iter().any(|(name, _, qualifier)| {
                    if name != symbol_name {
                        return false;
                    }
                    // Match if: no qualifier, or qualifier is self/Self
                    match qualifier {
                        None => true,
                        Some(q) => q == "self" || q == "Self",
                    }
                });
                if is_caller {
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
        let parser = SymbolParser::new();
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
        let parser = SymbolParser::new();
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
        let parser = SymbolParser::new();
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
