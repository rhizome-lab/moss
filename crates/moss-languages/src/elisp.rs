//! Emacs Lisp language support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// Emacs Lisp language support.
pub struct Elisp;

impl Language for Elisp {
    fn name(&self) -> &'static str {
        "Emacs Lisp"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["el", "elc"]
    }
    fn grammar_name(&self) -> &'static str {
        "elisp"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["list"] // (defgroup ...), etc.
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["list"] // (defun ...), (defmacro ...), etc.
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["list"] // (cl-defstruct ...)
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["list"] // (require ...), (load ...)
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["list"]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::NamingConvention // prefix-- for internal
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        if node.kind() != "list" {
            return Vec::new();
        }

        let text = &content[node.byte_range()];
        let line = node.start_position().row + 1;

        for prefix in &["(defun ", "(defmacro ", "(defsubst ", "(cl-defun "] {
            if text.starts_with(prefix) {
                if let Some(name) = text[prefix.len()..].split_whitespace().next() {
                    // Skip internal functions (double dash convention)
                    if name.contains("--") {
                        return Vec::new();
                    }
                    return vec![Export {
                        name: name.to_string(),
                        kind: SymbolKind::Function,
                        line,
                    }];
                }
            }
        }

        if text.starts_with("(defvar ")
            || text.starts_with("(defconst ")
            || text.starts_with("(defcustom ")
        {
            let prefix_len = if text.starts_with("(defvar ") {
                8
            } else if text.starts_with("(defconst ") {
                10
            } else {
                11
            };
            if let Some(name) = text[prefix_len..].split_whitespace().next() {
                if !name.contains("--") {
                    return vec![Export {
                        name: name.to_string(),
                        kind: SymbolKind::Variable,
                        line,
                    }];
                }
            }
        }

        Vec::new()
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["list"] // let, let*, lambda
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &["list"] // if, cond, when, unless, while
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &["list"]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &["list"]
    }

    fn signature_suffix(&self) -> &'static str {
        ""
    }

    fn extract_function(&self, node: &Node, content: &str, _in_container: bool) -> Option<Symbol> {
        if node.kind() != "list" {
            return None;
        }

        let text = &content[node.byte_range()];
        let first_line = text.lines().next().unwrap_or(text);

        for prefix in &["(defun ", "(defmacro ", "(defsubst ", "(cl-defun "] {
            if text.starts_with(prefix) {
                if let Some(name) = text[prefix.len()..].split_whitespace().next() {
                    let is_private = name.contains("--");
                    return Some(Symbol {
                        name: name.to_string(),
                        kind: SymbolKind::Function,
                        signature: first_line.trim().to_string(),
                        docstring: self.extract_docstring(node, content),
                        attributes: Vec::new(),
                        start_line: node.start_position().row + 1,
                        end_line: node.end_position().row + 1,
                        visibility: if is_private {
                            Visibility::Private
                        } else {
                            Visibility::Public
                        },
                        children: Vec::new(),
                        is_interface_impl: false,
                        implements: Vec::new(),
                    });
                }
            }
        }

        None
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        if node.kind() != "list" {
            return None;
        }

        let text = &content[node.byte_range()];

        if text.starts_with("(defgroup ") {
            let name = text["(defgroup ".len()..].split_whitespace().next()?;
            return Some(Symbol {
                name: name.to_string(),
                kind: SymbolKind::Module,
                signature: format!("(defgroup {})", name),
                docstring: self.extract_docstring(node, content),
                attributes: Vec::new(),
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                visibility: Visibility::Public,
                children: Vec::new(),
                is_interface_impl: false,
                implements: Vec::new(),
            });
        }

        None
    }

    fn extract_type(&self, _node: &Node, _content: &str) -> Option<Symbol> {
        None
    }

    fn extract_docstring(&self, node: &Node, content: &str) -> Option<String> {
        // Elisp docstrings are strings after the argument list
        let text = &content[node.byte_range()];
        // Find first quoted string after arglist
        if let Some(paren_end) = text.find(')') {
            let after_args = &text[paren_end + 1..];
            if let Some(start) = after_args.find('"')
                && let Some(end) = after_args[start + 1..].find('"')
            {
                return Some(after_args[start + 1..start + 1 + end].to_string());
            }
        }
        None
    }

    fn extract_attributes(&self, _node: &Node, _content: &str) -> Vec<String> {
        Vec::new()
    }

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        if node.kind() != "list" {
            return Vec::new();
        }

        let text = &content[node.byte_range()];
        let line = node.start_position().row + 1;

        if text.starts_with("(require ") {
            let module = text["(require ".len()..]
                .split(|c: char| c.is_whitespace() || c == ')')
                .next()
                .map(|s| s.trim_matches('\''))
                .unwrap_or("")
                .to_string();

            if !module.is_empty() {
                return vec![Import {
                    module,
                    names: Vec::new(),
                    alias: None,
                    is_wildcard: false,
                    is_relative: false,
                    line,
                }];
            }
        }

        Vec::new()
    }

    fn format_import(&self, import: &Import, _names: Option<&[&str]>) -> String {
        // Emacs Lisp: (require 'package)
        format!("(require '{})", import.module)
    }

    fn is_public(&self, node: &Node, content: &str) -> bool {
        let text = &content[node.byte_range()];
        !text.contains("--")
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
        let ext = path.extension()?.to_str()?;
        if ext != "el" {
            return None;
        }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![format!("{}.el", module)]
    }

    fn lang_key(&self) -> &'static str {
        "elisp"
    }

    fn is_stdlib_import(&self, import_name: &str, _project_root: &Path) -> bool {
        matches!(
            import_name,
            "cl-lib" | "seq" | "subr-x" | "map" | "pcase" | "rx"
        )
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
        let dir = current_file.parent()?;
        let full = dir.join(format!("{}.el", import));
        if full.is_file() { Some(full) } else { None }
    }

    fn resolve_external_import(&self, _: &str, _: &Path) -> Option<ResolvedPackage> {
        None
    }

    fn get_version(&self, project_root: &Path) -> Option<String> {
        if project_root.join("Cask").is_file() {
            return Some("Cask".to_string());
        }
        None
    }

    fn find_package_cache(&self, _project_root: &Path) -> Option<PathBuf> {
        if let Some(home) = std::env::var_os("HOME") {
            let elpa = PathBuf::from(home).join(".emacs.d/elpa");
            if elpa.is_dir() {
                return Some(elpa);
            }
        }
        None
    }

    fn indexable_extensions(&self) -> &'static [&'static str] {
        &["el"]
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
            .strip_suffix(".el")
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
            // Definition forms (we extract via text matching instead)
            "function_definition", "macro_definition", "special_form",
        ];
        validate_unused_kinds_audit(&Elisp, documented_unused)
            .expect("Emacs Lisp unused node kinds audit failed");
    }
}
