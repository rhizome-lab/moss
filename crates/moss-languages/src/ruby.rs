//! Ruby language support.

use crate::{LanguageSupport, Symbol, SymbolKind, Visibility};
use moss_core::{tree_sitter::Node, Language};

pub struct RubySupport;

impl LanguageSupport for RubySupport {
    fn language(&self) -> Language { Language::Ruby }
    fn grammar_name(&self) -> &'static str { "ruby" }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["class", "module"]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["method", "singleton_method"]
    }

    fn extract_function(&self, node: &Node, content: &str, _in_container: bool) -> Option<Symbol> {
        let name = self.node_name(node, content)?;
        Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Method,
            signature: format!("def {}", name),
            docstring: None,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
            children: Vec::new(),
        })
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        let name = self.node_name(node, content)?;
        let kind = if node.kind() == "module" { SymbolKind::Module } else { SymbolKind::Class };

        Some(Symbol {
            name: name.to_string(),
            kind,
            signature: format!("{} {}", kind.as_str(), name),
            docstring: None,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
            children: Vec::new(),
        })
    }
}
