//! Nix language support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// Nix language support.
pub struct Nix;

impl Language for Nix {
    fn name(&self) -> &'static str {
        "Nix"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["nix"]
    }
    fn grammar_name(&self) -> &'static str {
        "nix"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &[
            "attrset_expression",
            "let_expression",
            "rec_attrset_expression",
        ]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_expression"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &[]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["apply_expression"] // import ./path
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["binding"]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::NotApplicable
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        if node.kind() != "binding" {
            return Vec::new();
        }

        let name = match self.node_name(node, content) {
            Some(n) => n.to_string(),
            None => return Vec::new(),
        };

        vec![Export {
            name,
            kind: SymbolKind::Variable,
            line: node.start_position().row + 1,
        }]
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["let_expression", "with_expression", "function_expression"]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &["if_expression"]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &["if_expression"]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &[
            "attrset_expression",
            "let_expression",
            "function_expression",
        ]
    }

    fn signature_suffix(&self) -> &'static str {
        ""
    }

    fn extract_function(&self, node: &Node, content: &str, _in_container: bool) -> Option<Symbol> {
        if node.kind() != "function_expression" {
            return None;
        }

        let text = &content[node.byte_range()];
        let first_line = text.lines().next().unwrap_or(text);

        // Try to get name from parent binding
        let name = node
            .parent()
            .filter(|p| p.kind() == "binding")
            .and_then(|p| p.child_by_field_name("attrpath"))
            .map(|n| content[n.byte_range()].to_string())
            .unwrap_or_else(|| "<lambda>".to_string());

        Some(Symbol {
            name,
            kind: SymbolKind::Function,
            signature: first_line.trim().chars().take(80).collect(),
            docstring: self.extract_docstring(node, content),
            attributes: Vec::new(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
            children: Vec::new(),
            is_interface_impl: false,
            implements: Vec::new(),
        })
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        let kind_str = node.kind();
        if !matches!(
            kind_str,
            "attrset_expression" | "let_expression" | "rec_attrset_expression"
        ) {
            return None;
        }

        // Try to get name from parent binding
        let name = node
            .parent()
            .filter(|p| p.kind() == "binding")
            .and_then(|p| p.child_by_field_name("attrpath"))
            .map(|n| content[n.byte_range()].to_string())
            .unwrap_or_else(|| match kind_str {
                "let_expression" => "let".to_string(),
                "rec_attrset_expression" => "rec { }".to_string(),
                _ => "{ }".to_string(),
            });

        Some(Symbol {
            name: name.clone(),
            kind: SymbolKind::Module,
            signature: name,
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

    fn extract_docstring(&self, node: &Node, content: &str) -> Option<String> {
        // Nix uses # for comments
        let mut prev = node.prev_sibling();
        let mut doc_lines = Vec::new();

        while let Some(sibling) = prev {
            let text = &content[sibling.byte_range()];
            if sibling.kind() == "comment" && text.starts_with('#') {
                let line = text.strip_prefix('#').unwrap_or(text).trim();
                doc_lines.push(line.to_string());
                prev = sibling.prev_sibling();
            } else {
                break;
            }
        }

        if doc_lines.is_empty() {
            return None;
        }

        doc_lines.reverse();
        Some(doc_lines.join(" "))
    }

    fn extract_attributes(&self, _node: &Node, _content: &str) -> Vec<String> {
        Vec::new()
    }

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        if node.kind() != "apply_expression" {
            return Vec::new();
        }

        let text = &content[node.byte_range()];
        if !text.starts_with("import ") {
            return Vec::new();
        }

        // Extract path after "import"
        let rest = text.strip_prefix("import ").unwrap_or("").trim();
        let module = rest.split_whitespace().next().unwrap_or(rest).to_string();

        vec![Import {
            module,
            names: Vec::new(),
            alias: None,
            is_wildcard: false,
            is_relative: rest.starts_with('.'),
            line: node.start_position().row + 1,
        }]
    }

    fn format_import(&self, import: &Import, _names: Option<&[&str]>) -> String {
        // Nix: import ./path.nix
        format!("import {}", import.module)
    }

    fn is_public(&self, _node: &Node, _content: &str) -> bool {
        true
    }
    fn get_visibility(&self, _node: &Node, _content: &str) -> Visibility {
        Visibility::Public
    }

    fn is_test_symbol(&self, symbol: &crate::Symbol) -> bool {
        let name = symbol.name.as_str();
        match symbol.kind {
            crate::SymbolKind::Function | crate::SymbolKind::Method => name.starts_with("test_"),
            crate::SymbolKind::Module => name == "tests" || name == "test",
            _ => false,
        }
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

    fn node_name<'a>(&self, node: &Node, content: &'a str) -> Option<&'a str> {
        node.child_by_field_name("attrpath")
            .map(|n| &content[n.byte_range()])
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        if ext != "nix" {
            return None;
        }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![format!("{}.nix", module), format!("{}/default.nix", module)]
    }

    fn lang_key(&self) -> &'static str {
        "nix"
    }

    fn is_stdlib_import(&self, import_name: &str, _project_root: &Path) -> bool {
        import_name.starts_with("<nixpkgs") || import_name.starts_with("<nixos")
    }

    fn find_stdlib(&self, _project_root: &Path) -> Option<PathBuf> {
        None
    }

    fn resolve_local_import(
        &self,
        import: &str,
        current_file: &Path,
        _project_root: &Path,
    ) -> Option<PathBuf> {
        if import.starts_with('.') {
            let dir = current_file.parent()?;
            let full = dir.join(import);
            if full.is_file() {
                return Some(full);
            }
            let default = full.join("default.nix");
            if default.is_file() {
                return Some(default);
            }
        }
        None
    }

    fn resolve_external_import(
        &self,
        _import_name: &str,
        _project_root: &Path,
    ) -> Option<ResolvedPackage> {
        None
    }

    fn get_version(&self, project_root: &Path) -> Option<String> {
        if project_root.join("flake.nix").is_file() {
            return Some("flake".to_string());
        }
        if project_root.join("default.nix").is_file() {
            return Some("nix".to_string());
        }
        None
    }

    fn find_package_cache(&self, _project_root: &Path) -> Option<PathBuf> {
        PathBuf::from("/nix/store")
            .is_dir()
            .then(|| PathBuf::from("/nix/store"))
    }

    fn indexable_extensions(&self) -> &'static [&'static str] {
        &["nix"]
    }
    fn package_sources(&self, _project_root: &Path) -> Vec<crate::PackageSource> {
        Vec::new()
    }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{has_extension, skip_dotfiles};
        if skip_dotfiles(name) {
            return true;
        }
        if is_dir && name == "result" {
            return true;
        }
        !is_dir && !has_extension(name, self.indexable_extensions())
    }

    fn discover_packages(&self, _source: &crate::PackageSource) -> Vec<(String, PathBuf)> {
        Vec::new()
    }

    fn package_module_name(&self, entry_name: &str) -> String {
        entry_name
            .strip_suffix(".nix")
            .unwrap_or(entry_name)
            .to_string()
    }

    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() {
            return Some(path.to_path_buf());
        }
        let default = path.join("default.nix");
        if default.is_file() {
            return Some(default);
        }
        None
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
            "assert_expression", "binary_expression", "float_expression",
            "formal", "formals", "has_attr_expression", "hpath_expression",
            "identifier", "indented_string_expression", "integer_expression",
            "list_expression", "let_attrset_expression", "parenthesized_expression",
            "path_expression", "select_expression", "spath_expression",
            "string_expression", "unary_expression", "uri_expression",
            "variable_expression",
        ];
        validate_unused_kinds_audit(&Nix, documented_unused)
            .expect("Nix unused node kinds audit failed");
    }
}
