//! Windows Batch file support.

use std::path::{Path, PathBuf};
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use crate::external_packages::ResolvedPackage;
use moss_core::tree_sitter::Node;

/// Batch language support.
pub struct Batch;

impl Language for Batch {
    fn name(&self) -> &'static str { "Batch" }
    fn extensions(&self) -> &'static [&'static str] { &["bat", "cmd"] }
    fn grammar_name(&self) -> &'static str { "batch" }

    fn has_symbols(&self) -> bool { true }

    fn container_kinds(&self) -> &'static [&'static str] {
        &[]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["label"]
    }

    fn type_kinds(&self) -> &'static [&'static str] { &[] }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["call_command"] // call to other batch files
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["label", "variable_assignment"]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::AllPublic
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        match node.kind() {
            "label" => {
                if let Some(name) = self.node_name(node, content) {
                    return vec![Export {
                        name: name.to_string(),
                        kind: SymbolKind::Function,
                        line: node.start_position().row + 1,
                    }];
                }
            }
            "variable_assignment" => {
                if let Some(name) = self.node_name(node, content) {
                    return vec![Export {
                        name: name.to_string(),
                        kind: SymbolKind::Variable,
                        line: node.start_position().row + 1,
                    }];
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["label"]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &["if_statement", "for_statement", "goto_statement"]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &["if_statement", "for_statement"]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &["if_statement", "for_statement"]
    }

    fn extract_function(&self, node: &Node, content: &str, _in_container: bool) -> Option<Symbol> {
        if node.kind() != "label" {
            return None;
        }

        let name = self.node_name(node, content)?;
        let text = &content[node.byte_range()];

        Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Function,
            signature: text.trim().to_string(),
            docstring: None,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
            children: Vec::new(),
        })
    }

    fn extract_container(&self, _node: &Node, _content: &str) -> Option<Symbol> { None }
    fn extract_type(&self, _node: &Node, _content: &str) -> Option<Symbol> { None }
    fn extract_docstring(&self, _node: &Node, _content: &str) -> Option<String> { None }

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        if node.kind() != "call_command" {
            return Vec::new();
        }

        let text = &content[node.byte_range()];
        vec![Import {
            module: text.trim().to_string(),
            names: Vec::new(),
            alias: None,
            is_wildcard: false,
            is_relative: false,
            line: node.start_position().row + 1,
        }]
    }

    fn is_public(&self, _node: &Node, _content: &str) -> bool { true }
    fn get_visibility(&self, _node: &Node, _content: &str) -> Visibility { Visibility::Public }

    fn embedded_content(&self, _node: &Node, _content: &str) -> Option<crate::EmbeddedBlock> { None }

    fn container_body<'a>(&self, _node: &'a Node<'a>) -> Option<Node<'a>> { None }
    fn body_has_docstring(&self, _body: &Node, _content: &str) -> bool { false }

    fn node_name<'a>(&self, node: &Node, content: &'a str) -> Option<&'a str> {
        match node.kind() {
            "label" => {
                let text = &content[node.byte_range()];
                // Labels start with : and the name follows
                if text.starts_with(':') {
                    Some(text[1..].trim())
                } else {
                    Some(text.trim())
                }
            }
            "variable_assignment" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    return Some(&content[name_node.byte_range()]);
                }
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "variable" {
                        return Some(&content[child.byte_range()]);
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        if !["bat", "cmd"].contains(&ext) { return None; }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![
            format!("{}.bat", module),
            format!("{}.cmd", module),
        ]
    }

    fn lang_key(&self) -> &'static str { "batch" }

    fn is_stdlib_import(&self, _: &str, _: &Path) -> bool { false }
    fn find_stdlib(&self, _project_root: &Path) -> Option<PathBuf> { None }
    fn resolve_local_import(&self, _: &str, _: &Path, _: &Path) -> Option<PathBuf> { None }
    fn resolve_external_import(&self, _: &str, _: &Path) -> Option<ResolvedPackage> { None }
    fn get_version(&self, _: &Path) -> Option<String> { None }
    fn find_package_cache(&self, _: &Path) -> Option<PathBuf> { None }
    fn indexable_extensions(&self) -> &'static [&'static str] { &["bat", "cmd"] }
    fn package_sources(&self, _: &Path) -> Vec<crate::PackageSource> { Vec::new() }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{skip_dotfiles, has_extension};
        if skip_dotfiles(name) { return true; }
        !is_dir && !has_extension(name, &["bat", "cmd"])
    }

    fn discover_packages(&self, _: &crate::PackageSource) -> Vec<(String, PathBuf)> { Vec::new() }

    fn package_module_name(&self, entry_name: &str) -> String {
        entry_name.strip_suffix(".bat")
            .or_else(|| entry_name.strip_suffix(".cmd"))
            .unwrap_or(entry_name)
            .to_string()
    }

    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() { Some(path.to_path_buf()) } else { None }
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
            "function_definition", "identifier", "variable_declaration",
        ];
        validate_unused_kinds_audit(&Batch, documented_unused)
            .expect("Batch unused node kinds audit failed");
    }
}
