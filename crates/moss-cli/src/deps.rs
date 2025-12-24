//! Module dependency extraction.
//!
//! Extracts imports and exports from source files.

use moss_core::{tree_sitter, Language, Parsers};
use std::path::Path;

/// An import statement
#[derive(Debug, Clone)]
pub struct Import {
    pub module: String,
    pub names: Vec<String>, // Names imported (empty for "import x")
    pub alias: Option<String>,
    pub line: usize,
    pub is_relative: bool,
}

/// An exported symbol
#[derive(Debug, Clone)]
pub struct Export {
    pub name: String,
    pub kind: &'static str, // "function", "class", "variable"
    pub line: usize,
}

/// A re-export statement (export * from './module' or export { foo } from './module')
#[derive(Debug, Clone)]
pub struct ReExport {
    pub module: String,
    pub names: Vec<String>, // Empty for "export * from", specific names for "export { x } from"
    pub is_star: bool,      // true for "export * from"
    pub line: usize,
}

/// Dependency information for a file
#[allow(dead_code)] // file_path provides context; format() is API method
pub struct DepsResult {
    pub imports: Vec<Import>,
    pub exports: Vec<Export>,
    pub reexports: Vec<ReExport>,
    pub file_path: String,
}

impl DepsResult {
    /// Format as compact text
    #[allow(dead_code)] // API method for CLI output
    pub fn format(&self) -> String {
        let mut lines = Vec::new();

        if !self.imports.is_empty() {
            lines.push("# Imports".to_string());
            for imp in &self.imports {
                let prefix = if imp.is_relative {
                    format!(".{}", imp.module)
                } else {
                    imp.module.clone()
                };

                if imp.names.is_empty() {
                    let alias = imp
                        .alias
                        .as_ref()
                        .map(|a| format!(" as {}", a))
                        .unwrap_or_default();
                    lines.push(format!("import {}{}", prefix, alias));
                } else {
                    lines.push(format!("from {} import {}", prefix, imp.names.join(", ")));
                }
            }
            lines.push(String::new());
        }

        if !self.exports.is_empty() {
            lines.push("# Exports".to_string());
            for exp in &self.exports {
                if exp.kind != "variable" {
                    lines.push(format!("{}: {}", exp.kind, exp.name));
                }
            }
            lines.push(String::new());
        }

        if !self.reexports.is_empty() {
            lines.push("# Re-exports".to_string());
            for reexp in &self.reexports {
                if reexp.is_star {
                    lines.push(format!("export * from '{}'", reexp.module));
                } else {
                    lines.push(format!(
                        "export {{ {} }} from '{}'",
                        reexp.names.join(", "),
                        reexp.module
                    ));
                }
            }
        }

        lines.join("\n").trim_end().to_string()
    }
}

pub struct DepsExtractor {
    parsers: Parsers,
}

impl DepsExtractor {
    pub fn new() -> Self {
        Self {
            parsers: Parsers::new(),
        }
    }

    pub fn extract(&self, path: &Path, content: &str) -> DepsResult {
        let lang = Language::from_path(path);
        let (imports, exports, reexports) = match lang {
            Some(Language::Python) => {
                let (i, e) = self.extract_python(content);
                (i, e, Vec::new())
            }
            Some(Language::Rust) => {
                let (i, e) = self.extract_rust(content);
                (i, e, Vec::new())
            }
            Some(Language::TypeScript) => self.extract_typescript(content),
            Some(Language::Tsx) => self.extract_tsx(content),
            Some(Language::JavaScript) => self.extract_javascript(content),
            Some(Language::Go) => {
                let (i, e) = self.extract_go(content);
                (i, e, Vec::new())
            }
            _ => (Vec::new(), Vec::new(), Vec::new()),
        };

        DepsResult {
            imports,
            exports,
            reexports,
            file_path: path.to_string_lossy().to_string(),
        }
    }

    fn extract_python(&self, content: &str) -> (Vec<Import>, Vec<Export>) {
        let tree = match self.parsers.parse_lang(Language::Python, content) {
            Some(t) => t,
            None => return (Vec::new(), Vec::new()),
        };

        let mut imports = Vec::new();
        let mut exports = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        self.collect_python_deps(&mut cursor, content, &mut imports, &mut exports, false);
        (imports, exports)
    }

