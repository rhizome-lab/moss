//! Markdown language support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// Markdown language support.
pub struct Markdown;

impl Language for Markdown {
    fn name(&self) -> &'static str {
        "Markdown"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["md", "markdown"]
    }
    fn grammar_name(&self) -> &'static str {
        "markdown"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    // Markdown is documentation, not code - no functions/types/control flow
    fn container_kinds(&self) -> &'static [&'static str] {
        &["atx_heading", "setext_heading"]
    }
    fn function_kinds(&self) -> &'static [&'static str] {
        &[]
    }
    fn type_kinds(&self) -> &'static [&'static str] {
        &[]
    }
    fn import_kinds(&self) -> &'static [&'static str] {
        &[]
    }
    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &[]
    }
    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::NotApplicable
    }
    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &[]
    }
    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &[]
    }
    fn complexity_nodes(&self) -> &'static [&'static str] {
        &[]
    }
    fn nesting_nodes(&self) -> &'static [&'static str] {
        &[]
    }

    fn signature_suffix(&self) -> &'static str {
        ""
    }

    fn extract_function(
        &self,
        _node: &Node,
        _content: &str,
        _in_container: bool,
    ) -> Option<Symbol> {
        None
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        // Extract heading text
        let mut cursor = node.walk();
        let text = node
            .children(&mut cursor)
            .find(|c| c.kind() == "heading_content" || c.kind() == "inline")
            .map(|c| content[c.byte_range()].trim().to_string())
            .unwrap_or_default();

        if text.is_empty() {
            return None;
        }

        // Determine heading level from marker node
        let mut cursor2 = node.walk();
        let level = node
            .children(&mut cursor2)
            .find_map(|c| match c.kind() {
                "atx_h1_marker" => Some(1),
                "atx_h2_marker" => Some(2),
                "atx_h3_marker" => Some(3),
                "atx_h4_marker" => Some(4),
                "atx_h5_marker" => Some(5),
                "atx_h6_marker" => Some(6),
                _ => None,
            })
            .unwrap_or(1);

        Some(Symbol {
            name: text.clone(),
            kind: SymbolKind::Heading,
            signature: format!("{} {}", "#".repeat(level), text),
            docstring: None,
            attributes: Vec::new(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
            children: Vec::new(),
            is_interface_impl: false,
            implements: Vec::new(),
        })
    }

    fn extract_type(&self, _node: &Node, _content: &str) -> Option<Symbol> {
        None
    }
    fn extract_docstring(&self, _node: &Node, _content: &str) -> Option<String> {
        None
    }

    fn extract_attributes(&self, _node: &Node, _content: &str) -> Vec<String> {
        Vec::new()
    }
    fn extract_imports(&self, _node: &Node, _content: &str) -> Vec<Import> {
        Vec::new()
    }

    fn format_import(&self, _import: &Import, _names: Option<&[&str]>) -> String {
        // Markdown has no imports
        String::new()
    }
    fn extract_public_symbols(&self, _node: &Node, _content: &str) -> Vec<Export> {
        Vec::new()
    }

    fn is_public(&self, _node: &Node, _content: &str) -> bool {
        true
    }
    fn get_visibility(&self, _node: &Node, _content: &str) -> Visibility {
        Visibility::Public
    }

    fn is_test_symbol(&self, _symbol: &crate::Symbol) -> bool {
        false
    }

    fn embedded_content(&self, _node: &Node, _content: &str) -> Option<crate::EmbeddedBlock> {
        None
    }

    fn container_body<'a>(&self, _node: &'a Node<'a>) -> Option<Node<'a>> {
        None
    }
    fn body_has_docstring(&self, _body: &Node, _content: &str) -> bool {
        false
    }
    fn node_name<'a>(&self, _node: &Node, _content: &'a str) -> Option<&'a str> {
        None
    }

    fn file_path_to_module_name(&self, _: &Path) -> Option<String> {
        None
    }
    fn module_name_to_paths(&self, _: &str) -> Vec<String> {
        Vec::new()
    }

    fn lang_key(&self) -> &'static str {
        ""
    }
    fn resolve_local_import(&self, _: &str, _: &Path, _: &Path) -> Option<PathBuf> {
        None
    }
    fn resolve_external_import(&self, _: &str, _: &Path) -> Option<ResolvedPackage> {
        None
    }
    fn is_stdlib_import(&self, _: &str, _: &Path) -> bool {
        false
    }
    fn get_version(&self, _: &Path) -> Option<String> {
        None
    }
    fn find_package_cache(&self, _: &Path) -> Option<PathBuf> {
        None
    }
    fn indexable_extensions(&self) -> &'static [&'static str] {
        &[]
    }
    fn find_stdlib(&self, _: &Path) -> Option<PathBuf> {
        None
    }
    fn package_module_name(&self, name: &str) -> String {
        name.to_string()
    }
    fn package_sources(&self, _: &Path) -> Vec<crate::PackageSource> {
        Vec::new()
    }
    fn discover_packages(&self, _: &crate::PackageSource) -> Vec<(String, PathBuf)> {
        Vec::new()
    }
    fn find_package_entry(&self, _: &Path) -> Option<PathBuf> {
        None
    }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{has_extension, skip_dotfiles};
        if skip_dotfiles(name) {
            return true;
        }
        !is_dir && !has_extension(name, self.indexable_extensions())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate_unused_kinds_audit;

    #[test]
    fn unused_node_kinds_audit() {
        #[rustfmt::skip]
        let documented_unused: &[&str] = &[
            "block_continuation", "block_quote", "block_quote_marker",
            "fenced_code_block", "fenced_code_block_delimiter",
            "html_block", "indented_code_block", "link_reference_definition",
        ];

        validate_unused_kinds_audit(&Markdown, documented_unused)
            .expect("Markdown unused node kinds audit failed");
    }
}
