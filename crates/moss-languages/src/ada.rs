//! Ada language support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// Ada language support.
pub struct Ada;

impl Language for Ada {
    fn name(&self) -> &'static str {
        "Ada"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["ada", "adb", "ads"]
    }
    fn grammar_name(&self) -> &'static str {
        "ada"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &[
            "package_declaration",
            "package_body",
            "generic_package_declaration",
        ]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &[
            "subprogram_declaration",
            "subprogram_body",
            "expression_function_declaration",
        ]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &[
            "full_type_declaration",
            "private_type_declaration",
            "incomplete_type_declaration",
        ]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["with_clause", "use_clause"]
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &[
            "package_declaration",
            "subprogram_declaration",
            "full_type_declaration",
        ]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::AllPublic // Ada uses separate spec/body files
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        match node.kind() {
            "package_declaration" | "package_body" | "generic_package_declaration" => {
                if let Some(name) = self.node_name(node, content) {
                    return vec![Export {
                        name: name.to_string(),
                        kind: SymbolKind::Module,
                        line: node.start_position().row + 1,
                    }];
                }
            }
            "subprogram_declaration" | "subprogram_body" | "expression_function_declaration" => {
                if let Some(name) = self.node_name(node, content) {
                    return vec![Export {
                        name: name.to_string(),
                        kind: SymbolKind::Function,
                        line: node.start_position().row + 1,
                    }];
                }
            }
            "full_type_declaration" | "private_type_declaration" => {
                if let Some(name) = self.node_name(node, content) {
                    return vec![Export {
                        name: name.to_string(),
                        kind: SymbolKind::Type,
                        line: node.start_position().row + 1,
                    }];
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &[
            "package_body",
            "subprogram_body",
            "block_statement",
            "loop_statement",
        ]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        // Ada grammar uses expression-based nodes
        &["case_expression", "if_expression", "quantified_expression"]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &[
            "case_expression",
            "if_expression",
            "case_expression_alternative",
        ]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &["case_expression", "if_expression", "declare_expression"]
    }

    fn signature_suffix(&self) -> &'static str {
        ""
    }

    fn extract_function(&self, node: &Node, content: &str, _in_container: bool) -> Option<Symbol> {
        match node.kind() {
            "subprogram_declaration" | "subprogram_body" | "expression_function_declaration" => {
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
            _ => None,
        }
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        match node.kind() {
            "package_declaration" | "package_body" | "generic_package_declaration" => {
                let name = self.node_name(node, content)?;
                let text = &content[node.byte_range()];
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
            _ => None,
        }
    }

    fn extract_type(&self, node: &Node, content: &str) -> Option<Symbol> {
        match node.kind() {
            "full_type_declaration"
            | "private_type_declaration"
            | "incomplete_type_declaration" => {
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
                    visibility: Visibility::Public,
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
        match node.kind() {
            "with_clause" => {
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
            "use_clause" => {
                let text = &content[node.byte_range()];
                vec![Import {
                    module: text.trim().to_string(),
                    names: Vec::new(),
                    alias: None,
                    is_wildcard: true,
                    is_relative: false,
                    line: node.start_position().row + 1,
                }]
            }
            _ => Vec::new(),
        }
    }

    fn format_import(&self, import: &Import, _names: Option<&[&str]>) -> String {
        // Ada: with Package;
        format!("with {};", import.module)
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
        node.child_by_field_name("declarations")
            .or_else(|| node.child_by_field_name("statements"))
    }

    fn body_has_docstring(&self, _body: &Node, _content: &str) -> bool {
        false
    }

    fn node_name<'a>(&self, node: &Node, content: &'a str) -> Option<&'a str> {
        if let Some(name_node) = node.child_by_field_name("name") {
            return Some(&content[name_node.byte_range()]);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" || child.kind() == "defining_identifier" {
                return Some(&content[child.byte_range()]);
            }
        }
        None
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        if !["ada", "adb", "ads"].contains(&ext) {
            return None;
        }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.replace('-', "."))
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        let base = module.to_lowercase().replace('.', "-");
        vec![format!("{}.ads", base), format!("{}.adb", base)]
    }

    fn lang_key(&self) -> &'static str {
        "ada"
    }

    fn is_stdlib_import(&self, import_name: &str, _project_root: &Path) -> bool {
        import_name.starts_with("Ada.") || import_name.starts_with("GNAT.")
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
        &["ada", "adb", "ads"]
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
            .strip_suffix(".ads")
            .or_else(|| entry_name.strip_suffix(".adb"))
            .or_else(|| entry_name.strip_suffix(".ada"))
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
            // Type definitions
            "access_definition", "access_to_object_definition", "access_to_subprogram_definition",
            "array_type_definition", "decimal_fixed_point_definition", "derived_type_definition",
            "enumeration_type_definition", "floating_point_definition", "formal_access_type_definition",
            "formal_array_type_definition", "formal_decimal_fixed_point_definition",
            "formal_derived_type_definition", "formal_discrete_type_definition",
            "formal_floating_point_definition", "formal_interface_type_definition",
            "formal_modular_type_definition", "formal_ordinary_fixed_point_definition",
            "formal_private_type_definition", "formal_signed_integer_type_definition",
            "interface_type_definition", "modular_type_definition", "ordinary_fixed_point_definition",
            "record_type_definition", "signed_integer_type_definition",
            // Declarations
            "body_stub", "component_declaration", "component_definition", "discriminant_specification",
            "discriminant_specification_list", "entry_declaration", "exception_declaration",
            "formal_abstract_subprogram_declaration", "formal_complete_type_declaration",
            "formal_concrete_subprogram_declaration", "formal_incomplete_type_declaration",
            "formal_object_declaration", "formal_package_declaration", "formal_subprogram_declaration",
            "generic_formal_part", "generic_renaming_declaration", "generic_subprogram_declaration",
            "null_procedure_declaration", "number_declaration", "object_declaration",
            "object_renaming_declaration", "package_renaming_declaration", "parameter_specification",
            "private_extension_declaration", "single_protected_declaration", "single_task_declaration",
            "subprogram_renaming_declaration", "subtype_declaration",
            // Protected and task types
            "protected_body", "protected_body_stub", "protected_definition", "protected_type_declaration",
            "task_body", "task_body_stub", "task_definition", "task_type_declaration",
            // Stubs
            "package_body_stub", "subprogram_body_stub",
            // Statements
            "abort_statement", "accept_statement", "assignment_statement", "case_statement_alternative",
            "delay_relative_statement", "delay_until_statement", "goto_statement", "null_statement",
            "procedure_call_statement", "raise_statement", "requeue_statement", "simple_return_statement",
            // Expressions
            "qualified_expression", "raise_expression",
            // Potentially useful - control flow
            "exception_handler", "if_statement", "exit_statement", "case_statement",
            // Representation clauses
            "at_clause", "attribute_definition_clause", "component_clause", "enumeration_aggregate",
            "enumeration_representation_clause", "mod_clause", "record_representation_clause",
            // Control flow and statements
            "asynchronous_select", "conditional_entry_call", "entry_body", "entry_barrier",
            "entry_call_alternative", "entry_index_specification", "extended_return_object_declaration",
            "extended_return_statement", "handled_sequence_of_statements", "loop_label",
            "loop_parameter_specification", "timed_entry_call",
            // Contracts and aspects
            "aspect_specification", "global_aspect_definition",
            // GNAT-specific
            "gnatprep_declarative_if_statement", "gnatprep_identifier", "gnatprep_if_statement",
            // Expressions and operators
            "binary_adding_operator", "choice_parameter_specification", "chunk_specification",
            "elsif_expression_item", "elsif_statement_item", "exception_choice", "exception_choice_list",
            "exception_renaming_declaration", "expression", "formal_part", "function_call",
            "function_specification", "general_access_modifier", "identifier", "index_subtype_definition",
            "iterator_specification", "multiplying_operator", "procedure_specification", "quantifier",
            "real_range_specification", "record_definition", "reduction_specification",
            "relational_operator", "subpool_specification", "unary_adding_operator",
        ];
        validate_unused_kinds_audit(&Ada, documented_unused)
            .expect("Ada unused node kinds audit failed");
    }
}
