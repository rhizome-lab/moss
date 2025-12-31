//! HLSL (High-Level Shading Language) support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// HLSL language support.
pub struct Hlsl;

impl Language for Hlsl {
    fn name(&self) -> &'static str {
        "HLSL"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["hlsl", "hlsli", "fx", "fxh", "cginc"]
    }
    fn grammar_name(&self) -> &'static str {
        "hlsl"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["struct_specifier", "cbuffer_specifier"]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_definition"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["struct_specifier"]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["preproc_include"]
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["function_definition", "struct_specifier"]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::AllPublic
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        let name = match self.node_name(node, content) {
            Some(n) => n.to_string(),
            None => return Vec::new(),
        };

        let kind = match node.kind() {
            "function_definition" => SymbolKind::Function,
            "struct_specifier" => SymbolKind::Struct,
            _ => return Vec::new(),
        };

        vec![Export {
            name,
            kind,
            line: node.start_position().row + 1,
        }]
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["function_definition", "compound_statement"]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &[
            "if_statement",
            "for_statement",
            "while_statement",
            "switch_statement",
        ]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &[
            "if_statement",
            "for_statement",
            "while_statement",
            "switch_statement",
            "case_statement",
            "conditional_expression",
        ]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &[
            "function_definition",
            "if_statement",
            "for_statement",
            "while_statement",
        ]
    }

    fn signature_suffix(&self) -> &'static str {
        ""
    }

    fn extract_function(&self, node: &Node, content: &str, _in_container: bool) -> Option<Symbol> {
        let name = self.node_name(node, content)?;
        let text = &content[node.byte_range()];
        let first_line = text.lines().next().unwrap_or(text);

        Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Function,
            signature: first_line.trim().to_string(),
            docstring: self.extract_docstring(node, content),
            attributes: Vec::new(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
            children: Vec::new(),
            is_interface_impl: false,
        })
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        let name = self.node_name(node, content)?;

        let kind = match node.kind() {
            "struct_specifier" => SymbolKind::Struct,
            "cbuffer_specifier" => SymbolKind::Module,
            _ => return None,
        };

        let text = &content[node.byte_range()];
        let first_line = text.lines().next().unwrap_or(text);

        Some(Symbol {
            name: name.to_string(),
            kind,
            signature: first_line.trim().to_string(),
            docstring: self.extract_docstring(node, content),
            attributes: Vec::new(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
            children: Vec::new(),
            is_interface_impl: false,
        })
    }

    fn extract_type(&self, node: &Node, content: &str) -> Option<Symbol> {
        if node.kind() != "struct_specifier" {
            return None;
        }
        self.extract_container(node, content)
    }

    fn extract_docstring(&self, node: &Node, content: &str) -> Option<String> {
        // HLSL uses C-style comments
        let mut prev = node.prev_sibling();
        while let Some(sibling) = prev {
            let text = &content[sibling.byte_range()];
            if sibling.kind() == "comment" {
                if text.starts_with("/*") {
                    let inner = text.trim_start_matches("/*").trim_end_matches("*/").trim();
                    if !inner.is_empty() {
                        return Some(inner.lines().next().unwrap_or(inner).to_string());
                    }
                } else if text.starts_with("//") {
                    let line = text.strip_prefix("//").unwrap_or(text).trim();
                    return Some(line.to_string());
                }
            }
            prev = sibling.prev_sibling();
        }
        None
    }

    fn extract_attributes(&self, _node: &Node, _content: &str) -> Vec<String> {
        Vec::new()
    }

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        if node.kind() != "preproc_include" {
            return Vec::new();
        }

        let text = &content[node.byte_range()];
        let line = node.start_position().row + 1;

        // #include "file.hlsl" or #include <file.hlsl>
        let module = text
            .split('"')
            .nth(1)
            .or_else(|| text.split('<').nth(1).and_then(|s| s.split('>').next()))
            .map(|s| s.to_string());

        if let Some(module) = module {
            return vec![Import {
                module,
                names: Vec::new(),
                alias: None,
                is_wildcard: false,
                is_relative: text.contains('"'),
                line,
            }];
        }

        Vec::new()
    }

    fn format_import(&self, import: &Import, _names: Option<&[&str]>) -> String {
        // HLSL: #include "file.hlsl" or #include <file.hlsl>
        if import.is_relative {
            format!("#include \"{}\"", import.module)
        } else {
            format!("#include <{}>", import.module)
        }
    }

    fn is_public(&self, _node: &Node, _content: &str) -> bool {
        true
    }
    fn get_visibility(&self, _node: &Node, _content: &str) -> Visibility {
        Visibility::Public
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
        node.child_by_field_name("declarator")
            .and_then(|d| d.child_by_field_name("declarator"))
            .map(|n| &content[n.byte_range()])
            .or_else(|| {
                node.child_by_field_name("name")
                    .map(|n| &content[n.byte_range()])
            })
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        if !["hlsl", "hlsli", "fx", "fxh", "cginc"].contains(&ext) {
            return None;
        }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![
            format!("{}.hlsl", module),
            format!("{}.hlsli", module),
            format!("{}.fx", module),
        ]
    }

    fn lang_key(&self) -> &'static str {
        "hlsl"
    }

    fn is_stdlib_import(&self, _: &str, _: &Path) -> bool {
        false
    }
    fn find_stdlib(&self, _: &Path) -> Option<PathBuf> {
        None
    }
    fn resolve_local_import(&self, import: &str, current_file: &Path, _: &Path) -> Option<PathBuf> {
        let dir = current_file.parent()?;
        let full = dir.join(import);
        if full.is_file() {
            Some(full)
        } else {
            None
        }
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
        &["hlsl", "hlsli", "fx", "fxh", "cginc"]
    }
    fn package_sources(&self, _: &Path) -> Vec<crate::PackageSource> {
        Vec::new()
    }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{has_extension, skip_dotfiles};
        if skip_dotfiles(name) {
            return true;
        }
        !is_dir && !has_extension(name, &["hlsl", "hlsli", "fx", "fxh", "cginc"])
    }

    fn discover_packages(&self, _: &crate::PackageSource) -> Vec<(String, PathBuf)> {
        Vec::new()
    }

    fn package_module_name(&self, entry_name: &str) -> String {
        for ext in &[".hlsl", ".hlsli", ".fx", ".fxh", ".cginc"] {
            if let Some(name) = entry_name.strip_suffix(ext) {
                return name.to_string();
            }
        }
        entry_name.to_string()
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
            "abstract_function_declarator", "access_specifier", "alias_declaration",
            "alignas_qualifier", "alignof_expression", "assignment_expression",
            "attribute_declaration", "attribute_specifier", "attributed_statement",
            "base_class_clause", "binary_expression", "bitfield_clause", "break_statement",
            "call_expression", "cast_expression", "catch_clause",
            "class_specifier", "co_await_expression", "co_return_statement", "co_yield_statement",
            "comma_expression", "compound_literal_expression", "concept_definition",
            "condition_clause", "consteval_block_declaration", "continue_statement",
            "declaration", "declaration_list", "decltype", "default_method_clause",
            "delete_expression", "delete_method_clause", "dependent_type", "destructor_name",
            "discard_statement", "do_statement", "else_clause", "enum_specifier", "enumerator",
            "enumerator_list", "expansion_statement", "explicit_function_specifier",
            "explicit_object_parameter_declaration", "export_declaration", "expression_statement",
            "extension_expression", "field_declaration", "field_declaration_list",
            "field_expression", "field_identifier", "fold_expression", "for_range_loop",
            "friend_declaration", "function_declarator", "generic_expression",
            "global_module_fragment_declaration", "gnu_asm_expression", "gnu_asm_qualifier",
            "goto_statement", "identifier", "import_declaration", "init_statement",
            "labeled_statement", "lambda_capture_initializer", "lambda_capture_specifier",
            "lambda_declarator", "lambda_default_capture", "lambda_expression", "lambda_specifier",
            "linkage_specification", "module_declaration", "module_name", "module_partition",
            "ms_based_modifier", "ms_call_modifier", "ms_declspec_modifier", "ms_pointer_modifier",
            "ms_restrict_modifier", "ms_signed_ptr_modifier", "ms_unaligned_ptr_modifier",
            "ms_unsigned_ptr_modifier", "namespace_alias_definition", "namespace_definition",
            "namespace_identifier", "nested_namespace_specifier", "new_expression", "noexcept",
            "offsetof_expression", "operator_cast", "operator_name", "optional_parameter_declaration",
            "optional_type_parameter_declaration", "parameter_declaration", "parenthesized_expression",
            "placeholder_type_specifier", "pointer_expression", "pointer_type_declarator",
            "preproc_elif", "preproc_elifdef", "preproc_else", "preproc_function_def",
            "preproc_if", "preproc_ifdef", "primitive_type", "private_module_fragment_declaration",
            "pure_virtual_clause", "qualified_identifier", "qualifiers", "ref_qualifier",
            "reflect_expression", "requires_clause", "requires_expression", "return_statement",
            "seh_except_clause", "seh_finally_clause", "seh_leave_statement", "seh_try_statement",
            "sized_type_specifier", "sizeof_expression", "splice_expression", "splice_specifier",
            "splice_type_specifier", "statement_identifier", "static_assert_declaration",
            "storage_class_specifier", "structured_binding_declarator", "subscript_expression",
            "template_declaration", "template_function", "template_method",
            "template_template_parameter_declaration", "template_type", "throw_specifier",
            "throw_statement", "trailing_return_type", "try_statement", "type_definition",
            "type_descriptor", "type_identifier", "type_parameter_declaration", "type_qualifier",
            "type_requirement", "unary_expression", "union_specifier", "update_expression",
            "using_declaration", "variadic_parameter_declaration",
            "variadic_type_parameter_declaration", "virtual_specifier",
        ];
        validate_unused_kinds_audit(&Hlsl, documented_unused)
            .expect("HLSL unused node kinds audit failed");
    }
}
