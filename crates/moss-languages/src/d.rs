//! D language support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// D language support.
pub struct D;

impl Language for D {
    fn name(&self) -> &'static str {
        "D"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["d", "di"]
    }
    fn grammar_name(&self) -> &'static str {
        "d"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &[
            "module_declaration",
            "class_declaration",
            "struct_declaration",
            "interface_declaration",
        ]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_literal", "auto_declaration"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &[
            "alias_declaration",
            "enum_declaration",
            "class_declaration",
            "struct_declaration",
        ]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["import_declaration"]
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &[
            "module_declaration",
            "class_declaration",
            "struct_declaration",
            "auto_declaration",
        ]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::AccessModifier
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        match node.kind() {
            "module_declaration" => {
                if let Some(name) = self.node_name(node, content) {
                    return vec![Export {
                        name: name.to_string(),
                        kind: SymbolKind::Module,
                        line: node.start_position().row + 1,
                    }];
                }
            }
            "class_declaration" | "struct_declaration" | "interface_declaration" => {
                if self.is_public(node, content) {
                    if let Some(name) = self.node_name(node, content) {
                        return vec![Export {
                            name: name.to_string(),
                            kind: SymbolKind::Class,
                            line: node.start_position().row + 1,
                        }];
                    }
                }
            }
            "auto_declaration" | "function_literal" => {
                if self.is_public(node, content) {
                    if let Some(name) = self.node_name(node, content) {
                        return vec![Export {
                            name: name.to_string(),
                            kind: SymbolKind::Function,
                            line: node.start_position().row + 1,
                        }];
                    }
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &[
            "function_literal",
            "class_declaration",
            "struct_declaration",
            "block_statement",
        ]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &[
            "if_statement",
            "switch_statement",
            "while_statement",
            "for_statement",
            "foreach_statement",
        ]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &[
            "if_statement",
            "switch_statement",
            "while_statement",
            "for_statement",
            "foreach_statement",
            "catch",
        ]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &[
            "if_statement",
            "switch_statement",
            "while_statement",
            "for_statement",
            "class_declaration",
        ]
    }

    fn signature_suffix(&self) -> &'static str {
        " {}"
    }

    fn extract_function(&self, node: &Node, content: &str, _in_container: bool) -> Option<Symbol> {
        match node.kind() {
            "function_literal" | "auto_declaration" => {
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
            "module_declaration" => {
                let name = self.node_name(node, content)?;
                Some(Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::Module,
                    signature: format!("module {}", name),
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
            "class_declaration" | "struct_declaration" | "interface_declaration" => {
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
            "alias_declaration" | "enum_declaration" => {
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
        if node.kind() != "import_declaration" {
            return Vec::new();
        }

        let text = &content[node.byte_range()];
        vec![Import {
            module: text.trim().to_string(),
            names: Vec::new(),
            alias: None,
            is_wildcard: text.contains(':'),
            is_relative: false,
            line: node.start_position().row + 1,
        }]
    }

    fn format_import(&self, import: &Import, names: Option<&[&str]>) -> String {
        // D: import module; or import module : a, b, c;
        let names_to_use: Vec<&str> = names
            .map(|n| n.to_vec())
            .unwrap_or_else(|| import.names.iter().map(|s| s.as_str()).collect());
        if names_to_use.is_empty() {
            format!("import {};", import.module)
        } else {
            format!("import {} : {};", import.module, names_to_use.join(", "))
        }
    }

    fn is_public(&self, node: &Node, content: &str) -> bool {
        let text = &content[node.byte_range()];
        text.starts_with("public ") || !text.starts_with("private ")
    }

    fn get_visibility(&self, node: &Node, content: &str) -> Visibility {
        let text = &content[node.byte_range()];
        if text.starts_with("private ") {
            Visibility::Private
        } else if text.starts_with("protected ") {
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
        if let Some(name_node) = node.child_by_field_name("name") {
            return Some(&content[name_node.byte_range()]);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" {
                return Some(&content[child.byte_range()]);
            }
        }
        None
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        if !["d", "di"].contains(&ext) {
            return None;
        }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        let path = module.replace('.', "/");
        vec![format!("{}.d", path), format!("{}/package.d", path)]
    }

    fn lang_key(&self) -> &'static str {
        "d"
    }

    fn is_stdlib_import(&self, import_name: &str, _project_root: &Path) -> bool {
        import_name.starts_with("std.") || import_name.starts_with("core.")
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
        &["d", "di"]
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
            .strip_suffix(".d")
            .or_else(|| entry_name.strip_suffix(".di"))
            .unwrap_or(entry_name)
            .to_string()
    }

    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() {
            return Some(path.to_path_buf());
        }
        let package_d = path.join("package.d");
        if package_d.is_file() {
            return Some(package_d);
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
            // Expressions
            "add_expression", "and_and_expression", "and_expression", "assign_expression",
            "assert_expression", "cat_expression", "cast_expression", "comma_expression",
            "complement_expression", "conditional_expression", "delete_expression", "equal_expression",
            "expression", "identity_expression", "import_expression", "in_expression",
            "index_expression", "is_expression", "key_expression", "lwr_expression",
            "mixin_expression", "mul_expression", "new_anon_class_expression", "new_expression",
            "or_expression", "or_or_expression", "postfix_expression", "pow_expression",
            "primary_expression", "qualified_identifier", "rel_expression", "shift_expression",
            "slice_expression", "traits_expression", "typeid_expression", "unary_expression",
            "upr_expression", "value_expression", "xor_expression",
            // Statements
            "asm_statement", "break_statement", "case_range_statement", "case_statement",
            "conditional_statement", "continue_statement", "declaration_statement", "default_statement",
            "do_statement", "empty_statement", "expression_statement", "final_switch_statement",
            "foreach_range_statement", "goto_statement", "labeled_statement", "mixin_statement",
            "out_statement", "pragma_statement", "return_statement", "scope_block_statement",
            "scope_guard_statement", "scope_statement_list", "statement_list",
            "statement_list_no_case_no_default", "static_foreach_statement", "synchronized_statement",
            "then_statement", "throw_statement", "try_statement", "with_statement",
            // Declarations
            "anonymous_enum_declaration", "anonymous_enum_member",
            "anonymous_enum_members", "anon_struct_declaration", "anon_union_declaration",
            "auto_func_declaration", "class_template_declaration",
            "conditional_declaration", "debug_specification", "destructor", "empty_declaration",
            "enum_body", "enum_member", "enum_member_attribute", "enum_member_attributes",
            "enum_members", "func_declaration", "interface_template_declaration", "mixin_declaration",
            "module", "shared_static_constructor", "shared_static_destructor", "static_constructor",
            "static_destructor", "static_foreach_declaration", "struct_template_declaration",
            "template_declaration", "template_mixin_declaration", "union_declaration",
            "union_template_declaration", "var_declarations", "version_specification",
            // Foreach-related
            "aggregate_foreach", "foreach", "foreach_aggregate", "foreach_type",
            "foreach_type_attribute", "foreach_type_attributes", "foreach_type_list",
            "range_foreach", "static_foreach",
            // Function-related
            "constructor_args", "constructor_template", "function_attribute_kwd",
            "function_attributes", "function_contracts", "function_literal_body",
            "function_literal_body2", "member_function_attribute", "member_function_attributes",
            "missing_function_body", "out_contract_expression", "in_contract_expression",
            "in_statement", "parameter_with_attributes", "parameter_with_member_attributes",
            "shortened_function_body", "specified_function_body",
            // Template-related
            "template_type_parameter", "template_type_parameter_default",
            "template_type_parameter_specialization", "type_specialization",
            // Type-related
            "aggregate_body", "basic_type", "catch_parameter", "catches", "constructor",
            "else_statement", "enum_base_type", "finally_statement", "fundamental_type",
            "if_condition", "interfaces", "linkage_type", "module_alias_identifier",
            "module_attributes", "module_fully_qualified_name", "module_name", "mixin_type",
            "mixin_qualified_identifier", "storage_class", "storage_classes", "type",
            "type_ctor", "type_ctors", "type_suffix", "type_suffixes", "typeof", "interface",
            // Import-related
            "import", "import_bind", "import_bind_list", "import_bindings", "import_list",
            // ASM-related
            "asm_instruction", "asm_instruction_list", "asm_shift_exp", "asm_type_prefix",
            "gcc_asm_instruction_list", "gcc_asm_statement", "gcc_basic_asm_instruction",
            "gcc_ext_asm_instruction", "gcc_goto_asm_instruction",
            // Misc
            "alt_declarator_identifier", "base_class_list", "base_interface_list",
            "block_comment", "declaration_block", "declarator_identifier_list", "dot_identifier",
            "identifier", "nesting_block_comment", "static_if_condition", "struct_initializer",
            "struct_member_initializer", "struct_member_initializers", "super_class_or_interface",
            "traits_arguments", "traits_keyword", "var_declarator_identifier", "vector_base_type",
            "attribute_specifier",
        ];
        validate_unused_kinds_audit(&D, documented_unused)
            .expect("D unused node kinds audit failed");
    }
}
