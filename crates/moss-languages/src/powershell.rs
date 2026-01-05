//! PowerShell language support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// PowerShell language support.
pub struct PowerShell;

impl Language for PowerShell {
    fn name(&self) -> &'static str {
        "PowerShell"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["ps1", "psm1", "psd1"]
    }
    fn grammar_name(&self) -> &'static str {
        "powershell"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["class_statement"]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_statement"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["class_statement", "enum_statement"]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["pipeline"] // Import-Module is a command in a pipeline
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["function_statement", "class_statement"]
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
            "function_statement" => SymbolKind::Function,
            "class_statement" => SymbolKind::Class,
            _ => return Vec::new(),
        };

        vec![Export {
            name,
            kind,
            line: node.start_position().row + 1,
        }]
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["function_statement", "class_statement", "script_block"]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &[
            "if_statement",
            "while_statement",
            "for_statement",
            "foreach_statement",
            "switch_statement",
            "try_statement",
        ]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &[
            "if_statement",
            "elseif_clause",
            "while_statement",
            "for_statement",
            "foreach_statement",
            "switch_statement",
            "catch_clause",
        ]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &[
            "function_statement",
            "class_statement",
            "if_statement",
            "while_statement",
            "for_statement",
            "try_statement",
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
        if node.kind() != "class_statement" {
            return None;
        }

        let name = self.node_name(node, content)?;
        let text = &content[node.byte_range()];
        let first_line = text.lines().next().unwrap_or(text);

        Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Class,
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
        let name = self.node_name(node, content)?;
        let kind = match node.kind() {
            "class_statement" => SymbolKind::Class,
            "enum_statement" => SymbolKind::Enum,
            _ => return None,
        };

        Some(Symbol {
            name: name.to_string(),
            kind,
            signature: format!("{} {}", node.kind().replace("_statement", ""), name),
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

    fn extract_docstring(&self, node: &Node, content: &str) -> Option<String> {
        // PowerShell uses <# #> for block comments
        let mut prev = node.prev_sibling();
        while let Some(sibling) = prev {
            let text = &content[sibling.byte_range()];
            if sibling.kind() == "comment" {
                if text.starts_with("<#") {
                    let inner = text.trim_start_matches("<#").trim_end_matches("#>").trim();
                    if !inner.is_empty() {
                        return Some(inner.lines().next().unwrap_or(inner).to_string());
                    }
                } else if text.starts_with('#') {
                    let line = text.strip_prefix('#').unwrap_or(text).trim();
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
        if node.kind() != "pipeline" {
            return Vec::new();
        }

        let text = &content[node.byte_range()];
        let line = node.start_position().row + 1;

        // Import-Module ModuleName
        if let Some(rest) = text.strip_prefix("Import-Module ") {
            let module = rest.split_whitespace().next().map(|s| s.to_string());
            if let Some(module) = module {
                return vec![Import {
                    module,
                    names: Vec::new(),
                    alias: None,
                    is_wildcard: true,
                    is_relative: false,
                    line,
                }];
            }
        }

        Vec::new()
    }

    fn format_import(&self, import: &Import, _names: Option<&[&str]>) -> String {
        // PowerShell: Import-Module or using module
        format!("Import-Module {}", import.module)
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
        if !["ps1", "psm1", "psd1"].contains(&ext) {
            return None;
        }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![format!("{}.psm1", module), format!("{}.ps1", module)]
    }

    fn lang_key(&self) -> &'static str {
        "powershell"
    }

    fn is_stdlib_import(&self, import_name: &str, _project_root: &Path) -> bool {
        matches!(
            import_name,
            "Microsoft.PowerShell.Core"
                | "Microsoft.PowerShell.Utility"
                | "Microsoft.PowerShell.Management"
                | "Microsoft.PowerShell.Security"
        )
    }

    fn find_stdlib(&self, _project_root: &Path) -> Option<PathBuf> {
        None
    }
    fn resolve_local_import(&self, import: &str, _: &Path, project_root: &Path) -> Option<PathBuf> {
        let full = project_root.join(format!("{}.psm1", import));
        if full.is_file() { Some(full) } else { None }
    }
    fn resolve_external_import(&self, _: &str, _: &Path) -> Option<ResolvedPackage> {
        None
    }

    fn get_version(&self, project_root: &Path) -> Option<String> {
        // Check for module manifest
        for entry in std::fs::read_dir(project_root).ok()? {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "psd1") {
                return Some("PowerShell Module".to_string());
            }
        }
        None
    }

    fn find_package_cache(&self, _: &Path) -> Option<PathBuf> {
        if let Some(home) = std::env::var_os("HOME") {
            let modules = PathBuf::from(home).join(".local/share/powershell/Modules");
            if modules.is_dir() {
                return Some(modules);
            }
        }
        None
    }

    fn indexable_extensions(&self) -> &'static [&'static str] {
        &["ps1", "psm1"]
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
            .strip_suffix(".psm1")
            .or_else(|| entry_name.strip_suffix(".ps1"))
            .or_else(|| entry_name.strip_suffix(".psd1"))
            .unwrap_or(entry_name)
            .to_string()
    }

    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() {
            return Some(path.to_path_buf());
        }
        // Look for .psm1 in directory
        if path.is_dir() {
            let name = path.file_name()?.to_str()?;
            let module = path.join(format!("{}.psm1", name));
            if module.is_file() {
                return Some(module);
            }
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
            "additive_argument_expression", "additive_expression", "argument_expression",
            "argument_expression_list", "array_expression", "array_literal_expression",
            "array_type_name", "assignement_operator", "assignment_expression",
            "bitwise_argument_expression", "bitwise_expression", "block_name", "cast_expression",
            "catch_clauses", "catch_type_list", "class_attribute", "class_method_definition",
            "class_method_parameter", "class_method_parameter_list", "class_property_definition",
            "command_invokation_operator", "comparison_argument_expression",
            "comparison_expression", "comparison_operator", "data_statement", "do_statement",
            "else_clause", "elseif_clauses", "empty_statement", "enum_member",
            "expression_with_unary_operator", "file_redirection_operator", "finally_clause",
            "flow_control_statement", "for_condition", "for_initializer", "for_iterator",
            "foreach_command", "foreach_parameter", "format_argument_expression",
            "format_expression", "format_operator", "function_name",
            "function_parameter_declaration", "generic_type_arguments", "generic_type_name",
            "hash_entry", "hash_literal_body", "hash_literal_expression",
            "inlinescript_statement", "invokation_expression", "invokation_foreach_expression",
            "key_expression", "label_expression", "left_assignment_expression",
            "logical_argument_expression", "logical_expression", "merging_redirection_operator",
            "multiplicative_argument_expression", "multiplicative_expression", "named_block",
            "named_block_list", "parallel_statement", "param_block", "parenthesized_expression",
            "post_decrement_expression", "post_increment_expression", "pre_decrement_expression",
            "pre_increment_expression", "range_argument_expression", "range_expression",
            "script_block_body", "script_block_expression", "sequence_statement",
            "statement_block", "statement_list", "sub_expression", "switch_body",
            "switch_clause", "switch_clause_condition", "switch_clauses", "trap_statement",
            "type_identifier", "type_literal", "type_name", "type_spec", "unary_expression",
            "while_condition",
        ];
        validate_unused_kinds_audit(&PowerShell, documented_unused)
            .expect("PowerShell unused node kinds audit failed");
    }
}
