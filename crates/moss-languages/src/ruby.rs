//! Ruby language support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// Ruby language support.
pub struct Ruby;

impl Language for Ruby {
    fn name(&self) -> &'static str {
        "Ruby"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["rb"]
    }
    fn grammar_name(&self) -> &'static str {
        "ruby"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["class", "module"]
    }
    fn function_kinds(&self) -> &'static [&'static str] {
        &["method", "singleton_method"]
    }
    fn type_kinds(&self) -> &'static [&'static str] {
        &["class", "module"]
    }
    fn import_kinds(&self) -> &'static [&'static str] {
        &["call"] // require, require_relative, load are method calls
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["class", "module", "method", "singleton_method"]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::AllPublic // Ruby methods are public by default
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["do_block", "block", "lambda", "for"]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &[
            "if", "unless", "case", "while", "until", "for", "return", "break", "next", "redo",
            "retry", "begin",
        ]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &[
            "if",
            "unless",
            "case",
            "when",
            "while",
            "until",
            "for",
            "begin", // rescue clauses
            "rescue",
            "and",
            "or",
            "conditional",
        ]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &[
            "if",
            "unless",
            "case",
            "while",
            "until",
            "for",
            "begin",
            "method",
            "singleton_method",
            "class",
            "module",
            "do_block",
            "block",
        ]
    }

    fn signature_suffix(&self) -> &'static str {
        "; end"
    }

    fn extract_function(&self, node: &Node, content: &str, _in_container: bool) -> Option<Symbol> {
        let name = self.node_name(node, content)?;
        Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Method,
            signature: format!("def {}", name),
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
        let name = self.node_name(node, content)?;
        let kind = if node.kind() == "module" {
            SymbolKind::Module
        } else {
            SymbolKind::Class
        };

        Some(Symbol {
            name: name.to_string(),
            kind,
            signature: format!("{} {}", kind.as_str(), name),
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

    fn extract_type(&self, node: &Node, content: &str) -> Option<Symbol> {
        self.extract_container(node, content)
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

    fn format_import(&self, import: &Import, _names: Option<&[&str]>) -> String {
        // Ruby: require 'x' or require_relative 'x'
        if import.is_relative {
            format!("require_relative '{}'", import.module)
        } else {
            format!("require '{}'", import.module)
        }
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        let name = match self.node_name(node, content) {
            Some(n) => n.to_string(),
            None => return Vec::new(),
        };
        let kind = match node.kind() {
            "class" => SymbolKind::Class,
            "module" => SymbolKind::Module,
            "method" | "singleton_method" => SymbolKind::Method,
            _ => return Vec::new(),
        };
        vec![Export {
            name,
            kind,
            line: node.start_position().row + 1,
        }]
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

    fn container_body<'a>(&self, node: &'a Node<'a>) -> Option<Node<'a>> {
        node.child_by_field_name("body")
    }
    fn body_has_docstring(&self, _body: &Node, _content: &str) -> bool {
        false
    }

    fn node_name<'a>(&self, node: &Node, content: &'a str) -> Option<&'a str> {
        let name_node = node.child_by_field_name("name")?;
        Some(&content[name_node.byte_range()])
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        if path.extension()?.to_str()? != "rb" {
            return None;
        }
        Some(path.to_string_lossy().to_string())
    }
    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![format!("{}.rb", module)]
    }

    fn lang_key(&self) -> &'static str {
        "ruby"
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
        &["rb"]
    }
    fn find_stdlib(&self, _: &Path) -> Option<PathBuf> {
        None
    }
    fn package_module_name(&self, name: &str) -> String {
        name.strip_suffix(".rb").unwrap_or(name).to_string()
    }
    fn package_sources(&self, _: &Path) -> Vec<crate::PackageSource> {
        Vec::new()
    }
    fn discover_packages(&self, _: &crate::PackageSource) -> Vec<(String, PathBuf)> {
        Vec::new()
    }
    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() {
            Some(path.to_path_buf())
        } else {
            None
        }
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
            // STRUCTURAL
            "begin_block", "block_argument", "block_body", "block_parameter", "block_parameters",
            "body_statement", "class_variable", "destructured_left_assignment",
            "destructured_parameter", "else", "elsif", "empty_statement", "end_block",
            "exception_variable", "exceptions", "expression_reference_pattern", "forward_argument",
            "forward_parameter", "heredoc_body", "identifier", "lambda_parameters",
            "method_parameters", "operator", "operator_assignment", "parenthesized_statements",
            "singleton_class", "superclass",
            // CLAUSE
            "case_match", "if_guard", "if_modifier", "in_clause", "match_pattern",
            "rescue_modifier", "unless_modifier", "until_modifier", "while_modifier",
            // EXPRESSION
            "yield",
        ];

        validate_unused_kinds_audit(&Ruby, documented_unused)
            .expect("Ruby unused node kinds audit failed");
    }
}
