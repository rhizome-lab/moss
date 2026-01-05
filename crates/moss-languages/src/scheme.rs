//! Scheme language support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// Scheme language support.
pub struct Scheme;

impl Language for Scheme {
    fn name(&self) -> &'static str {
        "Scheme"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["scm", "ss", "rkt"]
    }
    fn grammar_name(&self) -> &'static str {
        "scheme"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["list"] // (define-library ...), (module ...)
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["list"] // (define (name args) ...)
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["list"] // (define-record-type ...)
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["list"] // (import ...), (require ...)
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["list"]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::AllPublic
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        if node.kind() != "list" {
            return Vec::new();
        }

        let text = &content[node.byte_range()];
        let line = node.start_position().row + 1;

        // (define name ...) or (define (name args) ...)
        if text.starts_with("(define ") {
            let rest = &text["(define ".len()..];
            let name = if rest.starts_with('(') {
                // (define (name args) ...)
                rest[1..].split_whitespace().next()
            } else {
                // (define name ...)
                rest.split_whitespace().next()
            };

            if let Some(name) = name {
                let kind = if rest.starts_with('(') || rest.contains("(lambda") {
                    SymbolKind::Function
                } else {
                    SymbolKind::Variable
                };

                return vec![Export {
                    name: name.to_string(),
                    kind,
                    line,
                }];
            }
        }

        if text.starts_with("(define-syntax ") {
            if let Some(name) = text["(define-syntax ".len()..].split_whitespace().next() {
                return vec![Export {
                    name: name.to_string(),
                    kind: SymbolKind::Function,
                    line,
                }];
            }
        }

        Vec::new()
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["list"] // let, let*, letrec, lambda
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &["list"] // if, cond, case, when
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

        if text.starts_with("(define ") {
            let rest = &text["(define ".len()..];

            // Only extract function definitions
            if rest.starts_with('(') || rest.contains("(lambda") {
                let name = if rest.starts_with('(') {
                    rest[1..].split_whitespace().next()
                } else {
                    rest.split_whitespace().next()
                }?;

                return Some(Symbol {
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
                });
            }
        }

        if text.starts_with("(define-syntax ") {
            let name = text["(define-syntax ".len()..].split_whitespace().next()?;
            return Some(Symbol {
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
            });
        }

        None
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        if node.kind() != "list" {
            return None;
        }

        let text = &content[node.byte_range()];

        if text.starts_with("(define-library ")
            || text.starts_with("(library ")
            || text.starts_with("(module ")
        {
            let prefix_len = if text.starts_with("(define-library ") {
                16
            } else if text.starts_with("(library ") {
                9
            } else {
                8
            };

            let name = text[prefix_len..]
                .split(|c: char| c.is_whitespace() || c == ')')
                .next()?
                .to_string();

            return Some(Symbol {
                name: name.clone(),
                kind: SymbolKind::Module,
                signature: format!("(library {})", name),
                docstring: None,
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
    fn extract_docstring(&self, _node: &Node, _content: &str) -> Option<String> {
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

        for prefix in &["(import ", "(require "] {
            if text.starts_with(prefix) {
                return vec![Import {
                    module: "import".to_string(),
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

    fn format_import(&self, import: &Import, names: Option<&[&str]>) -> String {
        // Scheme: (import (library)) or (import (only (library) a b c))
        let names_to_use: Vec<&str> = names
            .map(|n| n.to_vec())
            .unwrap_or_else(|| import.names.iter().map(|s| s.as_str()).collect());
        if names_to_use.is_empty() {
            format!("(import ({}))", import.module)
        } else {
            format!(
                "(import (only ({}) {}))",
                import.module,
                names_to_use.join(" ")
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
    fn node_name<'a>(&self, _node: &Node, _content: &'a str) -> Option<&'a str> {
        None
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        if !["scm", "ss", "rkt"].contains(&ext) {
            return None;
        }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![
            format!("{}.scm", module),
            format!("{}.ss", module),
            format!("{}.rkt", module),
        ]
    }

    fn lang_key(&self) -> &'static str {
        "scheme"
    }

    fn is_stdlib_import(&self, import_name: &str, _project_root: &Path) -> bool {
        import_name.starts_with("scheme/") || import_name.starts_with("srfi/")
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
        &["scm", "ss", "rkt"]
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
            .strip_suffix(".scm")
            .or_else(|| entry_name.strip_suffix(".ss"))
            .or_else(|| entry_name.strip_suffix(".rkt"))
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
            "block_comment",
        ];
        validate_unused_kinds_audit(&Scheme, documented_unused)
            .expect("Scheme unused node kinds audit failed");
    }
}
