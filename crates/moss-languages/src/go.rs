//! Go language support.

use crate::{Export, Import, LanguageSupport, Symbol, SymbolKind, Visibility};
use moss_core::{tree_sitter::Node, Language};

pub struct GoSupport;

impl LanguageSupport for GoSupport {
    fn language(&self) -> Language { Language::Go }
    fn grammar_name(&self) -> &'static str { "go" }

    fn container_kinds(&self) -> &'static [&'static str] {
        &[] // Go types don't have children in the tree-sitter sense
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_declaration", "method_declaration"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["type_spec"] // The actual type is in type_spec, not type_declaration
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["import_declaration"]
    }

    fn export_kinds(&self) -> &'static [&'static str] {
        &["function_declaration", "method_declaration", "type_spec"]
    }

    fn extract_function(&self, node: &Node, content: &str, in_container: bool) -> Option<Symbol> {
        let name = self.node_name(node, content)?;
        let params = node.child_by_field_name("parameters")
            .map(|p| content[p.byte_range()].to_string())
            .unwrap_or_else(|| "()".to_string());

        Some(Symbol {
            name: name.to_string(),
            kind: if in_container { SymbolKind::Method } else { SymbolKind::Function },
            signature: format!("func {}{}", name, params),
            docstring: None,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                Visibility::Public
            } else {
                Visibility::Private
            },
            children: Vec::new(),
        })
    }

    fn extract_container(&self, _node: &Node, _content: &str) -> Option<Symbol> {
        None // Go types are extracted via extract_type
    }

    fn extract_type(&self, node: &Node, content: &str) -> Option<Symbol> {
        // Go type_spec: name field + type field (struct_type, interface_type, etc.)
        let name_node = node.child_by_field_name("name")?;
        let name = content[name_node.byte_range()].to_string();

        let type_node = node.child_by_field_name("type");
        let type_kind = type_node.map(|t| t.kind()).unwrap_or("");

        let kind = match type_kind {
            "struct_type" => SymbolKind::Struct,
            "interface_type" => SymbolKind::Interface,
            _ => SymbolKind::Type,
        };

        Some(Symbol {
            name: name.clone(),
            kind,
            signature: format!("type {}", name),
            docstring: None,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                Visibility::Public
            } else {
                Visibility::Private
            },
            children: Vec::new(),
        })
    }

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        if node.kind() != "import_declaration" {
            return Vec::new();
        }

        let mut imports = Vec::new();
        let line = node.start_position().row + 1;

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "import_spec" => {
                    // import "path" or import alias "path"
                    if let Some(imp) = Self::parse_import_spec(&child, content, line) {
                        imports.push(imp);
                    }
                }
                "import_spec_list" => {
                    // Grouped imports
                    let mut list_cursor = child.walk();
                    for spec in child.children(&mut list_cursor) {
                        if spec.kind() == "import_spec" {
                            if let Some(imp) = Self::parse_import_spec(&spec, content, line) {
                                imports.push(imp);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        imports
    }

    fn extract_exports(&self, node: &Node, content: &str) -> Vec<Export> {
        // Go exports are determined by uppercase first letter
        let name = match self.node_name(node, content) {
            Some(n) if n.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) => n,
            _ => return Vec::new(),
        };

        let line = node.start_position().row + 1;
        let kind = match node.kind() {
            "function_declaration" => SymbolKind::Function,
            "method_declaration" => SymbolKind::Method,
            "type_spec" => SymbolKind::Type,
            _ => return Vec::new(),
        };

        vec![Export {
            name: name.to_string(),
            kind,
            line,
        }]
    }

    fn is_public(&self, node: &Node, content: &str) -> bool {
        self.node_name(node, content)
            .and_then(|n| n.chars().next())
            .map(|c| c.is_uppercase())
            .unwrap_or(false)
    }
}

impl GoSupport {
    fn parse_import_spec(node: &Node, content: &str, line: usize) -> Option<Import> {
        let mut path = String::new();
        let mut alias = None;

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "interpreted_string_literal" => {
                    let text = &content[child.byte_range()];
                    path = text.trim_matches('"').to_string();
                }
                "package_identifier" | "blank_identifier" | "dot" => {
                    alias = Some(content[child.byte_range()].to_string());
                }
                _ => {}
            }
        }

        if path.is_empty() {
            return None;
        }

        let is_wildcard = alias.as_deref() == Some(".");
        Some(Import {
            module: path,
            names: Vec::new(),
            alias,
            is_wildcard,
            is_relative: false, // Go doesn't have relative imports in the traditional sense
            line,
        })
    }
}