    fn collect_python_deps(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        imports: &mut Vec<Import>,
        exports: &mut Vec<Export>,
        in_class: bool,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            match kind {
                "import_statement" => {
                    // import x, import x as y
                    for i in 0..node.child_count() {
                        if let Some(child) = node.child(i) {
                            if child.kind() == "dotted_name" {
                                let module = content[child.byte_range()].to_string();
                                imports.push(Import {
                                    module,
                                    names: Vec::new(),
                                    alias: None,
                                    line: node.start_position().row + 1,
                                    is_relative: false,
                                });
                            } else if child.kind() == "aliased_import" {
                                let name_node = child.child_by_field_name("name");
                                let alias_node = child.child_by_field_name("alias");
                                if let Some(name) = name_node {
                                    let module = content[name.byte_range()].to_string();
                                    let alias =
                                        alias_node.map(|a| content[a.byte_range()].to_string());
                                    imports.push(Import {
                                        module,
                                        names: Vec::new(),
                                        alias,
                                        line: node.start_position().row + 1,
                                        is_relative: false,
                                    });
                                }
                            }
                        }
                    }
                }
                "import_from_statement" => {
                    // from x import y, z
                    let module_node = node.child_by_field_name("module_name");
                    let module = module_node
                        .map(|n| content[n.byte_range()].to_string())
                        .unwrap_or_default();

                    // Check for relative import (starts with .)
                    let text = &content[node.byte_range()];
                    let is_relative = text.contains("from .");

                    let mut names = Vec::new();
                    for i in 0..node.child_count() {
                        if let Some(child) = node.child(i) {
                            if child.kind() == "import_name" || child.kind() == "dotted_name" {
                                // Skip the module name
                                if Some(child) != module_node {
                                    names.push(content[child.byte_range()].to_string());
                                }
                            } else if child.kind() == "aliased_import" {
                                if let Some(name) = child.child_by_field_name("name") {
                                    names.push(content[name.byte_range()].to_string());
                                }
                            }
                        }
                    }

                    imports.push(Import {
                        module,
                        names,
                        alias: None,
                        line: node.start_position().row + 1,
                        is_relative,
                    });
                }
                "function_definition" | "async_function_definition" => {
                    if !in_class {
                        if let Some(name_node) = node.child_by_field_name("name") {
                            let name = content[name_node.byte_range()].to_string();
                            if !name.starts_with('_') {
                                exports.push(Export {
                                    name,
                                    kind: "function",
                                    line: node.start_position().row + 1,
                                });
                            }
                        }
                    }
                }
                "class_definition" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = content[name_node.byte_range()].to_string();
                        if !name.starts_with('_') {
                            exports.push(Export {
                                name,
                                kind: "class",
                                line: node.start_position().row + 1,
                            });
                        }
                    }
                    // Mark that we're inside a class
                    if cursor.goto_first_child() {
                        self.collect_python_deps(cursor, content, imports, exports, true);
                        cursor.goto_parent();
                    }
                    if cursor.goto_next_sibling() {
                        continue;
                    }
                    break;
                }
                _ => {}
            }

            // Recurse
            if kind != "class_definition" && cursor.goto_first_child() {
                self.collect_python_deps(cursor, content, imports, exports, in_class);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn extract_rust(&self, content: &str) -> (Vec<Import>, Vec<Export>) {
        let tree = match self.parsers.parse_lang(Language::Rust, content) {
            Some(t) => t,
            None => return (Vec::new(), Vec::new()),
        };

        let mut imports = Vec::new();
        let mut exports = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        self.collect_rust_deps(&mut cursor, content, &mut imports, &mut exports);
        (imports, exports)
    }

    fn collect_rust_deps(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        imports: &mut Vec<Import>,
        exports: &mut Vec<Export>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            match kind {
                "use_declaration" => {
                    let text = &content[node.byte_range()];
                    // Extract module path (simplified)
                    let module = text.trim_start_matches("use ").trim_end_matches(';').trim();

                    // Extract names if it's a use with braces
                    let mut names = Vec::new();
                    if module.contains('{') {
                        if let Some(brace_start) = module.find('{') {
                            let prefix = &module[..brace_start].trim_end_matches("::");
                            if let Some(brace_end) = module.find('}') {
                                let items = &module[brace_start + 1..brace_end];
                                for item in items.split(',') {
                                    names.push(item.trim().to_string());
                                }
                            }
                            imports.push(Import {
                                module: prefix.to_string(),
                                names,
                                alias: None,
                                line: node.start_position().row + 1,
                                is_relative: prefix.starts_with("crate")
                                    || prefix.starts_with("self")
                                    || prefix.starts_with("super"),
                            });
                        }
                    } else {
                        imports.push(Import {
                            module: module.to_string(),
                            names: Vec::new(),
                            alias: None,
                            line: node.start_position().row + 1,
                            is_relative: module.starts_with("crate")
                                || module.starts_with("self")
                                || module.starts_with("super"),
                        });
                    }
                }
                "function_item" => {
                    // Check for pub
                    let mut is_pub = false;
                    for i in 0..node.child_count() {
                        if let Some(child) = node.child(i) {
                            if child.kind() == "visibility_modifier" {
                                is_pub = content[child.byte_range()].contains("pub");
                                break;
                            }
                        }
                    }
                    if is_pub {
                        if let Some(name_node) = node.child_by_field_name("name") {
                            let name = content[name_node.byte_range()].to_string();
                            exports.push(Export {
                                name,
                                kind: "function",
                                line: node.start_position().row + 1,
                            });
                        }
                    }
                }
                "struct_item" | "enum_item" | "trait_item" => {
                    let mut is_pub = false;
                    for i in 0..node.child_count() {
                        if let Some(child) = node.child(i) {
                            if child.kind() == "visibility_modifier" {
                                is_pub = content[child.byte_range()].contains("pub");
                                break;
                            }
                        }
                    }
                    if is_pub {
                        if let Some(name_node) = node.child_by_field_name("name") {
                            let name = content[name_node.byte_range()].to_string();
                            let item_kind = match kind {
                                "struct_item" => "struct",
                                "enum_item" => "enum",
                                "trait_item" => "trait",
                                _ => "type",
                            };
                            exports.push(Export {
                                name,
                                kind: item_kind,
                                line: node.start_position().row + 1,
                            });
                        }
                    }
                }
                _ => {}
            }

            // Recurse
            if cursor.goto_first_child() {
                self.collect_rust_deps(cursor, content, imports, exports);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn extract_typescript(&self, content: &str) -> (Vec<Import>, Vec<Export>, Vec<ReExport>) {
        let tree = match self.parsers.parse_lang(Language::TypeScript, content) {
            Some(t) => t,
            None => return (Vec::new(), Vec::new(), Vec::new()),
        };
        self.extract_js_ts_deps(&tree, content)
    }

    fn extract_tsx(&self, content: &str) -> (Vec<Import>, Vec<Export>, Vec<ReExport>) {
        let tree = match self.parsers.parse_lang(Language::Tsx, content) {
            Some(t) => t,
            None => return (Vec::new(), Vec::new(), Vec::new()),
        };
        self.extract_js_ts_deps(&tree, content)
    }

    fn extract_javascript(&self, content: &str) -> (Vec<Import>, Vec<Export>, Vec<ReExport>) {
        let tree = match self.parsers.parse_lang(Language::JavaScript, content) {
            Some(t) => t,
            None => return (Vec::new(), Vec::new(), Vec::new()),
        };
        self.extract_js_ts_deps(&tree, content)
    }

    fn extract_go(&self, content: &str) -> (Vec<Import>, Vec<Export>) {
        let tree = match self.parsers.parse_lang(Language::Go, content) {
            Some(t) => t,
            None => return (Vec::new(), Vec::new()),
        };

        let mut imports = Vec::new();
        let mut exports = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        self.collect_go_deps(&mut cursor, content, &mut imports, &mut exports);
        (imports, exports)
    }

    fn collect_go_deps(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        imports: &mut Vec<Import>,
        exports: &mut Vec<Export>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            match kind {
                "import_declaration" => {
                    // Handle both single and grouped imports
                    self.collect_go_imports(node, content, imports);
                }
                "function_declaration" => {
                    // Exported if name starts with uppercase
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = &content[name_node.byte_range()];
                        if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                            exports.push(Export {
                                name: name.to_string(),
                                kind: "function",
                                line: node.start_position().row + 1,
                            });
                        }
                    }
                }
                "method_declaration" => {
                    // Exported if name starts with uppercase
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = &content[name_node.byte_range()];
                        if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                            exports.push(Export {
                                name: name.to_string(),
                                kind: "method",
                                line: node.start_position().row + 1,
                            });
                        }
                    }
                }
                "type_declaration" => {
                    // type Foo struct { ... } or type Bar interface { ... }
                    for i in 0..node.child_count() {
                        if let Some(child) = node.child(i) {
                            if child.kind() == "type_spec" {
                                if let Some(name_node) = child.child_by_field_name("name") {
                                    let name = &content[name_node.byte_range()];
                                    if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
                                    {
                                        exports.push(Export {
                                            name: name.to_string(),
                                            kind: "type",
                                            line: child.start_position().row + 1,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                "const_declaration" | "var_declaration" => {
                    // const Foo = ... or var Bar = ...
                    for i in 0..node.child_count() {
                        if let Some(child) = node.child(i) {
                            if child.kind() == "const_spec" || child.kind() == "var_spec" {
                                if let Some(name_node) = child.child_by_field_name("name") {
                                    let name = &content[name_node.byte_range()];
                                    if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
                                    {
                                        let export_kind = if kind == "const_declaration" {
                                            "constant"
                                        } else {
                                            "variable"
                                        };
                                        exports.push(Export {
                                            name: name.to_string(),
                                            kind: export_kind,
                                            line: child.start_position().row + 1,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }

            // Recurse into children
            if cursor.goto_first_child() {
                self.collect_go_deps(cursor, content, imports, exports);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn collect_go_imports(
        &self,
        node: tree_sitter::Node,
        content: &str,
        imports: &mut Vec<Import>,
    ) {
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                match child.kind() {
                    "import_spec" => {
                        // Single import or import within group
                        let mut alias = None;
                        let mut path = String::new();

                        if let Some(name_node) = child.child_by_field_name("name") {
                            alias = Some(content[name_node.byte_range()].to_string());
                        }
                        if let Some(path_node) = child.child_by_field_name("path") {
                            let text = &content[path_node.byte_range()];
                            path = text.trim_matches('"').to_string();
                        }

                        if !path.is_empty() {
                            imports.push(Import {
                                module: path,
                                names: Vec::new(),
                                alias,
                                line: child.start_position().row + 1,
                                is_relative: false, // Go doesn't have relative imports
                            });
                        }
                    }
                    "import_spec_list" => {
                        // Grouped imports: import ( ... )
                        self.collect_go_imports(child, content, imports);
                    }
                    "interpreted_string_literal" => {
                        // Simple import: import "fmt"
                        let text = &content[child.byte_range()];
                        let path = text.trim_matches('"').to_string();
                        if !path.is_empty() {
                            imports.push(Import {
                                module: path,
                                names: Vec::new(),
                                alias: None,
                                line: child.start_position().row + 1,
                                is_relative: false,
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// Shared extraction for JavaScript/TypeScript AST
    fn extract_js_ts_deps(
        &self,
        tree: &tree_sitter::Tree,
        content: &str,
    ) -> (Vec<Import>, Vec<Export>, Vec<ReExport>) {
        let mut imports = Vec::new();
        let mut exports = Vec::new();
        let mut reexports = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        self.collect_js_ts_deps(&mut cursor, content, &mut imports, &mut exports, &mut reexports);
        (imports, exports, reexports)
    }

    fn collect_js_ts_deps(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        imports: &mut Vec<Import>,
        exports: &mut Vec<Export>,
        reexports: &mut Vec<ReExport>,
    ) {
        loop {
            let node = cursor.node();
            let kind = node.kind();

            match kind {
                // import { foo, bar } from './module'
                // import foo from './module'
                // import * as foo from './module'
                "import_statement" => {
                    let mut module = String::new();
                    let mut names = Vec::new();

                    for i in 0..node.child_count() {
                        if let Some(child) = node.child(i) {
                            match child.kind() {
                                "string" | "string_fragment" => {
                                    // Extract module path (remove quotes)
                                    let text = &content[child.byte_range()];
                                    module = text.trim_matches(|c| c == '"' || c == '\'').to_string();
                                }
                                "import_clause" => {
                                    // Extract imported names
                                    self.collect_import_names(child, content, &mut names);
                                }
                                _ => {}
                            }
                        }
                    }

                    if !module.is_empty() {
                        let is_relative = module.starts_with('.');
                        imports.push(Import {
                            module,
                            names,
                            alias: None,
                            line: node.start_position().row + 1,
                            is_relative,
                        });
                    }
                }
                // export function foo() {}
                // export class Bar {}
                // export const baz = ...
                // export * from './module'
                // export { foo, bar } from './module'
                // export * as helpers from './helpers'
                "export_statement" => {
                    // Check if this is a re-export (has a source module)
                    let mut source_module = None;
                    let mut is_star = false;
                    let mut named_exports: Vec<String> = Vec::new();

                    for i in 0..node.child_count() {
                        if let Some(child) = node.child(i) {
                            match child.kind() {
                                "string" => {
                                    // The source module in 'export ... from "module"'
                                    let text = &content[child.byte_range()];
                                    source_module =
                                        Some(text.trim_matches(|c| c == '"' || c == '\'').to_string());
                                }
                                "*" => {
                                    // export * from './module'
                                    is_star = true;
                                }
                                "namespace_export" => {
                                    // export * as foo from './module'
                                    is_star = true;
                                }
                                "export_clause" => {
                                    // export { foo, bar } from './module'
                                    self.collect_export_clause_names(child, content, &mut named_exports);
                                }
                                "function_declaration" | "generator_function_declaration" => {
                                    if let Some(name_node) = child.child_by_field_name("name") {
                                        exports.push(Export {
                                            name: content[name_node.byte_range()].to_string(),
                                            kind: "function",
                                            line: node.start_position().row + 1,
                                        });
                                    }
                                }
                                "class_declaration" => {
                                    if let Some(name_node) = child.child_by_field_name("name") {
                                        exports.push(Export {
                                            name: content[name_node.byte_range()].to_string(),
                                            kind: "class",
                                            line: node.start_position().row + 1,
                                        });
                                    }
                                }
                                "lexical_declaration" => {
                                    // export const foo = ..., bar = ...
                                    self.collect_variable_names(
                                        child,
                                        content,
                                        exports,
                                        node.start_position().row + 1,
                                    );
                                }
                                _ => {}
                            }
                        }
                    }

                    // If we found a source module, this is a re-export
                    if let Some(module) = source_module {
                        reexports.push(ReExport {
                            module,
                            names: named_exports,
                            is_star,
                            line: node.start_position().row + 1,
                        });
                    }
                }
                // Top-level function/class (could be exported via export default later)
                "function_declaration" | "generator_function_declaration" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = content[name_node.byte_range()].to_string();
                        if !name.starts_with('_') {
                            exports.push(Export {
                                name,
                                kind: "function",
                                line: node.start_position().row + 1,
                            });
                        }
                    }
                }
                "class_declaration" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = content[name_node.byte_range()].to_string();
                        if !name.starts_with('_') {
                            exports.push(Export {
                                name,
                                kind: "class",
                                line: node.start_position().row + 1,
                            });
                        }
                    }
                }
                _ => {}
            }

            // Recurse into children
            if cursor.goto_first_child() {
                self.collect_js_ts_deps(cursor, content, imports, exports, reexports);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    /// Collect names from export clause: export { foo, bar } from ...
    fn collect_export_clause_names(
        &self,
        node: tree_sitter::Node,
        content: &str,
        names: &mut Vec<String>,
    ) {
        // Walk through children directly
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                match child.kind() {
                    "export_specifier" => {
                        // { foo as bar } - get the first identifier (original name)
                        // or check for "name" field
                        if let Some(name) = child.child_by_field_name("name") {
                            names.push(content[name.byte_range()].to_string());
                        } else {
                            // Find first identifier child
                            for j in 0..child.child_count() {
                                if let Some(id) = child.child(j) {
                                    if id.kind() == "identifier" {
                                        names.push(content[id.byte_range()].to_string());
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    _ => {
                        // Recurse into other nodes
                        self.collect_export_clause_names(child, content, names);
                    }
                }
            }
        }
    }

    fn collect_import_names(&self, node: tree_sitter::Node, content: &str, names: &mut Vec<String>) {
        let mut cursor = node.walk();
        loop {
            let child = cursor.node();
            match child.kind() {
                "identifier" => {
                    names.push(content[child.byte_range()].to_string());
                }
                "import_specifier" => {
                    // { foo as bar } - we want "foo"
                    if let Some(name) = child.child_by_field_name("name") {
                        names.push(content[name.byte_range()].to_string());
                    }
                }
                "namespace_import" => {
                    // import * as foo - we want "foo"
                    for i in 0..child.child_count() {
                        if let Some(id) = child.child(i) {
                            if id.kind() == "identifier" {
                                names.push(content[id.byte_range()].to_string());
                            }
                        }
                    }
                }
                _ => {}
            }

            if cursor.goto_first_child() {
                self.collect_import_names(cursor.node(), content, names);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn collect_variable_names(&self, node: tree_sitter::Node, content: &str, exports: &mut Vec<Export>, line: usize) {
        let mut cursor = node.walk();
        loop {
            let child = cursor.node();
            if child.kind() == "variable_declarator" {
                if let Some(name_node) = child.child_by_field_name("name") {
                    if name_node.kind() == "identifier" {
                        exports.push(Export {
                            name: content[name_node.byte_range()].to_string(),
                            kind: "variable",
                            line,
                        });
                    }
                }
            }

            if cursor.goto_first_child() {
                self.collect_variable_names(cursor.node(), content, exports, line);
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
    fn test_python_imports() {
        let mut extractor = DepsExtractor::new();
        let content = r#"
import os
import json as j
from pathlib import Path
from typing import Optional, List

def foo():
    pass

class Bar:
    pass
"#;
        let result = extractor.extract(&PathBuf::from("test.py"), content);

        assert!(result.imports.len() >= 3);
        assert!(result.exports.iter().any(|e| e.name == "foo"));
        assert!(result.exports.iter().any(|e| e.name == "Bar"));
    }

    #[test]
    fn test_rust_imports() {
        let mut extractor = DepsExtractor::new();
        let content = r#"
use std::path::Path;
use std::collections::{HashMap, HashSet};

pub fn foo() {}

pub struct Bar {}
"#;
        let result = extractor.extract(&PathBuf::from("test.rs"), content);

        assert!(result.imports.len() >= 2);
        assert!(result.exports.iter().any(|e| e.name == "foo"));
        assert!(result.exports.iter().any(|e| e.name == "Bar"));
    }

    #[test]
    fn test_typescript_imports() {
        let mut extractor = DepsExtractor::new();
        let content = r#"
import { foo, bar } from './utils';
import React from 'react';
import * as helpers from '../helpers';

export function greet(name: string): string {
    return `Hello, ${name}`;
}

export class User {
    name: string;
}

export const VERSION = "1.0.0";
"#;
        let result = extractor.extract(&PathBuf::from("test.ts"), content);

        assert!(result.imports.len() >= 2);
        assert!(result.imports.iter().any(|i| i.module == "./utils"));
        assert!(result.exports.iter().any(|e| e.name == "greet"));
        assert!(result.exports.iter().any(|e| e.name == "User"));
    }

    #[test]
    fn test_typescript_barrel_reexports() {
        let mut extractor = DepsExtractor::new();
        let content = r#"
export * from './utils';
export * as helpers from './helpers';
export { foo, bar } from './specific';
"#;
        let result = extractor.extract(&PathBuf::from("index.ts"), content);

        assert_eq!(result.reexports.len(), 3);

        // Star re-export
        let star = result.reexports.iter().find(|r| r.module == "./utils").unwrap();
        assert!(star.is_star);
        assert!(star.names.is_empty());

        // Namespace re-export (export * as helpers)
        let namespace = result.reexports.iter().find(|r| r.module == "./helpers").unwrap();
        assert!(namespace.is_star);

        // Named re-export
        let named = result.reexports.iter().find(|r| r.module == "./specific").unwrap();
        assert!(!named.is_star);
        assert!(named.names.contains(&"foo".to_string()));
        assert!(named.names.contains(&"bar".to_string()));
    }

    #[test]
    fn test_go_imports() {
        let mut extractor = DepsExtractor::new();
        let content = r#"
package main

import "fmt"

import (
    "os"
    "path/filepath"
    alias "github.com/user/pkg"
)

func main() {}

func PublicFunc() {}

func privateFunc() {}

type PublicType struct {}

type privateType struct {}

const PublicConst = 1

var PublicVar = "hello"
"#;
        let result = extractor.extract(&PathBuf::from("main.go"), content);

        // Check imports
        assert!(result.imports.iter().any(|i| i.module == "fmt"));
        assert!(result.imports.iter().any(|i| i.module == "os"));
        assert!(result.imports.iter().any(|i| i.module == "path/filepath"));
        assert!(result
            .imports
            .iter()
            .any(|i| i.module == "github.com/user/pkg" && i.alias == Some("alias".to_string())));

        // Check exports (only uppercase names are exported in Go)
        assert!(result.exports.iter().any(|e| e.name == "PublicFunc"));
        assert!(result.exports.iter().any(|e| e.name == "PublicType"));
        assert!(result.exports.iter().any(|e| e.name == "PublicConst"));
        assert!(result.exports.iter().any(|e| e.name == "PublicVar"));

        // Private items should NOT be exported
        assert!(!result.exports.iter().any(|e| e.name == "main"));
        assert!(!result.exports.iter().any(|e| e.name == "privateFunc"));
        assert!(!result.exports.iter().any(|e| e.name == "privateType"));
    }
}
