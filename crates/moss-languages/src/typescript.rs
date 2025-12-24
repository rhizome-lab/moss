//! TypeScript language support.

use crate::{LanguageSupport, Symbol, SymbolKind, Visibility};
use moss_core::{tree_sitter::Node, Language};

pub struct TypeScriptSupport;

impl LanguageSupport for TypeScriptSupport {
    fn language(&self) -> Language {
        Language::TypeScript
    }

    fn grammar_name(&self) -> &'static str {
        "typescript"
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["class_declaration", "class"]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_declaration", "method_definition"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["class_declaration", "interface_declaration", "type_alias_declaration", "enum_declaration"]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["import_statement"]
    }

    fn export_kinds(&self) -> &'static [&'static str] {
        &["export_statement"]
    }

    fn extract_function(&self, node: &Node, content: &str, in_container: bool) -> Option<Symbol> {
        let name = self.node_name(node, content)?;
        let params = node
            .child_by_field_name("parameters")
            .map(|p| content[p.byte_range()].to_string())
            .unwrap_or_else(|| "()".to_string());

        Some(Symbol {
            name: name.to_string(),
            kind: if in_container { SymbolKind::Method } else { SymbolKind::Function },
            signature: format!("function {}{}", name, params),
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

    fn extract_type(&self, node: &Node, content: &str) -> Option<Symbol> {
        let name = self.node_name(node, content)?;
        let (kind, keyword) = match node.kind() {
            "interface_declaration" => (SymbolKind::Interface, "interface"),
            "type_alias_declaration" => (SymbolKind::Type, "type"),
            "enum_declaration" => (SymbolKind::Enum, "enum"),
            "class_declaration" => (SymbolKind::Class, "class"),
            _ => return None,
        };

        Some(Symbol {
            name: name.to_string(),
            kind,
            signature: format!("{} {}", keyword, name),
            docstring: None,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
            children: Vec::new(),
        })
    }
}
