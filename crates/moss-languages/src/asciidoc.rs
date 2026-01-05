//! AsciiDoc language support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// AsciiDoc language support.
pub struct AsciiDoc;

impl Language for AsciiDoc {
    fn name(&self) -> &'static str {
        "AsciiDoc"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["adoc", "asciidoc", "asc"]
    }
    fn grammar_name(&self) -> &'static str {
        "asciidoc"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["section_block"]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &[]
    }
    fn type_kinds(&self) -> &'static [&'static str] {
        &[]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["block_macro"] // includes are block macros in AsciiDoc
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &[
            "section_block",
            "title1",
            "title2",
            "title3",
            "title4",
            "title5",
        ]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::AllPublic
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        match node.kind() {
            "section_block" | "title1" | "title2" | "title3" | "title4" | "title5" => {
                if let Some(name) = self.node_name(node, content) {
                    return vec![Export {
                        name: name.to_string(),
                        kind: SymbolKind::Module,
                        line: node.start_position().row + 1,
                    }];
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["section_block"]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &[]
    }
    fn complexity_nodes(&self) -> &'static [&'static str] {
        &[]
    }
    fn nesting_nodes(&self) -> &'static [&'static str] {
        &["section_block"]
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
        if node.kind() != "section_block" {
            return None;
        }

        let name = self.node_name(node, content)?;
        let text = &content[node.byte_range()];
        let first_line = text.lines().next().unwrap_or(text);

        Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Module,
            signature: first_line.trim().to_string(),
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

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        if node.kind() != "block_macro" {
            return Vec::new();
        }

        let text = &content[node.byte_range()];
        // Only include macros are imports
        if !text.starts_with("include::") {
            return Vec::new();
        }

        vec![Import {
            module: text.trim().to_string(),
            names: Vec::new(),
            alias: None,
            is_wildcard: false,
            is_relative: false,
            line: node.start_position().row + 1,
        }]
    }

    fn format_import(&self, _import: &Import, _names: Option<&[&str]>) -> String {
        // AsciiDoc has no imports
        String::new()
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

    fn container_body<'a>(&self, node: &'a Node<'a>) -> Option<Node<'a>> {
        node.child_by_field_name("content")
    }

    fn body_has_docstring(&self, _body: &Node, _content: &str) -> bool {
        false
    }

    fn node_name<'a>(&self, node: &Node, content: &'a str) -> Option<&'a str> {
        // For sections, the title is typically the first line
        let text = &content[node.byte_range()];
        let first_line = text.lines().next()?;
        // Strip section markers (=, ==, etc.)
        let name = first_line.trim().trim_start_matches('=').trim();
        if !name.is_empty() { Some(name) } else { None }
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        if !["adoc", "asciidoc", "asc"].contains(&ext) {
            return None;
        }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![format!("{}.adoc", module), format!("{}.asciidoc", module)]
    }

    fn lang_key(&self) -> &'static str {
        "asciidoc"
    }

    fn is_stdlib_import(&self, _: &str, _: &Path) -> bool {
        false
    }
    fn find_stdlib(&self, _project_root: &Path) -> Option<PathBuf> {
        None
    }
    fn resolve_local_import(&self, _: &str, _: &Path, _: &Path) -> Option<PathBuf> {
        None
    }
    fn resolve_external_import(&self, _: &str, _: &Path) -> Option<ResolvedPackage> {
        None
    }
    fn get_version(&self, _: &Path) -> Option<String> {
        None
    }
    fn find_package_cache(&self, _: &Path) -> Option<PathBuf> {
        None
    }
    fn indexable_extensions(&self) -> &'static [&'static str] {
        &["adoc", "asciidoc", "asc"]
    }
    fn package_sources(&self, _: &Path) -> Vec<crate::PackageSource> {
        Vec::new()
    }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{has_extension, skip_dotfiles};
        if skip_dotfiles(name) {
            return true;
        }
        !is_dir && !has_extension(name, self.indexable_extensions())
    }

    fn discover_packages(&self, _: &crate::PackageSource) -> Vec<(String, PathBuf)> {
        Vec::new()
    }

    fn package_module_name(&self, entry_name: &str) -> String {
        entry_name
            .strip_suffix(".adoc")
            .or_else(|| entry_name.strip_suffix(".asciidoc"))
            .or_else(|| entry_name.strip_suffix(".asc"))
            .unwrap_or(entry_name)
            .to_string()
    }

    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() {
            Some(path.to_path_buf())
        } else {
            None
        }
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
            // Block types - not symbols
            "literal_block", "listing_block", "open_block", "quoted_block",
            "passthrough_block", "delimited_block", "table_block", "ntable_block",
            "ident_block", "quoted_md_block",
            // Block markers and bodies
            "block_comment", "block_comment_start_marker", "block_comment_end_marker",
            "quoted_block_marker", "quoted_block_md_marker", "passthrough_block_marker",
            "open_block_marker", "table_block_marker", "ntable_block_marker",
            "literal_block_marker", "literal_block_body",
            "listing_block_start_marker", "listing_block_end_marker", "listing_block_body",
            "delimited_block_start_marker", "delimited_block_end_marker",
            // Block elements and titles
            "block_title", "block_title_marker", "block_element",
            "block_macro_name", "block_macro_attr",
            // Other content
            "body", "ident_block_line", "admonition_important",
        ];
        validate_unused_kinds_audit(&AsciiDoc, documented_unused)
            .expect("AsciiDoc unused node kinds audit failed");
    }
}
