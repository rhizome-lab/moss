//! JavaScript language support.

use crate::{LanguageSupport, Symbol, SymbolKind, Visibility};
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
}
