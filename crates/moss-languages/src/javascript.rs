//! JavaScript language support.

use crate::{Export, Import, LanguageSupport, Symbol, SymbolKind, Visibility};
use moss_core::{tree_sitter::Node, Language};

pub struct JavaScriptSupport;

impl LanguageSupport for JavaScriptSupport {
    fn language(&self) -> Language {
        Language::JavaScript
    }

    fn grammar_name(&self) -> &'static str {
        "javascript"
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["class_declaration", "class"]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_declaration", "method_definition", "generator_function_declaration"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["class_declaration"]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["import_statement"]
    }

    fn export_kinds(&self) -> &'static [&'static str] {
        &["export_statement"]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &[
            "if_statement", "for_statement", "for_in_statement", "while_statement",
            "do_statement", "switch_case", "catch_clause", "ternary_expression",
            "binary_expression", // for && and ||
        ]
    }

    fn extract_function(&self, node: &Node, content: &str, in_container: bool) -> Option<Symbol> {
        let name = self.node_name(node, content)?;
        let params = node
            .child_by_field_name("parameters")
            .map(|p| content[p.byte_range()].to_string())
            .unwrap_or_else(|| "()".to_string());

        let signature = if node.kind() == "method_definition" {
            format!("{}{}", name, params)
        } else {
            format!("function {}{}", name, params)
        };

        Some(Symbol {
            name: name.to_string(),
            kind: if in_container { SymbolKind::Method } else { SymbolKind::Function },
            signature,
            docstring: None,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
            children: Vec::new(),
        })
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        let name = self.node_name(node, content)?;

        Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Class,
            signature: format!("class {}", name),
            docstring: None,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
            children: Vec::new(),
        })
    }

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        if node.kind() != "import_statement" {
            return Vec::new();
        }

        let line = node.start_position().row + 1;
        let mut module = String::new();
        let mut names = Vec::new();

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "string" | "string_fragment" => {
                    let text = &content[child.byte_range()];
                    module = text.trim_matches(|c| c == '"' || c == '\'').to_string();
                }
                "import_clause" => {
                    Self::collect_import_names(&child, content, &mut names);
                }
                _ => {}
            }
        }

        if module.is_empty() {
            return Vec::new();
        }

        vec![Import {
            module: module.clone(),
            names,
            alias: None,
            is_wildcard: false,
            is_relative: module.starts_with('.'),
            line,
        }]
    }

    fn extract_exports(&self, node: &Node, content: &str) -> Vec<Export> {
        if node.kind() != "export_statement" {
            return Vec::new();
        }

        let line = node.start_position().row + 1;
        let mut exports = Vec::new();

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "function_declaration" | "generator_function_declaration" => {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        exports.push(Export {
                            name: content[name_node.byte_range()].to_string(),
                            kind: SymbolKind::Function,
                            line,
                        });
                    }
                }
                "class_declaration" => {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        exports.push(Export {
                            name: content[name_node.byte_range()].to_string(),
                            kind: SymbolKind::Class,
                            line,
                        });
                    }
                }
                "lexical_declaration" => {
                    // export const foo = ...
                    let mut decl_cursor = child.walk();
                    for decl_child in child.children(&mut decl_cursor) {
                        if decl_child.kind() == "variable_declarator" {
                            if let Some(name_node) = decl_child.child_by_field_name("name") {
                                exports.push(Export {
                                    name: content[name_node.byte_range()].to_string(),
                                    kind: SymbolKind::Variable,
                                    line,
                                });
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        exports
    }
}

impl JavaScriptSupport {
    fn collect_import_names(import_clause: &Node, content: &str, names: &mut Vec<String>) {
        let mut cursor = import_clause.walk();
        for child in import_clause.children(&mut cursor) {
            match child.kind() {
                "identifier" => {
                    // Default import: import foo from './module'
                    names.push(content[child.byte_range()].to_string());
                }
                "named_imports" => {
                    // { foo, bar }
                    let mut inner_cursor = child.walk();
                    for inner in child.children(&mut inner_cursor) {
                        if inner.kind() == "import_specifier" {
                            if let Some(name_node) = inner.child_by_field_name("name") {
                                names.push(content[name_node.byte_range()].to_string());
                            }
                        }
                    }
                }
                "namespace_import" => {
                    // import * as foo
                    if let Some(name_node) = child.child_by_field_name("name") {
                        names.push(format!("* as {}", &content[name_node.byte_range()]));
                    }
                }
                _ => {}
            }
        }
    }
}
