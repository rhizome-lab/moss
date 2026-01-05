//! Perl language support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// Perl language support.
pub struct Perl;

impl Language for Perl {
    fn name(&self) -> &'static str {
        "Perl"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["pl", "pm", "t"]
    }
    fn grammar_name(&self) -> &'static str {
        "perl"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["package_statement"]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["subroutine_declaration_statement"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &[]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["use_statement", "require_expression"]
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["subroutine_declaration_statement"]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::NamingConvention // _ prefix for private
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        let name = match self.node_name(node, content) {
            Some(n) => n.to_string(),
            None => return Vec::new(),
        };

        // _ prefix is conventionally private
        if name.starts_with('_') {
            return Vec::new();
        }

        vec![Export {
            name,
            kind: SymbolKind::Function,
            line: node.start_position().row + 1,
        }]
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["block", "subroutine_declaration_statement"]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &[
            "conditional_statement",
            "loop_statement",
            "for_statement",
            "cstyle_for_statement",
        ]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &[
            "conditional_statement",
            "loop_statement",
            "for_statement",
            "conditional_expression",
        ]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &[
            "subroutine_declaration_statement",
            "conditional_statement",
            "loop_statement",
            "block",
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
            visibility: if name.starts_with('_') {
                Visibility::Private
            } else {
                Visibility::Public
            },
            children: Vec::new(),
            is_interface_impl: false,
            implements: Vec::new(),
        })
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        if node.kind() != "package_statement" {
            return None;
        }

        let text = &content[node.byte_range()];
        let name = text
            .strip_prefix("package ")
            .and_then(|s| s.split(';').next())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "main".to_string());

        Some(Symbol {
            name: name.clone(),
            kind: SymbolKind::Module,
            signature: format!("package {}", name),
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

    fn extract_docstring(&self, node: &Node, content: &str) -> Option<String> {
        // Perl uses # for comments, POD for docs
        let mut prev = node.prev_sibling();
        let mut doc_lines = Vec::new();

        while let Some(sibling) = prev {
            let text = &content[sibling.byte_range()];
            if sibling.kind() == "comment" && text.starts_with('#') {
                let line = text.strip_prefix('#').unwrap_or(text).trim();
                doc_lines.push(line.to_string());
                prev = sibling.prev_sibling();
            } else {
                break;
            }
        }

        if doc_lines.is_empty() {
            return None;
        }

        doc_lines.reverse();
        Some(doc_lines.join(" "))
    }

    fn extract_attributes(&self, _node: &Node, _content: &str) -> Vec<String> {
        Vec::new()
    }

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        let text = &content[node.byte_range()];
        let line = node.start_position().row + 1;

        // use Module::Name;
        // require Module::Name;
        let module = if let Some(rest) = text.strip_prefix("use ") {
            rest.split(|c| c == ';' || c == ' ').next()
        } else if let Some(rest) = text.strip_prefix("require ") {
            rest.split(|c| c == ';' || c == ' ').next()
        } else {
            None
        };

        if let Some(module) = module {
            let module = module.trim().to_string();
            return vec![Import {
                module: module.clone(),
                names: Vec::new(),
                alias: None,
                is_wildcard: false,
                is_relative: false,
                line,
            }];
        }

        Vec::new()
    }

    fn format_import(&self, import: &Import, names: Option<&[&str]>) -> String {
        // Perl: use Module; or use Module qw(a b c);
        let names_to_use: Vec<&str> = names
            .map(|n| n.to_vec())
            .unwrap_or_else(|| import.names.iter().map(|s| s.as_str()).collect());
        if names_to_use.is_empty() {
            format!("use {};", import.module)
        } else {
            format!("use {} qw({});", import.module, names_to_use.join(" "))
        }
    }

    fn is_public(&self, node: &Node, content: &str) -> bool {
        self.node_name(node, content)
            .map_or(true, |n| !n.starts_with('_'))
    }

    fn get_visibility(&self, node: &Node, content: &str) -> Visibility {
        if self.is_public(node, content) {
            Visibility::Public
        } else {
            Visibility::Private
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
        if !["pl", "pm"].contains(&ext) {
            return None;
        }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        let path = module.replace("::", "/");
        vec![format!("{}.pm", path), format!("{}.pl", path)]
    }

    fn lang_key(&self) -> &'static str {
        "perl"
    }

    fn is_stdlib_import(&self, import_name: &str, _project_root: &Path) -> bool {
        // Core Perl modules
        import_name == "strict"
            || import_name == "warnings"
            || import_name.starts_with("File::")
            || import_name.starts_with("IO::")
            || import_name.starts_with("Data::")
            || import_name.starts_with("Carp")
    }

    fn find_stdlib(&self, _project_root: &Path) -> Option<PathBuf> {
        None
    }

    fn resolve_local_import(
        &self,
        import: &str,
        _current_file: &Path,
        project_root: &Path,
    ) -> Option<PathBuf> {
        let path = import.replace("::", "/");
        let full = project_root.join("lib").join(format!("{}.pm", path));
        if full.is_file() { Some(full) } else { None }
    }

    fn resolve_external_import(
        &self,
        _import_name: &str,
        _project_root: &Path,
    ) -> Option<ResolvedPackage> {
        None
    }

    fn get_version(&self, project_root: &Path) -> Option<String> {
        if project_root.join("cpanfile").is_file() {
            return Some("cpan".to_string());
        }
        if project_root.join("Makefile.PL").is_file() {
            return Some("ExtUtils::MakeMaker".to_string());
        }
        None
    }

    fn find_package_cache(&self, _project_root: &Path) -> Option<PathBuf> {
        None
    }
    fn indexable_extensions(&self) -> &'static [&'static str] {
        &["pl", "pm"]
    }
    fn package_sources(&self, _project_root: &Path) -> Vec<crate::PackageSource> {
        Vec::new()
    }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{has_extension, skip_dotfiles};
        if skip_dotfiles(name) {
            return true;
        }
        if is_dir && name == "blib" {
            return true;
        }
        !is_dir && !has_extension(name, self.indexable_extensions())
    }

    fn discover_packages(&self, _source: &crate::PackageSource) -> Vec<(String, PathBuf)> {
        Vec::new()
    }

    fn package_module_name(&self, entry_name: &str) -> String {
        entry_name
            .strip_suffix(".pm")
            .or_else(|| entry_name.strip_suffix(".pl"))
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
            "ambiguous_function_call_expression", "amper_deref_expression",
            "anonymous_array_expression", "anonymous_hash_expression",
            "anonymous_method_expression", "anonymous_slice_expression",
            "anonymous_subroutine_expression", "array_deref_expression",
            "array_element_expression", "arraylen_deref_expression", "assignment_expression",
            "await_expression", "binary_expression", "block_statement", "class_phaser_statement",
            "class_statement", "coderef_call_expression",
            "defer_statement", "do_expression", "else", "elsif",
            "equality_expression", "eval_expression", "expression_statement",
            "fileglob_expression", "func0op_call_expression", "func1op_call_expression",
            "function", "function_call_expression", "glob_deref_expression",
            "glob_slot_expression", "goto_expression", "hash_deref_expression",
            "hash_element_expression", "identifier", "keyval_expression",
            "list_expression", "localization_expression",
            "loopex_expression", "lowprec_logical_expression", "map_grep_expression",
            "match_regexp", "match_regexp_modifiers", "method", "method_call_expression",
            "method_declaration_statement", "phaser_statement", "postfix_conditional_expression",
            "postfix_for_expression", "postfix_loop_expression", "postinc_expression",
            "preinc_expression", "prototype", "quoted_regexp_modifiers", "readline_expression",
            "refgen_expression", "relational_expression",
            "require_version_expression", "return_expression", "role_statement",
            "scalar_deref_expression", "slice_expression", "sort_expression", "statement_label",
            "stub_expression", "substitution_regexp_modifiers", "transliteration_expression",
            "transliteration_modifiers", "try_statement", "unary_expression", "undef_expression",
            "use_version_statement", "variable_declaration",
        ];
        validate_unused_kinds_audit(&Perl, documented_unused)
            .expect("Perl unused node kinds audit failed");
    }
}
