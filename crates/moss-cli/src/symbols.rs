use moss_core::{tree_sitter, Language, Parsers};
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
        let lang = Language::from_path(path);

        match lang {
            Some(Language::Python) => self.parse_python(content),
            Some(Language::Rust) => self.parse_rust(content),
            Some(Language::Java) => self.parse_java(content),
            Some(Language::TypeScript) => self.parse_typescript(content),
            Some(Language::Tsx) => self.parse_tsx(content),
            Some(Language::JavaScript) => self.parse_javascript(content),
            Some(Language::Go) => self.parse_go(content),
            Some(Language::Json) => self.parse_json(content),
            Some(Language::Yaml) => self.parse_yaml(content),
            Some(Language::Toml) => self.parse_toml(content),
            _ => Vec::new(),
        }
    }

    fn parse_python(&self, content: &str) -> Vec<Symbol> {
        let tree = match self.parsers.parse_lang(Language::Python, content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut symbols = Vec::new();
        let root = tree.root_node();

        // Walk the tree to find functions, classes, methods
        let mut cursor = root.walk();
        self.collect_python_symbols(&mut cursor, content, &mut symbols, None);

        symbols
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

    fn collect_python_symbols(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        symbols: &mut Vec<Symbol>,
        parent: Option<&str>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            match kind {
                "function_definition" | "async_function_definition" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = &content[name_node.byte_range()];
                        let symbol_kind = if parent.is_some() {
                            SymbolKind::Method
                        } else {
                            SymbolKind::Function
                        };
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: symbol_kind,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            parent: parent.map(String::from),
                        });
                    }
                }
                "class_definition" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = &content[name_node.byte_range()];
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: SymbolKind::Class,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            parent: parent.map(String::from),
                        });

                        // Recurse into class body to find methods
                        if cursor.goto_first_child() {
                            self.collect_python_symbols(cursor, content, symbols, Some(name));
                            cursor.goto_parent();
                        }
                        if cursor.goto_next_sibling() {
                            continue;
                        }
                        break;
                    }
                }
                _ => {}
            }

            // Recurse into children (but not for class definitions, handled above)
            if kind != "class_definition" && cursor.goto_first_child() {
                self.collect_python_symbols(cursor, content, symbols, parent);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn parse_rust(&self, content: &str) -> Vec<Symbol> {
        let tree = match self.parsers.parse_lang(Language::Rust, content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut symbols = Vec::new();
        let root = tree.root_node();

        let mut cursor = root.walk();
        self.collect_rust_symbols(&mut cursor, content, &mut symbols, None);

        symbols
    }

    fn collect_rust_symbols(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        symbols: &mut Vec<Symbol>,
        parent: Option<&str>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            match kind {
                "function_item" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = &content[name_node.byte_range()];
                        let symbol_kind = if parent.is_some() {
                            SymbolKind::Method
                        } else {
                            SymbolKind::Function
                        };
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: symbol_kind,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            parent: parent.map(String::from),
                        });
                    }
                }
                "struct_item" | "enum_item" | "trait_item" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = &content[name_node.byte_range()];
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: SymbolKind::Class, // Use Class for struct/enum/trait
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            parent: parent.map(String::from),
                        });
                    }
                }
                "impl_item" => {
                    // Find the type being implemented
                    let impl_name = node
                        .child_by_field_name("type")
                        .map(|n| content[n.byte_range()].to_string());

                    if let Some(name) = &impl_name {
                        // Recurse into impl block to find methods
                        if cursor.goto_first_child() {
                            self.collect_rust_symbols(cursor, content, symbols, Some(name));
                            cursor.goto_parent();
                        }
                        if cursor.goto_next_sibling() {
                            continue;
                        }
                        break;
                    }
                }
                _ => {}
            }

            // Recurse into children (but not for impl blocks, handled above)
            if kind != "impl_item" && cursor.goto_first_child() {
                self.collect_rust_symbols(cursor, content, symbols, parent);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn parse_java(&self, content: &str) -> Vec<Symbol> {
        let tree = match self.parsers.parse_lang(Language::Java, content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();
        self.collect_java_symbols(&mut cursor, content, &mut symbols, None);
        symbols
    }

    fn collect_java_symbols(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        symbols: &mut Vec<Symbol>,
        parent: Option<&str>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            match kind {
                "class_declaration" | "interface_declaration" | "enum_declaration" | "record_declaration" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = &content[name_node.byte_range()];
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: SymbolKind::Class,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            parent: parent.map(String::from),
                        });

                        // Recurse into class body to find methods
                        if cursor.goto_first_child() {
                            self.collect_java_symbols(cursor, content, symbols, Some(name));
                            cursor.goto_parent();
                        }
                        if cursor.goto_next_sibling() {
                            continue;
                        }
                        break;
                    }
                }
                "method_declaration" | "constructor_declaration" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = &content[name_node.byte_range()];
                        let symbol_kind = if parent.is_some() {
                            SymbolKind::Method
                        } else {
                            SymbolKind::Function
                        };
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: symbol_kind,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            parent: parent.map(String::from),
                        });
                    }
                }
                _ => {}
            }

            // Recurse into children (skip class bodies handled above)
            let dominated = matches!(kind, "class_declaration" | "interface_declaration" | "enum_declaration" | "record_declaration");
            if !dominated && cursor.goto_first_child() {
                self.collect_java_symbols(cursor, content, symbols, parent);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn parse_typescript(&self, content: &str) -> Vec<Symbol> {
        let tree = match self.parsers.parse_lang(Language::TypeScript, content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();
        self.collect_ts_symbols(&mut cursor, content, &mut symbols, None);
        symbols
    }

    fn parse_tsx(&self, content: &str) -> Vec<Symbol> {
        let tree = match self.parsers.parse_lang(Language::Tsx, content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();
        self.collect_ts_symbols(&mut cursor, content, &mut symbols, None);
        symbols
    }

    fn parse_javascript(&self, content: &str) -> Vec<Symbol> {
        let tree = match self.parsers.parse_lang(Language::JavaScript, content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();
        self.collect_ts_symbols(&mut cursor, content, &mut symbols, None); // Same AST structure
        symbols
    }

    fn collect_ts_symbols(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        symbols: &mut Vec<Symbol>,
        parent: Option<&str>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            match kind {
                "class_declaration" | "abstract_class_declaration" | "interface_declaration" | "enum_declaration" | "type_alias_declaration" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = &content[name_node.byte_range()];
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: SymbolKind::Class,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            parent: parent.map(String::from),
                        });

                        // Recurse into class body
                        if cursor.goto_first_child() {
                            self.collect_ts_symbols(cursor, content, symbols, Some(name));
                            cursor.goto_parent();
                        }
                        if cursor.goto_next_sibling() {
                            continue;
                        }
                        break;
                    }
                }
                "function_declaration" | "generator_function_declaration" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = &content[name_node.byte_range()];
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: SymbolKind::Function,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            parent: parent.map(String::from),
                        });
                    }
                }
                "method_definition" | "public_field_definition" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = &content[name_node.byte_range()];
                        // Skip computed property names like [Symbol.iterator]
                        if !name.starts_with('[') {
                            symbols.push(Symbol {
                                name: name.to_string(),
                                kind: SymbolKind::Method,
                                start_line: node.start_position().row + 1,
                                end_line: node.end_position().row + 1,
                                parent: parent.map(String::from),
                            });
                        }
                    }
                }
                // Arrow functions and function expressions assigned to variables
                "lexical_declaration" | "variable_declaration" => {
                    // Look for const foo = () => {} or const foo = function() {}
                    for i in 0..node.child_count() {
                        if let Some(decl) = node.child(i) {
                            if decl.kind() == "variable_declarator" {
                                if let (Some(name_node), Some(value_node)) = (
                                    decl.child_by_field_name("name"),
                                    decl.child_by_field_name("value"),
                                ) {
                                    let value_kind = value_node.kind();
                                    if matches!(value_kind, "arrow_function" | "function_expression" | "function") {
                                        let name = &content[name_node.byte_range()];
                                        symbols.push(Symbol {
                                            name: name.to_string(),
                                            kind: SymbolKind::Function,
                                            start_line: node.start_position().row + 1,
                                            end_line: node.end_position().row + 1,
                                            parent: parent.map(String::from),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }

            let dominated = matches!(kind, "class_declaration" | "abstract_class_declaration" | "interface_declaration" | "enum_declaration");
            if !dominated && cursor.goto_first_child() {
                self.collect_ts_symbols(cursor, content, symbols, parent);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn parse_go(&self, content: &str) -> Vec<Symbol> {
        let tree = match self.parsers.parse_lang(Language::Go, content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();
        self.collect_go_symbols(&mut cursor, content, &mut symbols, None);
        symbols
    }

    fn collect_go_symbols(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        symbols: &mut Vec<Symbol>,
        parent: Option<&str>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            match kind {
                "function_declaration" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = &content[name_node.byte_range()];
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: SymbolKind::Function,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            parent: parent.map(String::from),
                        });
                    }
                }
                "method_declaration" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = &content[name_node.byte_range()];
                        // Try to get receiver type as parent
                        let receiver_type = node
                            .child_by_field_name("receiver")
                            .and_then(|r| {
                                // Receiver is (name Type) or (name *Type)
                                for i in 0..r.child_count() {
                                    if let Some(c) = r.child(i) {
                                        let ck = c.kind();
                                        if ck == "type_identifier" || ck == "pointer_type" {
                                            return Some(content[c.byte_range()].trim_start_matches('*').to_string());
                                        }
                                    }
                                }
                                None
                            });
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: SymbolKind::Method,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            parent: receiver_type.or_else(|| parent.map(String::from)),
                        });
                    }
                }
                "type_declaration" => {
                    // type Foo struct { ... } or type Foo interface { ... }
                    for i in 0..node.child_count() {
                        if let Some(spec) = node.child(i) {
                            if spec.kind() == "type_spec" {
                                if let Some(name_node) = spec.child_by_field_name("name") {
                                    let name = &content[name_node.byte_range()];
                                    symbols.push(Symbol {
                                        name: name.to_string(),
                                        kind: SymbolKind::Class, // struct/interface as Class
                                        start_line: node.start_position().row + 1,
                                        end_line: node.end_position().row + 1,
                                        parent: parent.map(String::from),
                                    });
                                }
                            }
                        }
                    }
                }
                _ => {}
            }

            if cursor.goto_first_child() {
                self.collect_go_symbols(cursor, content, symbols, parent);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn parse_json(&self, content: &str) -> Vec<Symbol> {
        let tree = match self.parsers.parse_lang(Language::Json, content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();
        self.collect_json_keys(&mut cursor, content, &mut symbols, None);
        symbols
    }

    fn collect_json_keys(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        symbols: &mut Vec<Symbol>,
        parent: Option<&str>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            if kind == "pair" {
                if let Some(key_node) = node.child_by_field_name("key") {
                    // Key is a "string" node, get the content without quotes
                    let key_text = &content[key_node.byte_range()];
                    let key_name = key_text.trim_matches('"');

                    // Check if value is an object (has nested keys)
                    let is_object = node
                        .child_by_field_name("value")
                        .map(|v| v.kind() == "object")
                        .unwrap_or(false);

                    symbols.push(Symbol {
                        name: key_name.to_string(),
                        kind: if is_object { SymbolKind::Class } else { SymbolKind::Variable },
                        start_line: node.start_position().row + 1,
                        end_line: node.end_position().row + 1,
                        parent: parent.map(String::from),
                    });

                    // Recurse into object values
                    if is_object {
                        if cursor.goto_first_child() {
                            self.collect_json_keys(cursor, content, symbols, Some(key_name));
                            cursor.goto_parent();
                        }
                        if cursor.goto_next_sibling() {
                            continue;
                        }
                        break;
                    }
                }
            }

            // Recurse into children
            if kind != "pair" && cursor.goto_first_child() {
                self.collect_json_keys(cursor, content, symbols, parent);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn parse_yaml(&self, content: &str) -> Vec<Symbol> {
        let tree = match self.parsers.parse_lang(Language::Yaml, content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();
        self.collect_yaml_keys(&mut cursor, content, &mut symbols, None);
        symbols
    }

    fn collect_yaml_keys(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        symbols: &mut Vec<Symbol>,
        parent: Option<&str>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            // YAML uses "block_mapping_pair" for key-value pairs
            if kind == "block_mapping_pair" || kind == "flow_pair" {
                if let Some(key_node) = node.child_by_field_name("key") {
                    let key_text = &content[key_node.byte_range()];
                    // Remove quotes if present
                    let key_name = key_text.trim_matches(|c| c == '"' || c == '\'').trim();

                    if !key_name.is_empty() {
                        // Check if value is a block_node with block_mapping (nested object)
                        let is_object = node
                            .child_by_field_name("value")
                            .map(|v| {
                                v.kind() == "block_node" || v.kind() == "flow_node" ||
                                v.kind() == "block_mapping" || v.kind() == "flow_mapping"
                            })
                            .unwrap_or(false);

                        symbols.push(Symbol {
                            name: key_name.to_string(),
                            kind: if is_object { SymbolKind::Class } else { SymbolKind::Variable },
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            parent: parent.map(String::from),
                        });

                        // Recurse into nested mappings
                        if is_object {
                            if cursor.goto_first_child() {
                                self.collect_yaml_keys(cursor, content, symbols, Some(key_name));
                                cursor.goto_parent();
                            }
                            if cursor.goto_next_sibling() {
                                continue;
                            }
                            break;
                        }
                    }
                }
            }

            // Recurse
            let dominated = kind == "block_mapping_pair" || kind == "flow_pair";
            if !dominated && cursor.goto_first_child() {
                self.collect_yaml_keys(cursor, content, symbols, parent);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn parse_toml(&self, content: &str) -> Vec<Symbol> {
        let tree = match self.parsers.parse_lang(Language::Toml, content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut symbols = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();
        self.collect_toml_keys(&mut cursor, content, &mut symbols, None);
        symbols
    }

    fn collect_toml_keys(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        symbols: &mut Vec<Symbol>,
        parent: Option<&str>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            match kind {
                // [section] or [[array_of_tables]]
                "table" | "table_array_element" => {
                    // Get the section name from child nodes
                    for i in 0..node.child_count() {
                        if let Some(child) = node.child(i) {
                            if child.kind() == "dotted_key" || child.kind() == "key" {
                                let key_text = &content[child.byte_range()];
                                symbols.push(Symbol {
                                    name: key_text.to_string(),
                                    kind: SymbolKind::Class,
                                    start_line: node.start_position().row + 1,
                                    end_line: node.end_position().row + 1,
                                    parent: parent.map(String::from),
                                });
                                // Recurse with this section as parent
                                if cursor.goto_first_child() {
                                    self.collect_toml_keys(cursor, content, symbols, Some(key_text));
                                    cursor.goto_parent();
                                }
                                break;
                            }
                        }
                    }
                    if cursor.goto_next_sibling() {
                        continue;
                    }
                    break;
                }
                // key = value pairs
                "pair" => {
                    for i in 0..node.child_count() {
                        if let Some(child) = node.child(i) {
                            if child.kind() == "dotted_key" || child.kind() == "bare_key" || child.kind() == "quoted_key" {
                                let key_text = &content[child.byte_range()];
                                let key_name = key_text.trim_matches(|c| c == '"' || c == '\'');
                                symbols.push(Symbol {
                                    name: key_name.to_string(),
                                    kind: SymbolKind::Variable,
                                    start_line: node.start_position().row + 1,
                                    end_line: node.end_position().row + 1,
                                    parent: parent.map(String::from),
                                });
                                break;
                            }
                        }
                    }
                }
                _ => {}
            }

            // Recurse into children
            let dominated = matches!(kind, "table" | "table_array_element");
            if !dominated && cursor.goto_first_child() {
                self.collect_toml_keys(cursor, content, symbols, parent);
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
