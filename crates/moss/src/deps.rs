//! Module dependency extraction.
//!
//! Extracts imports and exports from source files.

use crate::parsers;
use rhizome_moss_languages::{
    Export, Import, Language, SymbolKind, support_for_grammar, support_for_path,
};
use std::path::Path;
use tree_sitter;

/// A re-export statement (export * from './module' or export { foo } from './module')
#[derive(Debug, Clone)]
pub struct ReExport {
    pub module: String,
    pub names: Vec<String>, // Empty for "export * from", specific names for "export { x } from"
    pub is_star: bool,      // true for "export * from"
    #[allow(dead_code)] // Consistent with Import/Export, useful for diagnostics
    pub line: usize,
}

/// Extracted dependencies (without file context)
struct ExtractedDeps {
    imports: Vec<Import>,
    exports: Vec<Export>,
    reexports: Vec<ReExport>,
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
                if exp.kind != SymbolKind::Variable {
                    lines.push(format!("{}: {}", exp.kind.as_str(), exp.name));
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

pub struct DepsExtractor {}

impl DepsExtractor {
    pub fn new() -> Self {
        Self {}
    }

    pub fn extract(&self, path: &Path, content: &str) -> DepsResult {
        let support = support_for_path(path);

        let extracted = match support.map(|s| s.grammar_name()) {
            // JS/TS need special handling for re-exports
            Some("javascript") => self.extract_javascript(content),
            Some("typescript") => self.extract_typescript(content),
            Some("tsx") => self.extract_tsx(content),
            // All other languages use trait-based extraction
            Some(_) => {
                let support = support.unwrap();
                self.extract_with_trait(content, support)
            }
            None => ExtractedDeps {
                imports: Vec::new(),
                exports: Vec::new(),
                reexports: Vec::new(),
            },
        };

        DepsResult {
            imports: extracted.imports,
            exports: extracted.exports,
            reexports: extracted.reexports,
            file_path: path.to_string_lossy().to_string(),
        }
    }

    /// Extract using the Language trait
    fn extract_with_trait(&self, content: &str, support: &dyn Language) -> ExtractedDeps {
        let tree = match parsers::parse_with_grammar(support.grammar_name(), content) {
            Some(t) => t,
            None => {
                return ExtractedDeps {
                    imports: Vec::new(),
                    exports: Vec::new(),
                    reexports: Vec::new(),
                };
            }
        };

        let mut imports = Vec::new();
        let mut exports = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        self.collect_with_trait(&mut cursor, content, support, &mut imports, &mut exports);
        ExtractedDeps {
            imports,
            exports,
            reexports: Vec::new(),
        }
    }

    fn collect_with_trait(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        support: &dyn Language,
        imports: &mut Vec<Import>,
        exports: &mut Vec<Export>,
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
                let mut sub_imports = Vec::new();
                let mut sub_exports = Vec::new();
                let sub_root = sub_tree.root_node();
                let mut sub_cursor = sub_root.walk();
                self.collect_with_trait(
                    &mut sub_cursor,
                    &embedded.content,
                    sub_lang,
                    &mut sub_imports,
                    &mut sub_exports,
                );

                // Adjust line numbers for embedded content offset
                for mut imp in sub_imports {
                    imp.line += embedded.start_line - 1;
                    imports.push(imp);
                }
                for mut exp in sub_exports {
                    exp.line += embedded.start_line - 1;
                    exports.push(exp);
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
                imports.extend(lang_imports);
            }

            // Check for public symbol nodes
            if support.public_symbol_kinds().contains(&kind) {
                let lang_exports = support.extract_public_symbols(&node, content);
                exports.extend(lang_exports);
            }

            // Recurse into children
            if cursor.goto_first_child() {
                self.collect_with_trait(cursor, content, support, imports, exports);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn extract_typescript(&self, content: &str) -> ExtractedDeps {
        let tree = match parsers::parse_with_grammar("typescript", content) {
            Some(t) => t,
            None => {
                return ExtractedDeps {
                    imports: Vec::new(),
                    exports: Vec::new(),
                    reexports: Vec::new(),
                };
            }
        };
        self.extract_js_ts_deps(&tree, content)
    }

    fn extract_tsx(&self, content: &str) -> ExtractedDeps {
        let tree = match parsers::parse_with_grammar("tsx", content) {
            Some(t) => t,
            None => {
                return ExtractedDeps {
                    imports: Vec::new(),
                    exports: Vec::new(),
                    reexports: Vec::new(),
                };
            }
        };
        self.extract_js_ts_deps(&tree, content)
    }

    fn extract_javascript(&self, content: &str) -> ExtractedDeps {
        let tree = match parsers::parse_with_grammar("javascript", content) {
            Some(t) => t,
            None => {
                return ExtractedDeps {
                    imports: Vec::new(),
                    exports: Vec::new(),
                    reexports: Vec::new(),
                };
            }
        };
        self.extract_js_ts_deps(&tree, content)
    }

    /// Shared extraction for JavaScript/TypeScript AST
    fn extract_js_ts_deps(&self, tree: &tree_sitter::Tree, content: &str) -> ExtractedDeps {
        let mut imports = Vec::new();
        let mut exports = Vec::new();
        let mut reexports = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        self.collect_js_ts_deps(
            &mut cursor,
            content,
            &mut imports,
            &mut exports,
            &mut reexports,
        );
        ExtractedDeps {
            imports,
            exports,
            reexports,
        }
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
                                    module =
                                        text.trim_matches(|c| c == '"' || c == '\'').to_string();
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
                            is_wildcard: false,
                            is_relative,
                            line: node.start_position().row + 1,
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
                                    source_module = Some(
                                        text.trim_matches(|c| c == '"' || c == '\'').to_string(),
                                    );
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
                                    self.collect_export_clause_names(
                                        child,
                                        content,
                                        &mut named_exports,
                                    );
                                }
                                "function_declaration" | "generator_function_declaration" => {
                                    if let Some(name_node) = child.child_by_field_name("name") {
                                        exports.push(Export {
                                            name: content[name_node.byte_range()].to_string(),
                                            kind: SymbolKind::Function,
                                            line: node.start_position().row + 1,
                                        });
                                    }
                                }
                                "class_declaration" => {
                                    if let Some(name_node) = child.child_by_field_name("name") {
                                        exports.push(Export {
                                            name: content[name_node.byte_range()].to_string(),
                                            kind: SymbolKind::Class,
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
                                kind: SymbolKind::Function,
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
                                kind: SymbolKind::Class,
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

    fn collect_import_names(
        &self,
        node: tree_sitter::Node,
        content: &str,
        names: &mut Vec<String>,
    ) {
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

    fn collect_variable_names(
        &self,
        node: tree_sitter::Node,
        content: &str,
        exports: &mut Vec<Export>,
        line: usize,
    ) {
        let mut cursor = node.walk();
        loop {
            let child = cursor.node();
            if child.kind() == "variable_declarator" {
                if let Some(name_node) = child.child_by_field_name("name") {
                    if name_node.kind() == "identifier" {
                        exports.push(Export {
                            name: content[name_node.byte_range()].to_string(),
                            kind: SymbolKind::Variable,
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
        let extractor = DepsExtractor::new();
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
        let extractor = DepsExtractor::new();
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
        let extractor = DepsExtractor::new();
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
        let extractor = DepsExtractor::new();
        let content = r#"
export * from './utils';
export * as helpers from './helpers';
export { foo, bar } from './specific';
"#;
        let result = extractor.extract(&PathBuf::from("index.ts"), content);

        assert_eq!(result.reexports.len(), 3);

        // Star re-export
        let star = result
            .reexports
            .iter()
            .find(|r| r.module == "./utils")
            .unwrap();
        assert!(star.is_star);
        assert!(star.names.is_empty());

        // Namespace re-export (export * as helpers)
        let namespace = result
            .reexports
            .iter()
            .find(|r| r.module == "./helpers")
            .unwrap();
        assert!(namespace.is_star);

        // Named re-export
        let named = result
            .reexports
            .iter()
            .find(|r| r.module == "./specific")
            .unwrap();
        assert!(!named.is_star);
        assert!(named.names.contains(&"foo".to_string()));
        assert!(named.names.contains(&"bar".to_string()));
    }

    #[test]
    fn test_go_imports() {
        let extractor = DepsExtractor::new();
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
        assert!(
            result
                .imports
                .iter()
                .any(|i| i.module == "github.com/user/pkg" && i.alias == Some("alias".to_string()))
        );

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

    #[test]
    fn test_vue_embedded_imports() {
        let extractor = DepsExtractor::new();
        let content = r#"
<template>
  <div>{{ message }}</div>
</template>

<script lang="ts">
import { ref, computed } from 'vue';
import { useStore } from './store';

export function greet(name: string): string {
  return `Hello, ${name}`;
}

const message = ref('Hello World');
</script>
"#;
        let result = extractor.extract(&PathBuf::from("App.vue"), content);

        // Check imports from embedded script
        assert!(
            !result.imports.is_empty(),
            "Should extract imports from Vue script: {:?}",
            result.imports
        );
        assert!(
            result.imports.iter().any(|i| i.module == "vue"),
            "Should have vue import"
        );
        assert!(
            result
                .imports
                .iter()
                .any(|i| i.module == "./store" && i.is_relative),
            "Should have relative store import"
        );

        // Verify line numbers are correctly offset
        let vue_import = result.imports.iter().find(|i| i.module == "vue").unwrap();
        assert!(
            vue_import.line >= 7,
            "Vue import should be on line 7 or later (was {})",
            vue_import.line
        );
    }

    #[test]
    fn test_html_embedded_imports() {
        let extractor = DepsExtractor::new();
        let content = r#"
<!DOCTYPE html>
<html>
<body>
  <script type="module">
    import { init } from './app.js';

    function main() {
      init();
    }
  </script>
</body>
</html>
"#;
        let result = extractor.extract(&PathBuf::from("index.html"), content);

        // Check imports from embedded script
        assert!(
            !result.imports.is_empty(),
            "Should extract imports from HTML script"
        );
        assert!(
            result.imports.iter().any(|i| i.module == "./app.js"),
            "Should have app.js import"
        );
    }
}
