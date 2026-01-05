//! GLSL (OpenGL Shading Language) support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// GLSL language support.
pub struct Glsl;

impl Language for Glsl {
    fn name(&self) -> &'static str {
        "GLSL"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["glsl", "vert", "frag", "geom", "comp", "tesc", "tese"]
    }
    fn grammar_name(&self) -> &'static str {
        "glsl"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["struct_specifier"]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_definition"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["struct_specifier"]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &[]
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
            implements: Vec::new(),
        })
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        if node.kind() != "struct_specifier" {
            return None;
        }

        let name = self.node_name(node, content)?;
        let text = &content[node.byte_range()];
        let first_line = text.lines().next().unwrap_or(text);

        Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Struct,
            signature: first_line.trim().to_string(),
            docstring: self.extract_docstring(node, content),
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

    fn extract_docstring(&self, node: &Node, content: &str) -> Option<String> {
        // GLSL uses C-style comments
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

    fn extract_imports(&self, _node: &Node, _content: &str) -> Vec<Import> {
        Vec::new()
    }

    fn format_import(&self, _import: &Import, _names: Option<&[&str]>) -> String {
        // GLSL has no standard import mechanism (uses #include via extensions)
        String::new()
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
        if !["glsl", "vert", "frag", "geom", "comp", "tesc", "tese"].contains(&ext) {
            return None;
        }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![
            format!("{}.glsl", module),
            format!("{}.vert", module),
            format!("{}.frag", module),
        ]
    }

    fn lang_key(&self) -> &'static str {
        "glsl"
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
        &["glsl", "vert", "frag", "geom", "comp", "tesc", "tese"]
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
        for ext in &[
            ".glsl", ".vert", ".frag", ".geom", ".comp", ".tesc", ".tese",
        ] {
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
            "abstract_function_declarator", "alignas_qualifier", "alignof_expression",
            "assignment_expression", "attribute_declaration", "attribute_specifier",
            "attributed_statement", "binary_expression", "bitfield_clause", "break_statement",
            "call_expression", "cast_expression", "comma_expression", "compound_literal_expression",
            "continue_statement", "declaration", "declaration_list", "do_statement",
            "else_clause", "enum_specifier", "enumerator", "enumerator_list",
            "expression_statement", "extension_expression", "extension_storage_class",
            "field_declaration", "field_declaration_list", "field_expression", "field_identifier",
            "function_declarator", "generic_expression", "gnu_asm_expression", "gnu_asm_qualifier",
            "goto_statement", "identifier", "labeled_statement", "layout_qualifiers",
            "layout_specification", "linkage_specification", "macro_type_specifier",
            "ms_based_modifier", "ms_call_modifier", "ms_declspec_modifier",
            "ms_pointer_modifier", "ms_restrict_modifier", "ms_signed_ptr_modifier",
            "ms_unaligned_ptr_modifier", "ms_unsigned_ptr_modifier", "offsetof_expression",
            "parameter_declaration", "parenthesized_expression", "pointer_expression",
            "preproc_elif", "preproc_elifdef", "preproc_else", "preproc_function_def",
            "preproc_if", "preproc_ifdef", "primitive_type", "qualifier", "return_statement",
            "seh_except_clause", "seh_finally_clause", "seh_leave_statement", "seh_try_statement",
            "sizeof_expression", "sized_type_specifier", "statement_identifier",
            "storage_class_specifier", "subscript_expression", "type_definition",
            "type_descriptor", "type_identifier", "type_qualifier", "unary_expression",
            "union_specifier", "update_expression",
        ];
        validate_unused_kinds_audit(&Glsl, documented_unused)
            .expect("GLSL unused node kinds audit failed");
    }
}
