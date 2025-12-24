//! Bash language support (parse only, minimal skeleton).

use crate::{LanguageSupport, Symbol, SymbolKind, Visibility};
use moss_core::{tree_sitter::Node, Language};

pub struct BashSupport;

impl LanguageSupport for BashSupport {
    fn language(&self) -> Language { Language::Bash }
    fn grammar_name(&self) -> &'static str { "bash" }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_definition"]
    }

    fn extract_function(&self, node: &Node, content: &str, _in_container: bool) -> Option<Symbol> {
        let name = self.node_name(node, content)?;
        Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Function,
            signature: format!("function {}", name),
            docstring: None,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
            children: Vec::new(),
        })
    }

    fn extract_container(&self, _node: &Node, _content: &str) -> Option<Symbol> {
        None
    }
}
