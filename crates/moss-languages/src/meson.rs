//! Meson build system support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// Meson language support.
pub struct Meson;

impl Language for Meson {
    fn name(&self) -> &'static str {
        "Meson"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["meson.build", "meson_options.txt"]
    }
    fn grammar_name(&self) -> &'static str {
        "meson"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &[] // Meson doesn't have traditional containers
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["normal_command"] // function calls
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &[]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["normal_command"] // subproject(), dependency() are function calls
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["expression_statement"]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::AllPublic
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        if node.kind() == "expression_statement" {
            if let Some(name) = self.node_name(node, content) {
                return vec![Export {
                    name: name.to_string(),
                    kind: SymbolKind::Variable,
                    line: node.start_position().row + 1,
                }];
            }
        }
        Vec::new()
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["if_command", "foreach_command"]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &["if_command", "foreach_command"]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &["if_command", "foreach_command", "if_condition"]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &["if_command", "foreach_command"]
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
        None // Meson uses function calls, not definitions
    }

    fn extract_container(&self, _node: &Node, _content: &str) -> Option<Symbol> {
        None // Meson doesn't have containers
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
        if node.kind() != "normal_command" {
            return Vec::new();
        }

        let text = &content[node.byte_range()];
        if text.starts_with("subproject(") || text.starts_with("dependency(") {
            return vec![Import {
                module: text.to_string(),
                names: Vec::new(),
                alias: None,
                is_wildcard: false,
                is_relative: false,
                line: node.start_position().row + 1,
            }];
        }

        Vec::new()
    }

    fn format_import(&self, import: &Import, _names: Option<&[&str]>) -> String {
        // Meson: subdir('path')
        format!("subdir('{}')", import.module)
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
        node.child_by_field_name("body")
    }

    fn body_has_docstring(&self, _body: &Node, _content: &str) -> bool {
        false
    }

    fn node_name<'a>(&self, node: &Node, content: &'a str) -> Option<&'a str> {
        if let Some(name_node) = node.child_by_field_name("name") {
            return Some(&content[name_node.byte_range()]);
        }
        if let Some(left_node) = node.child_by_field_name("left") {
            return Some(&content[left_node.byte_range()]);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" {
                return Some(&content[child.byte_range()]);
            }
        }
        None
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let name = path.file_name()?.to_str()?;
        if name == "meson.build" || name == "meson_options.txt" {
            Some(name.to_string())
        } else {
            None
        }
    }

    fn module_name_to_paths(&self, _module: &str) -> Vec<String> {
        vec!["meson.build".to_string()]
    }

    fn lang_key(&self) -> &'static str {
        "meson"
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
        &[]
    } // Special filenames only
    fn package_sources(&self, _: &Path) -> Vec<crate::PackageSource> {
        Vec::new()
    }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::skip_dotfiles;
        if skip_dotfiles(name) {
            return true;
        }
        !is_dir && name != "meson.build" && name != "meson_options.txt"
    }

    fn discover_packages(&self, _: &crate::PackageSource) -> Vec<(String, PathBuf)> {
        Vec::new()
    }

    fn package_module_name(&self, entry_name: &str) -> String {
        entry_name.to_string()
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
            // Control flow commands
            "else_command", "elseif_command",
            // Expression-related
            "formatunit", "identifier", "operatorunit", "ternaryoperator",
        ];
        validate_unused_kinds_audit(&Meson, documented_unused)
            .expect("Meson unused node kinds audit failed");
    }
}
