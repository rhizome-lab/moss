//! Go language support.

use crate::{LanguageSupport, Symbol, SymbolKind, Visibility};
use moss_core::{tree_sitter::Node, Language};

pub struct GoSupport;

impl LanguageSupport for GoSupport {
    fn language(&self) -> Language { Language::Go }
    fn grammar_name(&self) -> &'static str { "go" }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["type_declaration"] // for struct/interface with methods
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_declaration", "method_declaration"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["type_declaration"]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["import_declaration"]
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

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        // Go type declarations
        let name = self.node_name(node, content)?;
        Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Type,
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

    fn is_public(&self, node: &Node, content: &str) -> bool {
        self.node_name(node, content)
            .and_then(|n| n.chars().next())
            .map(|c| c.is_uppercase())
            .unwrap_or(false)
    }
}
