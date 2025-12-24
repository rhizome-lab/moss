//! JSON language support.

use crate::{LanguageSupport, Symbol, SymbolKind, Visibility};
use moss_core::{tree_sitter::Node, Language};

pub struct JsonSupport;

impl LanguageSupport for JsonSupport {
    fn language(&self) -> Language { Language::Json }
    fn grammar_name(&self) -> &'static str { "json" }

    fn extract_function(&self, _node: &Node, _content: &str, _in_container: bool) -> Option<Symbol> {
        None
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        // Extract top-level object keys
        if node.kind() == "pair" {
            let key = node.child_by_field_name("key")?;
            let key_text = content[key.byte_range()].trim_matches('"');

            return Some(Symbol {
                name: key_text.to_string(),
                kind: SymbolKind::Variable,
                signature: key_text.to_string(),
                docstring: None,
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                visibility: Visibility::Public,
                children: Vec::new(),
            });
        }
        None
    }
}
