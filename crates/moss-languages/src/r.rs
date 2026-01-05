//! R language support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// R language support.
pub struct R;

impl Language for R {
    fn name(&self) -> &'static str {
        "R"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["r", "R", "rmd", "Rmd"]
    }
    fn grammar_name(&self) -> &'static str {
        "r"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &[]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_definition"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &[]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["call"] // library(), require()
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["binary_operator"] // assignments in R are binary operators
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::NamingConvention // . prefix for internal
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        // Look for assignments like: foo <- function(...) or foo = function(...)
        // In R grammar, these are binary_operator nodes
        if node.kind() != "binary_operator" {
            return Vec::new();
        }

        // Check if it's an assignment (contains <- or =)
        let text = &content[node.byte_range()];
        if !text.contains("<-") && !text.contains("=") {
            return Vec::new();
        }

        let name = match node.child(0).map(|n| &content[n.byte_range()]) {
            Some(n) => n.to_string(),
            None => return Vec::new(),
        };

        // Check if RHS is a function
        let rhs = node.child(2);
        let is_function = rhs.map_or(false, |n| n.kind() == "function_definition");

        if !is_function {
            return Vec::new();
        }

        // . prefix is internal by convention
        if name.starts_with('.') {
            return Vec::new();
        }

        vec![Export {
            name,
            kind: SymbolKind::Function,
            line: node.start_position().row + 1,
        }]
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["function_definition", "braced_expression"]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &[
            "if_statement",
            "for_statement",
            "while_statement",
            "repeat_statement",
        ]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &["if_statement", "for_statement", "while_statement"]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &[
            "function_definition",
            "if_statement",
            "for_statement",
            "braced_expression",
        ]
    }

    fn signature_suffix(&self) -> &'static str {
        ""
    }

    fn extract_function(&self, node: &Node, content: &str, _in_container: bool) -> Option<Symbol> {
        // R functions are typically assigned: name <- function(...) {}
        // We need to look at the parent assignment (binary_operator in R grammar)
        let parent = node.parent()?;
        if parent.kind() != "binary_operator" {
            return None;
        }

        let name = parent
            .child(0)
            .map(|n| content[n.byte_range()].to_string())?;
        let text = &content[parent.byte_range()];
        let first_line = text.lines().next().unwrap_or(text);

        Some(Symbol {
            name: name.clone(),
            kind: SymbolKind::Function,
            signature: first_line.trim().to_string(),
            docstring: self.extract_docstring(&parent, content),
            attributes: Vec::new(),
            start_line: parent.start_position().row + 1,
            end_line: parent.end_position().row + 1,
            visibility: if name.starts_with('.') {
                Visibility::Private
            } else {
                Visibility::Public
            },
            children: Vec::new(),
            is_interface_impl: false,
            implements: Vec::new(),
        })
    }

    fn extract_container(&self, _node: &Node, _content: &str) -> Option<Symbol> {
        None
    }
    fn extract_type(&self, _node: &Node, _content: &str) -> Option<Symbol> {
        None
    }

    fn extract_docstring(&self, node: &Node, content: &str) -> Option<String> {
        // R uses # for comments, roxygen2 uses #' for docs
        let mut prev = node.prev_sibling();
        let mut doc_lines = Vec::new();

        while let Some(sibling) = prev {
            let text = &content[sibling.byte_range()];
            if sibling.kind() == "comment" {
                if text.starts_with("#'") {
                    let line = text.strip_prefix("#'").unwrap_or(text).trim();
                    if !line.starts_with('@') {
                        doc_lines.push(line.to_string());
                    }
                }
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
        if node.kind() != "call" {
            return Vec::new();
        }

        let text = &content[node.byte_range()];
        if !text.starts_with("library(") && !text.starts_with("require(") {
            return Vec::new();
        }

        // Extract package name from library(pkg) or require(pkg)
        let inner = text
            .split('(')
            .nth(1)
            .and_then(|s| s.split(')').next())
            .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string());

        if let Some(module) = inner {
            return vec![Import {
                module,
                names: Vec::new(),
                alias: None,
                is_wildcard: true,
                is_relative: false,
                line: node.start_position().row + 1,
            }];
        }

        Vec::new()
    }

    fn format_import(&self, import: &Import, _names: Option<&[&str]>) -> String {
        // R: library(package)
        format!("library({})", import.module)
    }

    fn is_public(&self, node: &Node, content: &str) -> bool {
        node.child(0)
            .map_or(true, |n| !content[n.byte_range()].starts_with('.'))
    }

    fn get_visibility(&self, node: &Node, content: &str) -> Visibility {
        if self.is_public(node, content) {
            Visibility::Public
        } else {
            Visibility::Private
        }
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
    fn node_name<'a>(&self, _node: &Node, _content: &'a str) -> Option<&'a str> {
        None
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?.to_lowercase();
        if ext != "r" {
            return None;
        }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![format!("{}.R", module), format!("{}.r", module)]
    }

    fn lang_key(&self) -> &'static str {
        "r"
    }

    fn is_stdlib_import(&self, import_name: &str, _project_root: &Path) -> bool {
        matches!(
            import_name,
            "base"
                | "stats"
                | "graphics"
                | "grDevices"
                | "utils"
                | "datasets"
                | "methods"
                | "grid"
                | "tools"
                | "compiler"
        )
    }

    fn find_stdlib(&self, _project_root: &Path) -> Option<PathBuf> {
        None
    }

    fn resolve_local_import(
        &self,
        import: &str,
        _current_file: &Path,
        project_root: &Path,
    ) -> Option<PathBuf> {
        let full = project_root.join("R").join(format!("{}.R", import));
        if full.is_file() { Some(full) } else { None }
    }

    fn resolve_external_import(
        &self,
        _import_name: &str,
        _project_root: &Path,
    ) -> Option<ResolvedPackage> {
        None
    }

    fn get_version(&self, project_root: &Path) -> Option<String> {
        if project_root.join("DESCRIPTION").is_file() {
            return Some("R package".to_string());
        }
        None
    }

    fn find_package_cache(&self, _project_root: &Path) -> Option<PathBuf> {
        // R library paths
        if let Some(home) = std::env::var_os("HOME") {
            let lib = PathBuf::from(home).join("R/library");
            if lib.is_dir() {
                return Some(lib);
            }
        }
        None
    }

    fn indexable_extensions(&self) -> &'static [&'static str] {
        &["r", "R"]
    }
    fn package_sources(&self, _project_root: &Path) -> Vec<crate::PackageSource> {
        Vec::new()
    }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::skip_dotfiles;
        if skip_dotfiles(name) {
            return true;
        }
        if is_dir && (name == "man" || name == "inst") {
            return true;
        }
        !is_dir && !name.to_lowercase().ends_with(".r")
    }

    fn discover_packages(&self, _source: &crate::PackageSource) -> Vec<(String, PathBuf)> {
        Vec::new()
    }

    fn package_module_name(&self, entry_name: &str) -> String {
        entry_name
            .strip_suffix(".R")
            .or_else(|| entry_name.strip_suffix(".r"))
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
            "extract_operator", "identifier",
            "namespace_operator", "parenthesized_expression", "return", "unary_operator",
        ];
        validate_unused_kinds_audit(&R, documented_unused)
            .expect("R unused node kinds audit failed");
    }
}
