//! Visual Basic language support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// Visual Basic language support.
pub struct VB;

impl Language for VB {
    fn name(&self) -> &'static str {
        "Visual Basic"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["vb", "vbs"]
    }
    fn grammar_name(&self) -> &'static str {
        "vb"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &[
            "class_block",
            "module_block",
            "structure_block",
            "interface_block",
        ]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["method_declaration", "property_declaration"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["enum_block", "delegate_declaration"]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["imports_statement"]
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["class_block", "module_block", "method_declaration"]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::AccessModifier
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        match node.kind() {
            "class_block" | "module_block" | "structure_block" => {
                if let Some(name) = self.node_name(node, content) {
                    return vec![Export {
                        name: name.to_string(),
                        kind: SymbolKind::Class,
                        line: node.start_position().row + 1,
                    }];
                }
            }
            "method_declaration" => {
                if let Some(name) = self.node_name(node, content) {
                    return vec![Export {
                        name: name.to_string(),
                        kind: SymbolKind::Function,
                        line: node.start_position().row + 1,
                    }];
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["class_block", "module_block", "method_declaration"]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &[
            "if_statement",
            "select_case_statement",
            "while_statement",
            "for_statement",
            "for_each_statement",
            "do_statement",
        ]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &[
            "if_statement",
            "select_case_statement",
            "while_statement",
            "for_statement",
            "for_each_statement",
            "case_clause",
        ]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &[
            "if_statement",
            "select_case_statement",
            "while_statement",
            "for_statement",
            "do_statement",
        ]
    }

    fn signature_suffix(&self) -> &'static str {
        ""
    }

    fn extract_function(&self, node: &Node, content: &str, _in_container: bool) -> Option<Symbol> {
        match node.kind() {
            "method_declaration" | "property_declaration" => {
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
                    visibility: self.get_visibility(node, content),
                    children: Vec::new(),
                    is_interface_impl: false,
                    implements: Vec::new(),
                })
            }
            _ => None,
        }
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        match node.kind() {
            "class_block" | "module_block" | "structure_block" | "interface_block" => {
                let name = self.node_name(node, content)?;
                let text = &content[node.byte_range()];
                let first_line = text.lines().next().unwrap_or(text);

                Some(Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::Class,
                    signature: first_line.trim().to_string(),
                    docstring: None,
                    attributes: Vec::new(),
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    visibility: self.get_visibility(node, content),
                    children: Vec::new(),
                    is_interface_impl: false,
                    implements: Vec::new(),
                })
            }
            _ => None,
        }
    }

    fn extract_type(&self, node: &Node, content: &str) -> Option<Symbol> {
        match node.kind() {
            "enum_block" | "delegate_declaration" => {
                let name = self.node_name(node, content)?;
                let text = &content[node.byte_range()];
                let first_line = text.lines().next().unwrap_or(text);

                Some(Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::Type,
                    signature: first_line.trim().to_string(),
                    docstring: None,
                    attributes: Vec::new(),
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    visibility: self.get_visibility(node, content),
                    children: Vec::new(),
                    is_interface_impl: false,
                    implements: Vec::new(),
                })
            }
            _ => None,
        }
    }

    fn extract_docstring(&self, _node: &Node, _content: &str) -> Option<String> {
        None
    }

    fn extract_attributes(&self, _node: &Node, _content: &str) -> Vec<String> {
        Vec::new()
    }

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        if node.kind() != "imports_statement" {
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

    fn format_import(&self, import: &Import, _names: Option<&[&str]>) -> String {
        // Visual Basic: Imports Namespace
        format!("Imports {}", import.module)
    }

    fn is_public(&self, node: &Node, content: &str) -> bool {
        let text = &content[node.byte_range()];
        text.to_lowercase().contains("public") || !text.to_lowercase().contains("private")
    }

    fn get_visibility(&self, node: &Node, content: &str) -> Visibility {
        let text = &content[node.byte_range()];
        let lower = text.to_lowercase();
        if lower.contains("private") {
            Visibility::Private
        } else if lower.contains("protected") {
            Visibility::Protected
        } else {
            Visibility::Public
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

    fn container_body<'a>(&self, node: &'a Node<'a>) -> Option<Node<'a>> {
        node.child_by_field_name("body")
    }

    fn body_has_docstring(&self, _body: &Node, _content: &str) -> bool {
        false
    }

    fn node_name<'a>(&self, node: &Node, content: &'a str) -> Option<&'a str> {
        node.child_by_field_name("name")
            .map(|n| &content[n.byte_range()])
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        if !["vb", "vbs"].contains(&ext) {
            return None;
        }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![format!("{}.vb", module), format!("{}.vbs", module)]
    }

    fn lang_key(&self) -> &'static str {
        "vb"
    }

    fn is_stdlib_import(&self, import_name: &str, _project_root: &Path) -> bool {
        import_name.starts_with("System.") || import_name.starts_with("Microsoft.")
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
        &["vb", "vbs"]
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
            .strip_suffix(".vb")
            .or_else(|| entry_name.strip_suffix(".vbs"))
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
            // Block types
            "namespace_block",
            // Declaration types
            "field_declaration", "constructor_declaration", "event_declaration",
            "type_declaration", "const_declaration", "enum_member",
            // Statement types
            "statement", "assignment_statement", "compound_assignment_statement",
            "call_statement", "dim_statement", "redim_statement", "re_dim_clause",
            "exit_statement", "continue_statement", "return_statement", "goto_statement",
            "label_statement", "throw_statement", "empty_statement",
            // Control flow
            "try_statement", "catch_block", "finally_block",
            "case_block", "case_else_block", "else_clause", "elseif_clause",
            "with_statement", "with_initializer",
            "using_statement", "sync_lock_statement",
            // Expression types
            "expression", "binary_expression", "unary_expression", "ternary_expression",
            "parenthesized_expression", "lambda_expression", "new_expression",
            // Type-related
            "type", "generic_type", "array_type", "primitive_type",
            "type_parameters", "type_parameter", "type_constraint",
            "type_argument_list", "array_rank_specifier",
            // Clauses
            "as_clause", "inherits_clause", "implements_clause",
            // Modifiers
            "modifier", "modifiers",
            // Event handlers
            "add_handler_block", "remove_handler_block", "raise_event_block",
            // Other
            "identifier", "attribute_block", "option_statements",
            "relational_operator", "lambda_parameter",
        ];
        validate_unused_kinds_audit(&VB, documented_unused)
            .expect("Visual Basic unused node kinds audit failed");
    }
}
