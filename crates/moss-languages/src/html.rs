//! HTML language support (parse only, minimal skeleton).

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// HTML language support.
pub struct Html;

impl Language for Html {
    fn name(&self) -> &'static str {
        "HTML"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["html", "htm"]
    }
    fn grammar_name(&self) -> &'static str {
        "html"
    }

    fn has_symbols(&self) -> bool {
        false
    }

    // HTML has no functions/containers/types in the traditional sense
    fn container_kinds(&self) -> &'static [&'static str] {
        &[]
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

    fn extract_container(&self, _node: &Node, _content: &str) -> Option<Symbol> {
        None
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
        // HTML has no imports
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

    fn embedded_content(&self, node: &Node, content: &str) -> Option<crate::EmbeddedBlock> {
        match node.kind() {
            "script_element" => {
                let raw = find_raw_text_child(node)?;
                let grammar = detect_script_type(node, content);
                Some(crate::EmbeddedBlock {
                    grammar,
                    content: content[raw.byte_range()].to_string(),
                    start_line: raw.start_position().row + 1,
                })
            }
            "style_element" => {
                let raw = find_raw_text_child(node)?;
                Some(crate::EmbeddedBlock {
                    grammar: "css",
                    content: content[raw.byte_range()].to_string(),
                    start_line: raw.start_position().row + 1,
                })
            }
            _ => None,
        }
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

/// Find the raw_text child of a script/style element.
fn find_raw_text_child<'a>(node: &'a Node<'a>) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "raw_text" {
            return Some(child);
        }
    }
    None
}

/// Detect script type from the type attribute (e.g., <script type="module">).
/// HTML scripts default to JavaScript; type="module" is still JavaScript.
fn detect_script_type(node: &Node, content: &str) -> &'static str {
    if let Some(script_type) = get_type_attribute(node, content) {
        match script_type {
            "text/typescript" => return "typescript",
            "module" | "text/javascript" | "application/javascript" => return "javascript",
            _ => {}
        }
    }
    "javascript"
}

/// Get the type attribute value from a script element.
fn get_type_attribute<'a>(node: &Node, content: &'a str) -> Option<&'a str> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        // Look for start_tag which contains the attributes
        if child.kind() == "start_tag" {
            let mut inner_cursor = child.walk();
            for attr in child.children(&mut inner_cursor) {
                if attr.kind() == "attribute" {
                    // Check if this is a type attribute
                    let mut attr_cursor = attr.walk();
                    let mut is_type = false;
                    for part in attr.children(&mut attr_cursor) {
                        if part.kind() == "attribute_name" {
                            let name = &content[part.byte_range()];
                            is_type = name == "type";
                        } else if is_type && part.kind() == "quoted_attribute_value" {
                            // Get the value inside quotes
                            let value = &content[part.byte_range()];
                            return Some(value.trim_matches('"').trim_matches('\''));
                        }
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate_unused_kinds_audit;

    #[test]
    fn unused_node_kinds_audit() {
        #[rustfmt::skip]
        let documented_unused: &[&str] = &[
            "doctype",
        ];

        validate_unused_kinds_audit(&Html, documented_unused)
            .expect("HTML unused node kinds audit failed");
    }
}
