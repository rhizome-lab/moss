//! VHDL support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// VHDL language support.
pub struct Vhdl;

impl Language for Vhdl {
    fn name(&self) -> &'static str {
        "VHDL"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["vhd", "vhdl"]
    }
    fn grammar_name(&self) -> &'static str {
        "vhdl"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &[
            "entity_declaration",
            "architecture_body",
            "package_declaration",
        ]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_body", "procedure_body"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["full_type_declaration"]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["use_clause"]
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["entity_declaration", "package_declaration"]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::AllPublic
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        let kind = match node.kind() {
            "entity_declaration" => SymbolKind::Module,
            "package_declaration" => SymbolKind::Module,
            _ => return Vec::new(),
        };

        if let Some(name) = self.node_name(node, content) {
            return vec![Export {
                name: name.to_string(),
                kind,
                line: node.start_position().row + 1,
            }];
        }
        Vec::new()
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &[
            "entity_declaration",
            "architecture_body",
            "function_body",
            "procedure_body",
        ]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &["if_statement", "case_statement", "loop_statement"]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &["if_statement", "case_statement", "loop_statement"]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &["entity_declaration", "architecture_body"]
    }

    fn signature_suffix(&self) -> &'static str {
        ""
    }

    fn extract_function(&self, node: &Node, content: &str, _in_container: bool) -> Option<Symbol> {
        if node.kind() != "function_body" && node.kind() != "procedure_body" {
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
        if node.kind() != "entity_declaration"
            && node.kind() != "architecture_body"
            && node.kind() != "package_declaration"
        {
            return None;
        }

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

    fn extract_type(&self, node: &Node, content: &str) -> Option<Symbol> {
        if node.kind() != "full_type_declaration" {
            return None;
        }

        let name = self.node_name(node, content)?;
        let text = &content[node.byte_range()];

        Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Type,
            signature: text.trim().to_string(),
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

    fn extract_docstring(&self, _node: &Node, _content: &str) -> Option<String> {
        None
    }

    fn extract_attributes(&self, _node: &Node, _content: &str) -> Vec<String> {
        Vec::new()
    }

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        if node.kind() != "use_clause" {
            return Vec::new();
        }

        let text = &content[node.byte_range()];
        vec![Import {
            module: text.trim().to_string(),
            names: Vec::new(),
            alias: None,
            is_wildcard: text.contains(".all"),
            is_relative: false,
            line: node.start_position().row + 1,
        }]
    }

    fn format_import(&self, import: &Import, names: Option<&[&str]>) -> String {
        // VHDL: use library.package.all; or use library.package.item;
        let names_to_use: Vec<&str> = names
            .map(|n| n.to_vec())
            .unwrap_or_else(|| import.names.iter().map(|s| s.as_str()).collect());
        if import.is_wildcard || names_to_use.is_empty() {
            format!("use {}.all;", import.module)
        } else if names_to_use.len() == 1 {
            format!("use {}.{};", import.module, names_to_use[0])
        } else {
            names_to_use
                .iter()
                .map(|n| format!("use {}.{};", import.module, n))
                .collect::<Vec<_>>()
                .join("\n")
        }
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
        node.child_by_field_name("name")
            .map(|n| &content[n.byte_range()])
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        if !["vhd", "vhdl"].contains(&ext) {
            return None;
        }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![format!("{}.vhd", module), format!("{}.vhdl", module)]
    }

    fn lang_key(&self) -> &'static str {
        "vhdl"
    }

    fn is_stdlib_import(&self, _: &str, _: &Path) -> bool {
        false
    }
    fn find_stdlib(&self, _: &Path) -> Option<PathBuf> {
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
        &["vhd", "vhdl"]
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
            .strip_suffix(".vhd")
            .or_else(|| entry_name.strip_suffix(".vhdl"))
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
            // Declarations
            "constant_declaration", "signal_declaration", "variable_declaration",
            "shared_variable_declaration", "file_declaration", "alias_declaration",
            "attribute_declaration", "attribute_specification", "component_declaration",
            "group_template_declaration", "group_declaration", "subtype_declaration",
            "incomplete_type_declaration", "disconnection_specification",
            "configuration_specification", "configuration_declaration",
            // Type definitions
            "enumeration_type_definition", "physical_type_definition",
            "primary_unit_declaration", "secondary_unit_declaration", "record_type_definition",
            "element_declaration", "access_type_definition", "file_type_definition",
            "constrained_array_definition", "unbounded_array_definition",
            "index_subtype_definition", "numeric_type_definition",
            // Protected type
            "protected_type_declaration", "protected_type_body",
            // Procedures/functions
            "procedure_declaration", "function_declaration",
            "procedure_instantiation_declaration", "function_instantiation_declaration",
            "procedure_parameter_clause", "function_parameter_clause",
            "procedure_call_statement",
            // Interface declarations
            "constant_interface_declaration", "signal_interface_declaration",
            "variable_interface_declaration", "file_interface_declaration",
            "type_interface_declaration", "procedure_interface_declaration",
            "function_interface_declaration", "package_interface_declaration",
            "interface_subprogram_default",
            // Process/statements
            "process_statement", "concurrent_statement_part", "sequence_of_statements",
            "wait_statement", "assertion_statement", "report_statement",
            "next_statement", "exit_statement", "return_statement", "null_statement",
            // Assignments
            "simple_waveform_assignment", "conditional_waveform_assignment",
            "selected_waveform_assignment", "simple_force_assignment",
            "conditional_force_assignment", "selected_force_assignment",
            "simple_variable_assignment", "conditional_variable_assignment",
            "selected_variable_assignment", "simple_release_assignment",
            // Waveforms
            "waveforms", "waveform_element", "conditional_waveforms",
            "selected_waveforms", "alternative_selected_waveforms",
            "alternative_conditional_waveforms",
            // Expressions
            "expression", "simple_expression", "shift_expression", "logical_expression",
            "conditional_expression", "parenthesized_expression", "qualified_expression",
            "alternative_conditional_expressions", "alternative_selected_expressions",
            "conditional_expressions", "selected_expressions", "expression_list",
            "string_expression", "time_expression", "severity_expression",
            "inertial_expression", "default_expression", "relation",
            "exponentiation", "concatenation", "reduction", "condition",
            // Generate
            "for_generate_statement", "if_generate_statement", "case_generate_statement",
            "if_generate", "elsif_generate", "else_generate", "case_generate_alternative",
            "generate_statement_body", "generate_statement_element",
            // Block
            "block_statement", "block_header", "block_configuration", "block_specification",
            // Component/instantiation
            "component_instantiation_statement", "verification_unit_binding_indication",
            "verification_unit_list", "component_configuration", "binding_indication",
            "port_map_aspect", "generic_map_aspect",
            "entity_instantiation", "configuration_instantiation", "component_instantiation",
            "instantiation_list", "all", "component_header", "component_map_aspect",
            // Clauses
            "context_clause", "library_clause", "generic_clause", "port_clause",
            // File
            "file_open_information", "file_open_kind",
            // Identifiers
            "identifier", "extended_identifier", "identifier_list", "operator_symbol",
            "label", "simple_name", "extended_simple_name",
            "external_signal_name", "external_constant_name", "external_variable_name",
            "pathname_element", "relative_pathname", "package_pathname", "absolute_pathname",
            // Control flow helpers
            "if", "elsif", "else", "return", "for_loop", "while_loop",
            // Case
            "case_statement_alternative",
            // Packages
            "package_body", "package_instantiation_declaration", "context_declaration",
            "package_header", "package_map_aspect",
            // Entity
            "entity_specification", "entity_class", "entity_class_entry",
            "entity_class_entry_list", "entity_header", "entity_name_list",
            "entity_designator",
            // Type/subtype
            "type_mark", "subtype_indication", "resolution_function",
            "range_constraint", "array_constraint", "record_constraint",
            "record_element_constraint", "index_constraint", "array_element_constraint",
            "parenthesized_resolution", "record_resolution", "record_element_resolution",
            // Signal specification
            "guarded_signal_specification", "signal_list", "signal_kind",
            // Function
            "function_call",
            // Parameter
            "parameter_specification",
            // Associations
            "association_list", "positional_association_element", "named_association_element",
            "default",
            // Names
            "attribute_name", "slice_name", "selected_name", "ambiguous_name",
            "predefined_designator",
            // Targets
            "aggregate", "positional_element_association", "named_element_association",
            "choices", "others",
            // Ranges
            "ascending_range", "descending_range",
            // Literals
            "physical_literal", "string_literal", "bit_string_literal",
            "character_literal", "integer_decimal", "real_decimal", "based_integer",
            "based_real", "allocator", "null",
            // Operators
            "sign", "factor", "term",
            // Signatures
            "signature", "tool_directive",
            // Library
            "design_unit", "design_file", "logical_name_list",
            "context_reference", "context_list",
            // Subprogram
            "subprogram_header", "subprogram_map_aspect",
            // Concurrent statements
            "conditional_concurrent_signal_assignment",
            "selected_concurrent_signal_assignment", "simple_concurrent_signal_assignment",
            // Group
            "group_constituent_list",
            // Misc syntax elements
            "force_mode", "declarative_part", "open", "semicolon",
            "transport", "inertial", "unaffected", "delay_mechanism",
            "sensitivity_list", "same", "any", "boolean", "comment",
            // PSL (Property Specification Language)
            "PSL_Verification_Unit_Body", "PSL_Property_Declaration",
            "PSL_Sequence_Declaration", "PSL_Clock_Declaration",
            "PSL_Built_In_Function_Call", "PSL_Union_Expression",
            "PSL_Expression", "PSL_Identifier", "PSL_HDL_Type", "PSL_Any_Type",
            "PSL_Type_Class", "PSL_Formal_Parameter", "PSL_Formal_Parameter_List",
            "PSL_Parameters_Definition", "PSL_Parameter_Specification",
            "PSL_Constant_Parameter_Specification", "PSL_Temporal_Parameter_Specification",
            "PSL_Assert_Directive", "PSL_Assume_Directive", "PSL_Assume_Guarantee_Directive",
            "PSL_Cover_Directive", "PSL_Restrict_Directive", "PSL_Restrict_Guarantee_Directive",
            "PSL_Fairness_Directive", "PSL_Strong_Fairness_Directive",
            "PSL_Property_Instance", "PSL_Sequence_Instance", "PSL_Actual_Parameter_List",
            "PSL_Property_Replicator", "PSL_Count", "PSL_Number",
            "PSL_Boolean", "PSL_Value_Set",
            "PSL_Compound_SERE", "PSL_Repeated_SERE",
            "PSL_Braced_SERE", "PSL_Clocked_SERE", "PSL_Simple_SERE",
            "PSL_Parameterized_SERE", "PSL_Ambiguous_Instance",
            "PSL_Parameterized_Property", "PSL_Index_Range",
            "PSL_Actual_Parameter",
            "PSL_Parenthesized_FL_Property", "PSL_Sequential_FL_Property",
            "PSL_Clocked_FL_Property", "PSL_Invariant_FL_Property",
            "PSL_Ocurrence_FL_Property", "PSL_Implication_FL_Property",
            "PSL_Logical_FL_Property", "PSL_Factor_FL_Property",
            "PSL_Extended_Ocurrence_FL_Property", "PSL_Termination_FL_Property",
            "PSL_Bounding_FL_Property", "PSL_Suffix_Implication_FL_Property",
            "PSL_VUnit", "PSL_VProp", "PSL_VMode",
            "PSL_Hierarchical_HDL_Name", "PSL_Inherit_Spec",
        ];
        validate_unused_kinds_audit(&Vhdl, documented_unused)
            .expect("VHDL unused node kinds audit failed");
    }
}
