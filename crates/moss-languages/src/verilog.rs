//! Verilog/SystemVerilog support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// Verilog language support.
pub struct Verilog;

impl Language for Verilog {
    fn name(&self) -> &'static str {
        "Verilog"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["v", "sv", "svh"]
    }
    fn grammar_name(&self) -> &'static str {
        "verilog"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["module_declaration"]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_declaration", "task_declaration"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &[]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["package_import_declaration"]
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["module_declaration"]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::AllPublic
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        if node.kind() != "module_declaration" {
            return Vec::new();
        }

        if let Some(name) = self.node_name(node, content) {
            return vec![Export {
                name: name.to_string(),
                kind: SymbolKind::Module,
                line: node.start_position().row + 1,
            }];
        }
        Vec::new()
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &[
            "module_declaration",
            "function_declaration",
            "task_declaration",
        ]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &[
            "if_generate_construct",
            "case_generate_construct",
            "conditional_statement",
            "case_statement",
        ]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &["conditional_statement", "case_statement", "loop_statement"]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &["module_declaration"]
    }

    fn signature_suffix(&self) -> &'static str {
        ""
    }

    fn extract_function(&self, node: &Node, content: &str, _in_container: bool) -> Option<Symbol> {
        if node.kind() != "function_declaration" && node.kind() != "task_declaration" {
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
        if node.kind() != "module_declaration" {
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
        if node.kind() != "package_import_declaration" {
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

    fn format_import(&self, import: &Import, names: Option<&[&str]>) -> String {
        // Verilog: import package::*; or import package::item;
        let names_to_use: Vec<&str> = names
            .map(|n| n.to_vec())
            .unwrap_or_else(|| import.names.iter().map(|s| s.as_str()).collect());
        if names_to_use.is_empty() {
            format!("import {}::*;", import.module)
        } else if names_to_use.len() == 1 {
            format!("import {}::{};", import.module, names_to_use[0])
        } else {
            // Multiple items need separate import statements
            names_to_use
                .iter()
                .map(|n| format!("import {}::{};", import.module, n))
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
        if !["v", "sv", "svh"].contains(&ext) {
            return None;
        }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![format!("{}.v", module), format!("{}.sv", module)]
    }

    fn lang_key(&self) -> &'static str {
        "verilog"
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
        &["v", "sv", "svh"]
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
            .strip_suffix(".v")
            .or_else(|| entry_name.strip_suffix(".sv"))
            .or_else(|| entry_name.strip_suffix(".svh"))
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
            "class_declaration", "interface_declaration", "checker_declaration",
            "udp_declaration", "clocking_declaration", "property_declaration",
            "net_declaration", "data_declaration", "parameter_declaration",
            "specparam_declaration", "type_declaration", "genvar_declaration",
            "modport_declaration", "constraint_declaration", "covergroup_declaration",
            "sequence_declaration", "let_declaration", "local_parameter_declaration",
            "overload_declaration", "interface_class_declaration", "anonymous_program",
            "package_declaration",
            // Module/interface headers
            "module_ansi_header", "module_nonansi_header", "module_header",
            "interface_ansi_header", "interface_nonansi_header",
            // Items
            "module_or_generate_item", "interface_or_generate_item", "class_item",
            "interface_class_item", "checker_or_generate_item_declaration",
            "tf_item_declaration", "block_item_declaration", "anonymous_program_item",
            // Identifiers
            "simple_identifier", "function_identifier", "task_identifier",
            "interface_identifier", "class_identifier", "package_identifier",
            "property_identifier", "checker_identifier",
            "covergroup_identifier", "cover_point_identifier", "constraint_identifier",
            "clocking_identifier", "modport_identifier", "generate_block_identifier",
            "specparam_identifier", "terminal_identifier", "port_identifier",
            "input_identifier", "output_port_identifier", "index_variable_identifier",
            "interface_instance_identifier", "hierarchical_btf_identifier",
            "ps_identifier", "system_tf_identifier",
            "genvar_identifier",
            "instance_identifier", "output_identifier", "tf_identifier",
            "enum_identifier", "member_identifier", "parameter_identifier",
            "dynamic_array_variable_identifier", "inout_port_identifier",
            "input_port_identifier", "program_identifier", "cross_identifier",
            "method_identifier", "formal_port_identifier", "const_identifier",
            "c_identifier", "escaped_identifier", "text_macro_identifier",
            // Expressions
            "expression", "conditional_expression", "inside_expression",
            "let_expression", "range_expression", "array_range_expression",
            "case_expression", "expression_or_dist", "data_source_expression",
            "constant_expression", "constant_mintypmax_expression", "constant_param_expression",
            "mintypmax_expression", "module_path_expression", "module_path_mintypmax_expression",
            "clockvar_expression", "inc_or_dec_expression", "assignment_pattern_expression",
            "event_expression", "param_expression", "select_expression", "bins_expression",
            "tagged_union_expression", "cycle_delay_const_range_expression",
            // Statements
            "statement_or_null", "seq_block", "wait_statement", "jump_statement",
            "randcase_statement", "action_block", "statement_item", "par_block",
            "procedural_timing_control_statement", "statement", "disable_statement",
            "function_statement", "function_statement_or_null",
            // Assertions
            "assert_property_statement", "assume_property_statement",
            "restrict_property_statement", "expect_property_statement",
            "deferred_immediate_assert_statement", "deferred_immediate_assume_statement",
            "deferred_immediate_cover_statement", "simple_immediate_assert_statement",
            "simple_immediate_assume_statement", "simple_immediate_cover_statement",
            "cover_property_statement", "cover_sequence_statement",
            "assertion_variable_declaration", "concurrent_assertion_item",
            "deferred_immediate_assertion_item",
            // Case
            "case_item", "case_keyword", "case_pattern_item", "case_inside_item",
            "case_generate_item", "case_item_expression", "property_case_item",
            // Generate constructs
            "loop_generate_construct", "generate_block", "generate_region",
            "genvar_iteration", "genvar_initialization",
            // Types
            "class_type", "integer_vector_type", "struct_union", "struct_union_member",
            "casting_type", "data_type_or_implicit1", "implicit_data_type1",
            "integer_atom_type", "type_reference", "net_type", "net_type_declaration",
            "data_type", "non_integer_type", "data_type_or_void", "enum_base_type",
            "net_port_type1", "interface_class_type",
            // Class/method
            "class_method", "class_property", "class_item_qualifier",
            "class_constructor_prototype", "method_qualifier", "method_call",
            "method_call_body", "sequence_method_call", "array_method_name",
            "class_scope", "class_constructor_declaration", "interface_class_method",
            "class_qualifier", "class_new", "implicit_class_handle", "random_qualifier",
            // Functions/tasks
            "function_body_declaration", "function_prototype", "task_prototype",
            "tf_port_declaration", "extern_tf_declaration", "function_subroutine_call",
            "dpi_function_proto", "task_body_declaration", "function_data_type_or_implicit1",
            "dpi_function_import_property", "dpi_task_proto", "tf_call", "tf_port_list",
            "tf_port_item1", "tf_port_direction",
            // Ports
            "input_declaration", "output_declaration", "inout_declaration",
            "interface_port_declaration", "interface_port_header", "port_direction",
            "port_declaration", "list_of_port_declarations", "ansi_port_declaration",
            // Parameters and lists
            "parameter_port_declaration", "list_of_tf_variable_identifiers",
            "list_of_variable_identifiers", "list_of_port_identifiers",
            "list_of_variable_port_identifiers", "list_of_interface_identifiers",
            "list_of_type_assignments", "list_of_formal_arguments",
            "list_of_path_delay_expressions", "identifier_list", "list_of_genvar_identifiers",
            "list_of_arguments", "list_of_arguments_parent", "list_of_port_connections",
            "list_of_parameter_assignments", "list_of_clocking_decl_assign",
            "list_of_cross_items", "list_of_defparam_assignments", "list_of_net_assignments",
            "list_of_net_decl_assignments", "list_of_param_assignments",
            "list_of_specparam_assignments", "list_of_udp_port_identifiers",
            "list_of_variable_assignments", "list_of_variable_decl_assignments",
            "list_of_path_inputs", "list_of_path_outputs", "variable_identifier_list",
            // UDP
            "udp_ansi_declaration", "udp_reg_declaration", "udp_input_declaration",
            "sequential_entry", "combinational_entry", "combinational_body",
            "udp_initial_statement", "udp_declaration_port_list", "udp_output_declaration",
            "udp_nonansi_declaration", "udp_port_declaration", "sequential_body",
            "udp_port_list", "udp_instance", "udp_instantiation",
            // Constraints
            "constraint_expression", "constraint_prototype_qualifier",
            "extern_constraint_declaration", "solve_before_list", "constraint_block_item",
            "constraint_block", "constraint_prototype", "constraint_primary",
            "constraint_set", "uniqueness_constraint", "dist_list", "dist_item", "dist_weight",
            // Specify
            "simple_path_declaration", "edge_sensitive_path_declaration",
            "state_dependent_path_declaration", "specify_input_terminal_descriptor",
            "specify_output_terminal_descriptor", "showcancelled_declaration",
            "edge_control_specifier", "edge_identifier", "specify_block",
            "pulsestyle_declaration", "path_delay_expression", "path_declaration",
            "parallel_path_description", "full_path_description",
            "parallel_edge_sensitive_path_description", "full_edge_sensitive_path_description",
            "path_delay_value",
            // Operators
            "stream_operator", "assignment_operator", "operator_assignment",
            "inc_or_dec_operator", "polarity_operator", "overload_operator",
            "overload_proto_formals", "unary_operator",
            // DPI
            "dpi_import_export", "dpi_task_import_property", "import_export", "dpi_spec_string",
            // Assignments
            "nonblocking_assignment", "for_step", "for_variable_declaration",
            "loop_variables1", "ref_declaration", "blocking_assignment",
            "for_initialization", "variable_assignment", "net_assignment",
            "net_decl_assignment", "variable_decl_assignment", "param_assignment",
            "specparam_assignment", "defparam_assignment", "type_assignment",
            "assignment_pattern", "assignment_pattern_key",
            "assignment_pattern_net_lvalue", "assignment_pattern_variable_lvalue",
            "clocking_decl_assign",
            // Concatenations
            "module_path_concatenation", "module_path_multiple_concatenation",
            "concatenation", "constant_concatenation", "constant_multiple_concatenation",
            "multiple_concatenation", "streaming_concatenation", "stream_concatenation",
            "stream_expression", "slice_size", "empty_unpacked_array_concatenation",
            // Interface/modport
            "interface_item", "interface_instantiation", "modport_clocking_declaration",
            "modport_item", "modport_ports_declaration", "modport_simple_ports_declaration",
            "modport_simple_port", "modport_tf_ports_declaration",
            // Module instantiation
            "module_instantiation", "module_keyword", "hierarchical_instance",
            "name_of_instance", "parameter_value_assignment",
            "ordered_parameter_assignment", "named_parameter_assignment",
            "ordered_port_connection", "named_port_connection",
            "program_instantiation", "checker_instantiation",
            // Lifetime
            "lifetime",
            // Block
            "block_event_expression", "join_keyword",
            // Program
            "program_declaration",
            // Coverage
            "cross_body", "cross_body_item", "cover_cross", "cover_point",
            "coverage_event", "coverage_option", "coverage_spec_or_option",
            "bins_or_empty", "bins_or_options", "bins_keyword",
            "bins_selection", "bins_selection_or_option", "select_condition",
            "covergroup_range_list", "covergroup_value_range",
            "trans_list", "trans_set", "trans_range_list", "trans_item", "repeat_range",
            // Cycle delay
            "cycle_delay", "cycle_delay_range",
            // Gate types
            "n_input_gatetype", "n_output_gatetype", "enable_gatetype", "cmos_switchtype",
            "gate_instantiation", "cmos_switch_instance", "enable_gate_instance",
            "mos_switch_instance", "n_input_gate_instance", "n_output_gate_instance",
            "pass_switch_instance", "pass_enable_switch_instance", "pull_gate_instance",
            "pulldown_strength", "pullup_strength", "enable_terminal", "inout_terminal",
            "input_terminal", "output_terminal", "ncontrol_terminal", "pcontrol_terminal",
            // Compiler directives
            "default_nettype_compiler_directive", "timeunits_declaration",
            "text_macro_definition",
            // Clocking
            "clocking_event", "clocking_item", "clocking_direction",
            "default_skew", "clocking_skew", "clocking_drive", "clockvar",
            // Delay/timing
            "delay3", "delay2", "delay_value", "delay_control", "delay_or_event_control",
            "event_control", "event_trigger",
            // Strength
            "drive_strength", "strength0", "strength1", "charge_strength",
            // Dimensions
            "unpacked_dimension", "packed_dimension", "associative_dimension",
            "queue_dimension", "unsized_dimension",
            // Selects and ranges
            "constant_range", "constant_indexed_range", "indexed_range",
            "constant_primary", "module_path_primary", "primary", "primary_literal",
            "bit_select1", "select1", "nonrange_select1", "constant_bit_select1",
            "constant_select1",
            // Randcase
            "randcase_item",
            // Always/initial
            "always_construct", "always_keyword", "initial_construct", "final_construct",
            // Continuous assign
            "continuous_assign", "net_alias", "procedural_continuous_assignment",
            // Loop
            "loop_variables1", "open_range_list", "open_value_range",
            // Pattern
            "pattern", "cond_pattern", "cond_predicate", "unique_priority",
            // Enum
            "enum_name_declaration",
            // Formal/args
            "formal_argument", "let_port_list", "let_port_item", "let_list_of_arguments",
            "let_actual_arg",
            // Property/sequence
            "property_port_list", "property_port_item", "property_lvar_port_direction",
            "property_spec", "property_expr", "sequence_port_list", "sequence_port_item",
            "sequence_lvar_port_direction", "sequence_expr", "sequence_instance",
            "sequence_list_of_arguments", "sequence_abbrev",
            "consecutive_repetition", "non_consecutive_repetition", "goto_repetition",
            // Lvalues
            "net_lvalue", "variable_lvalue", "nonrange_variable_lvalue",
            // Subroutines
            "subroutine_call", "system_tf_call", "array_manipulation_call", "randomize_call",
            // Literals
            "time_literal", "time_unit", "string_literal", "integral_number",
            "decimal_number", "real_number", "unbased_unsized_literal",
            // Cast
            "cast", "dynamic_array_new",
            // Package
            "package_export_declaration", "package_scope", "package_import_item",
            // Attributes
            "attribute_instance", "attr_spec",
            // Specparam
            "pulse_control_specparam", "error_limit_value", "reject_limit_value", "limit_value",
            // Timing checks
            "timing_check_event", "timing_check_event_control", "timing_check_condition",
            "timing_check_limit", "controlled_reference_event", "data_event",
            "delayed_data", "delayed_reference", "end_edge_offset", "event_based_flag",
            "reference_event", "remain_active_flag", "timestamp_condition",
            "start_edge_offset", "threshold", "scalar_timing_check_condition",
            "scalar_constant", "timecheck_condition", "edge_descriptor",
            // System timing checks
            "$setup_timing_check", "$hold_timing_check", "$setuphold_timing_check",
            "$recovery_timing_check", "$removal_timing_check", "$recrem_timing_check",
            "$skew_timing_check", "$timeskew_timing_check", "$fullskew_timing_check",
            "$period_timing_check", "$width_timing_check", "$nochange_timing_check",
            // Level/edge
            "level_input_list", "edge_input_list", "edge_indicator", "next_state", "init_val",
            // Others
            "pass_switchtype", "pass_en_switchtype", "mos_switchtype",
            "default_nettype_value",
            "property_formal_type1", "sequence_formal_type1", "let_formal_type1",
            "package_or_generate_item_declaration", "notifier",
            "ps_or_hierarchical_array_identifier", "value_range",
        ];
        validate_unused_kinds_audit(&Verilog, documented_unused)
            .expect("Verilog unused node kinds audit failed");
    }
}
