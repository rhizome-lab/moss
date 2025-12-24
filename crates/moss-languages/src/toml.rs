//! TOML language support.

use crate::{LanguageSupport, Symbol, SymbolKind, Visibility};
use moss_core::{tree_sitter::Node, Language};

pub struct TomlSupport;

impl LanguageSupport for TomlSupport {
    fn language(&self) -> Language { Language::Toml }
    fn grammar_name(&self) -> &'static str { "toml" }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["table", "table_array_element"]
    }

    fn extract_function(&self, _node: &Node, _content: &str, _in_container: bool) -> Option<Symbol> {
        None
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "bare_key" || child.kind() == "dotted_key" || child.kind() == "quoted_key" {
                let name = content[child.byte_range()].to_string();
                return Some(Symbol {
                    name: name.clone(),
                    kind: SymbolKind::Module,
                    signature: format!("[{}]", name),
                    docstring: None,
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    visibility: Visibility::Public,
                    children: Vec::new(),
                });
            }
        }
        None
    }
}
