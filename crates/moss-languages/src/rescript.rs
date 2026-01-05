//! ReScript language support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// ReScript language support.
pub struct ReScript;

impl Language for ReScript {
    fn name(&self) -> &'static str {
        "ReScript"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["res", "resi"]
    }
    fn grammar_name(&self) -> &'static str {
        "rescript"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["module_declaration"]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["let_binding", "external_declaration"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["type_declaration"]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["open_statement"]
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["let_binding", "type_declaration", "module_declaration"]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::AllPublic
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        match node.kind() {
            "let_binding" | "external_declaration" => {
                if let Some(name) = self.node_name(node, content) {
                    return vec![Export {
                        name: name.to_string(),
                        kind: SymbolKind::Function,
                        line: node.start_position().row + 1,
                    }];
                }
            }
            "type_declaration" => {
                if let Some(name) = self.node_name(node, content) {
                    return vec![Export {
                        name: name.to_string(),
                        kind: SymbolKind::Type,
                        line: node.start_position().row + 1,
                    }];
                }
            }
            "module_declaration" => {
                if let Some(name) = self.node_name(node, content) {
                    return vec![Export {
                        name: name.to_string(),
                        kind: SymbolKind::Module,
                        line: node.start_position().row + 1,
                    }];
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["let_binding", "module_declaration", "block"]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &["if_expression", "switch_expression"]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &["if_expression", "switch_expression", "switch_match"]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &[
            "if_expression",
            "switch_expression",
            "block",
            "module_declaration",
        ]
    }

    fn signature_suffix(&self) -> &'static str {
        ""
    }

    fn extract_function(&self, node: &Node, content: &str, _in_container: bool) -> Option<Symbol> {
        match node.kind() {
            "let_binding" | "external_declaration" => {
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

    fn extract_type(&self, node: &Node, content: &str) -> Option<Symbol> {
        if node.kind() != "type_declaration" {
            return None;
        }

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

    fn extract_docstring(&self, _node: &Node, _content: &str) -> Option<String> {
        None
    }

    fn extract_attributes(&self, _node: &Node, _content: &str) -> Vec<String> {
        Vec::new()
    }

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        if node.kind() != "open_statement" {
            return Vec::new();
        }

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

    fn format_import(&self, import: &Import, _names: Option<&[&str]>) -> String {
        // ReScript: open Module
        format!("open {}", import.module)
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
        node.child_by_field_name("name")
            .map(|n| &content[n.byte_range()])
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        if !["res", "resi"].contains(&ext) {
            return None;
        }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![format!("{}.res", module), format!("{}.resi", module)]
    }

    fn lang_key(&self) -> &'static str {
        "rescript"
    }

    fn is_stdlib_import(&self, import_name: &str, _project_root: &Path) -> bool {
        import_name.starts_with("Belt")
            || import_name.starts_with("Js.")
            || import_name == "Array"
            || import_name == "List"
            || import_name == "Option"
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
        &["res", "resi"]
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
            .strip_suffix(".res")
            .or_else(|| entry_name.strip_suffix(".resi"))
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
            // Expression nodes
            "try_expression", "ternary_expression", "while_expression", "for_expression",
            "call_expression", "pipe_expression", "sequence_expression", "await_expression",
            "coercion_expression", "lazy_expression", "assert_expression",
            "parenthesized_expression", "unary_expression", "binary_expression",
            "subscript_expression", "member_expression", "mutation_expression",
            "extension_expression",
            // Type nodes
            "type_identifier", "type_identifier_path", "unit_type", "generic_type",
            "function_type", "polyvar_type", "polymorphic_type", "tuple_type",
            "record_type", "record_type_field", "object_type", "variant_type",
            "abstract_type", "type_arguments", "type_parameters", "type_constraint",
            "type_annotation", "type_binding", "type_spread", "constrain_type",
            "as_aliasing_type", "function_type_parameters",
            // Module nodes
            "parenthesized_module_expression", "module_type_constraint", "module_type_annotation",
            "module_type_of", "constrain_module", "module_identifier", "module_identifier_path",
            "module_pack", "module_unpack", "module_binding",
            // Declaration nodes
            "let_declaration", "exception_declaration", "variant_declaration",
            "polyvar_declaration", "include_statement",
            // JSX
            "jsx_expression", "jsx_identifier", "nested_jsx_identifier",
            // Pattern matching
            "exception_pattern", "polyvar_type_pattern",
            // Identifiers
            "value_identifier", "value_identifier_path", "variant_identifier",
            "nested_variant_identifier", "polyvar_identifier", "property_identifier",
            "extension_identifier", "decorator_identifier",
            // Clauses
            "else_clause", "else_if_clause",
            // Other
            "function", "expression_statement", "formal_parameters",
        ];
        validate_unused_kinds_audit(&ReScript, documented_unused)
            .expect("ReScript unused node kinds audit failed");
    }
}
