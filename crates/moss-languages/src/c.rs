//! C language support.

use crate::{LanguageSupport, Symbol, SymbolKind, Visibility};
use moss_core::{tree_sitter::Node, Language};

pub struct CSupport;

impl LanguageSupport for CSupport {
    fn language(&self) -> Language { Language::C }
    fn grammar_name(&self) -> &'static str { "c" }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_definition"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["struct_specifier", "enum_specifier", "type_definition"]
    }

    fn extract_function(&self, node: &Node, content: &str, _in_container: bool) -> Option<Symbol> {
        let declarator = node.child_by_field_name("declarator")?;
        let name = self.find_identifier(&declarator, content)?;

        Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Function,
            signature: format!("{}", name),
            docstring: None,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
            children: Vec::new(),
        })
    }

    fn extract_container(&self, _node: &Node, _content: &str) -> Option<Symbol> {
        None // C doesn't have containers in the same sense
    }

    fn extract_type(&self, node: &Node, content: &str) -> Option<Symbol> {
        let name = self.node_name(node, content)?;
        let kind = match node.kind() {
            "struct_specifier" => SymbolKind::Struct,
            "enum_specifier" => SymbolKind::Enum,
            _ => SymbolKind::Type,
        };

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

impl CSupport {
    fn find_identifier<'a>(&self, node: &Node, content: &'a str) -> Option<&'a str> {
        if node.kind() == "identifier" {
            return Some(&content[node.byte_range()]);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(id) = self.find_identifier(&child, content) {
                return Some(id);
            }
        }
        None
    }
}
