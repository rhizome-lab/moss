//! Prolog language support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// Prolog language support.
pub struct Prolog;

impl Language for Prolog {
    fn name(&self) -> &'static str {
        "Prolog"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["pl", "pro", "prolog"]
    }
    fn grammar_name(&self) -> &'static str {
        "prolog"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["directive_term"] // module declarations
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["clause_term"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &[]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["directive_term"] // use_module directives
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["clause_term", "directive_term"]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::ExplicitExport
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        if node.kind() != "clause_term" {
            return Vec::new();
        }

        if let Some(name) = self.node_name(node, content) {
            return vec![Export {
                name: name.to_string(),
                kind: SymbolKind::Function,
                line: node.start_position().row + 1,
            }];
        }

        Vec::new()
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["clause_term"]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &[] // Prolog uses pattern matching and backtracking
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &["clause_term"] // Each clause adds complexity
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &["clause_term"]
    }

    fn signature_suffix(&self) -> &'static str {
        ""
    }

    fn extract_function(&self, node: &Node, content: &str, _in_container: bool) -> Option<Symbol> {
        if node.kind() != "clause_term" {
            return None;
        }

        let name = self.node_name(node, content)?;
        let text = &content[node.byte_range()];
        let first_line = text.lines().next().unwrap_or(text);

        Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Function,
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

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        if node.kind() != "directive_term" {
            return None;
        }

        let text = &content[node.byte_range()];
        if !text.contains("module(") {
            return None;
        }

        let name = self.node_name(node, content)?;
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
        if node.kind() != "directive_term" {
            return Vec::new();
        }

        let text = &content[node.byte_range()];
        if text.contains("use_module(") {
            return vec![Import {
                module: text.trim().to_string(),
                names: Vec::new(),
                alias: None,
                is_wildcard: false,
                is_relative: false,
                line: node.start_position().row + 1,
            }];
        }

        Vec::new()
    }

    fn format_import(&self, import: &Import, names: Option<&[&str]>) -> String {
        // Prolog: :- use_module(module) or :- use_module(module, [pred/arity])
        let names_to_use: Vec<&str> = names
            .map(|n| n.to_vec())
            .unwrap_or_else(|| import.names.iter().map(|s| s.as_str()).collect());
        if names_to_use.is_empty() {
            format!(":- use_module({}).", import.module)
        } else {
            format!(
                ":- use_module({}, [{}]).",
                import.module,
                names_to_use.join(", ")
            )
        }
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
        // For clauses, get the predicate name
        let head = if let Some(h) = node.child_by_field_name("head") {
            h
        } else {
            let mut cursor = node.walk();
            let mut found = None;
            for child in node.children(&mut cursor) {
                if child.kind() == "atom" || child.kind() == "compound_term" {
                    found = Some(child);
                    break;
                }
            }
            found?
        };

        // Get first atom child as the predicate name
        let mut cursor = head.walk();
        for child in head.children(&mut cursor) {
            if child.kind() == "atom" {
                return Some(&content[child.byte_range()]);
            }
        }
        Some(&content[head.byte_range()])
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        if !["pl", "pro", "prolog"].contains(&ext) {
            return None;
        }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![
            format!("{}.pl", module),
            format!("{}.pro", module),
            format!("{}.prolog", module),
        ]
    }

    fn lang_key(&self) -> &'static str {
        "prolog"
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
        &["pl", "pro", "prolog"]
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
            .strip_suffix(".pl")
            .or_else(|| entry_name.strip_suffix(".pro"))
            .or_else(|| entry_name.strip_suffix(".prolog"))
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
            "binary_operator", "functional_notation",
            "operator_notation", "prefix_operator", "prexif_operator",
        ];
        validate_unused_kinds_audit(&Prolog, documented_unused)
            .expect("Prolog unused node kinds audit failed");
    }
}
