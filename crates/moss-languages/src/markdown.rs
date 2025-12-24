//! Markdown language support.

use crate::{LanguageSupport, Symbol, SymbolKind, Visibility};
use moss_core::{tree_sitter::Node, Language};

pub struct MarkdownSupport;

impl LanguageSupport for MarkdownSupport {
    fn language(&self) -> Language { Language::Markdown }
    fn grammar_name(&self) -> &'static str { "markdown" }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["atx_heading", "setext_heading"]
    }

    fn extract_function(&self, _node: &Node, _content: &str, _in_container: bool) -> Option<Symbol> {
        None
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        // Extract heading text
        let mut cursor = node.walk();
        let text = node.children(&mut cursor)
            .find(|c| c.kind() == "heading_content" || c.kind() == "inline")
            .map(|c| content[c.byte_range()].trim().to_string())
            .unwrap_or_default();

        if text.is_empty() {
            return None;
        }

        // Determine heading level
        let level = node.children(&mut cursor)
            .find(|c| c.kind().starts_with("atx_h"))
            .map(|c| c.kind().chars().last().and_then(|c| c.to_digit(10)).unwrap_or(1) as usize)
            .unwrap_or(1);

        Some(Symbol {
            name: text.clone(),
            kind: SymbolKind::Heading,
            signature: format!("{} {}", "#".repeat(level), text),
            docstring: None,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
            children: Vec::new(),
        })
    }
}
