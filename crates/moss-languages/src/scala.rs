//! Scala language support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// Scala language support.
pub struct Scala;

impl Language for Scala {
    fn name(&self) -> &'static str {
        "Scala"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["scala", "sc"]
    }
    fn grammar_name(&self) -> &'static str {
        "scala"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["class_definition", "object_definition", "trait_definition"]
    }
    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_definition"]
    }
    fn type_kinds(&self) -> &'static [&'static str] {
        &["class_definition", "trait_definition"]
    }
    fn import_kinds(&self) -> &'static [&'static str] {
        &["import_declaration"]
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &[
            "class_definition",
            "object_definition",
            "trait_definition",
            "function_definition",
        ]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::AccessModifier
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        // Scala: public by default, check for private/protected modifiers
        // TODO: implement proper visibility checking for Scala
        let name = match self.node_name(node, content) {
            Some(n) => n.to_string(),
            None => return Vec::new(),
        };

        let kind = match node.kind() {
            "class_definition" => SymbolKind::Class,
            "object_definition" => SymbolKind::Module,
            "trait_definition" => SymbolKind::Trait,
            "function_definition" => SymbolKind::Function,
            _ => return Vec::new(),
        };

        vec![Export {
            name,
            kind,
            line: node.start_position().row + 1,
        }]
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["for_expression", "block", "lambda_expression"]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &[
            "if_expression",
            "match_expression",
            "for_expression",
            "while_expression",
            "do_while_expression",
            "try_expression",
            "return_expression",
            "throw_expression",
        ]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &[
            "if_expression",
            "match_expression",
            "case_clause",
            "for_expression",
            "while_expression",
            "do_while_expression",
            "try_expression",
            "catch_clause",
            "infix_expression", // for && and ||
        ]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &[
            "if_expression",
            "match_expression",
            "for_expression",
            "while_expression",
            "do_while_expression",
            "try_expression",
            "function_definition",
            "class_definition",
            "object_definition",
            "trait_definition",
            "block",
        ]
    }

    fn signature_suffix(&self) -> &'static str {
        " {}"
    }

    fn extract_function(&self, node: &Node, content: &str, in_container: bool) -> Option<Symbol> {
        let name = self.node_name(node, content)?;
        let params = node
            .child_by_field_name("parameters")
            .map(|p| content[p.byte_range()].to_string())
            .unwrap_or_else(|| "()".to_string());
        let ret = node
            .child_by_field_name("return_type")
            .map(|r| format!(": {}", &content[r.byte_range()]))
            .unwrap_or_default();

        Some(Symbol {
            name: name.to_string(),
            kind: if in_container {
                SymbolKind::Method
            } else {
                SymbolKind::Function
            },
            signature: format!("def {}{}{}", name, params, ret),
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
        let (kind, keyword) = match node.kind() {
            "object_definition" => (SymbolKind::Module, "object"),
            "trait_definition" => (SymbolKind::Trait, "trait"),
            _ => (SymbolKind::Class, "class"),
        };

        Some(Symbol {
            name: name.to_string(),
            kind,
            signature: format!("{} {}", keyword, name),
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

    fn format_import(&self, import: &Import, names: Option<&[&str]>) -> String {
        // Scala: import pkg.Class or import pkg.{A, B, C}
        let names_to_use: Vec<&str> = names
            .map(|n| n.to_vec())
            .unwrap_or_else(|| import.names.iter().map(|s| s.as_str()).collect());
        if import.is_wildcard {
            format!("import {}._", import.module)
        } else if names_to_use.is_empty() {
            format!("import {}", import.module)
        } else if names_to_use.len() == 1 {
            format!("import {}.{}", import.module, names_to_use[0])
        } else {
            format!("import {}.{{{}}}", import.module, names_to_use.join(", "))
        }
    }

    fn is_public(&self, _node: &Node, _content: &str) -> bool {
        true
    }
    fn get_visibility(&self, _node: &Node, _content: &str) -> Visibility {
        Visibility::Public
    }

    fn is_test_symbol(&self, symbol: &crate::Symbol) -> bool {
        {
            let has_test_attr = symbol.attributes.iter().any(|a| a.contains("@Test"));
            if has_test_attr {
                return true;
            }
            match symbol.kind {
                crate::SymbolKind::Class => {
                    symbol.name.starts_with("Test") || symbol.name.ends_with("Test")
                }
                _ => false,
            }
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
        let ext = path.extension()?.to_str()?;
        if !["scala", "sc"].contains(&ext) {
            return None;
        }
        Some(path.to_string_lossy().to_string())
    }
    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![format!("{}.scala", module)]
    }

    fn lang_key(&self) -> &'static str {
        "scala"
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
        &["scala", "sc"]
    }
    fn find_stdlib(&self, _: &Path) -> Option<PathBuf> {
        None
    }
    fn package_module_name(&self, name: &str) -> String {
        name.strip_suffix(".scala").unwrap_or(name).to_string()
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
            "access_modifier", "access_qualifier", "arrow_renamed_identifier",
            "as_renamed_identifier", "block_comment", "case_block", "case_class_pattern",
            "class_parameter", "class_parameters", "derives_clause", "enum_body",
            "enum_case_definitions", "enum_definition", "enumerator", "enumerators",
            "export_declaration", "extends_clause", "extension_definition", "field_expression",
            "full_enum_case", "identifier", "identifiers", "indented_block", "indented_cases",
            "infix_modifier", "inline_modifier", "instance_expression", "into_modifier",
            "macro_body", "modifiers", "name_and_type", "opaque_modifier", "open_modifier",
            "operator_identifier", "package_clause", "package_identifier", "self_type",
            "simple_enum_case", "template_body", "tracked_modifier", "transparent_modifier",
            "val_declaration", "val_definition", "var_declaration", "var_definition",
            "with_template_body",
            // CLAUSE
            "finally_clause", "type_case_clause",
            // EXPRESSION
            "ascription_expression", "assignment_expression", "call_expression",
            "generic_function", "interpolated_string_expression", "parenthesized_expression",
            "postfix_expression", "prefix_expression", "quote_expression", "splice_expression",
            "tuple_expression",
            // TYPE
            "annotated_type", "applied_constructor_type", "compound_type",
            "contravariant_type_parameter", "covariant_type_parameter", "function_declaration",
            "function_type", "generic_type", "given_definition", "infix_type", "lazy_parameter_type",
            "literal_type", "match_type", "named_tuple_type", "parameter_types",
            "projected_type", "repeated_parameter_type", "singleton_type", "stable_identifier",
            "stable_type_identifier", "structural_type", "tuple_type", "type_arguments",
            "type_definition", "type_identifier", "type_lambda", "type_parameters", "typed_pattern",
        ];

        validate_unused_kinds_audit(&Scala, documented_unused)
            .expect("Scala unused node kinds audit failed");
    }
}
