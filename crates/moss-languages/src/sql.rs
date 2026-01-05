//! SQL language support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// SQL language support.
pub struct Sql;

impl Language for Sql {
    fn name(&self) -> &'static str {
        "SQL"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["sql"]
    }
    fn grammar_name(&self) -> &'static str {
        "sql"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["create_table", "create_view", "create_schema"]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["create_function"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["create_type"]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &[]
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["create_table", "create_view", "create_function"]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::NotApplicable
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        let name = match self.extract_sql_name(node, content) {
            Some(n) => n,
            None => return Vec::new(),
        };

        let kind = match node.kind() {
            "create_table" => SymbolKind::Struct,
            "create_view" | "create_materialized_view" => SymbolKind::Struct,
            "create_function" => SymbolKind::Function,
            "create_type" => SymbolKind::Type,
            "create_index" => SymbolKind::Variable,
            _ => return Vec::new(),
        };

        vec![Export {
            name,
            kind,
            line: node.start_position().row + 1,
        }]
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["subquery", "cte"]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &["case"]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &["case", "join", "where", "having"]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &["subquery", "case"]
    }

    fn signature_suffix(&self) -> &'static str {
        ""
    }

    fn extract_function(&self, node: &Node, content: &str, _in_container: bool) -> Option<Symbol> {
        let name = self.extract_sql_name(node, content)?;

        // Extract first line as signature
        let text = &content[node.byte_range()];
        let first_line = text.lines().next().unwrap_or(text);

        Some(Symbol {
            name,
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
        let name = self.extract_sql_name(node, content)?;
        let (kind, keyword) = match node.kind() {
            "create_view" | "create_materialized_view" => (SymbolKind::Struct, "VIEW"),
            "create_schema" => (SymbolKind::Module, "SCHEMA"),
            _ => (SymbolKind::Struct, "TABLE"),
        };

        Some(Symbol {
            name: name.clone(),
            kind,
            signature: format!("CREATE {} {}", keyword, name),
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
        let name = self.extract_sql_name(node, content)?;

        Some(Symbol {
            name: name.clone(),
            kind: SymbolKind::Type,
            signature: format!("CREATE TYPE {}", name),
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

    fn extract_docstring(&self, node: &Node, content: &str) -> Option<String> {
        // SQL uses -- for comments
        let mut prev = node.prev_sibling();
        let mut doc_lines = Vec::new();

        while let Some(sibling) = prev {
            let text = &content[sibling.byte_range()];
            if sibling.kind() == "comment" && text.starts_with("--") {
                let line = text.strip_prefix("--").unwrap_or(text).trim();
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

    fn extract_imports(&self, _node: &Node, _content: &str) -> Vec<Import> {
        Vec::new()
    }

    fn format_import(&self, _import: &Import, _names: Option<&[&str]>) -> String {
        // SQL has no imports
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

    fn container_body<'a>(&self, _node: &'a Node<'a>) -> Option<Node<'a>> {
        None
    }
    fn body_has_docstring(&self, _body: &Node, _content: &str) -> bool {
        false
    }
    fn node_name<'a>(&self, _node: &Node, _content: &'a str) -> Option<&'a str> {
        None
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        if ext != "sql" {
            return None;
        }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![format!("{}.sql", module)]
    }

    fn lang_key(&self) -> &'static str {
        "sql"
    }

    fn is_stdlib_import(&self, _import_name: &str, _project_root: &Path) -> bool {
        false
    }
    fn find_stdlib(&self, _project_root: &Path) -> Option<PathBuf> {
        None
    }
    fn resolve_local_import(
        &self,
        _import: &str,
        _current_file: &Path,
        _project_root: &Path,
    ) -> Option<PathBuf> {
        None
    }
    fn resolve_external_import(
        &self,
        _import_name: &str,
        _project_root: &Path,
    ) -> Option<ResolvedPackage> {
        None
    }
    fn get_version(&self, _project_root: &Path) -> Option<String> {
        None
    }
    fn find_package_cache(&self, _project_root: &Path) -> Option<PathBuf> {
        None
    }
    fn indexable_extensions(&self) -> &'static [&'static str] {
        &["sql"]
    }
    fn package_sources(&self, _project_root: &Path) -> Vec<crate::PackageSource> {
        Vec::new()
    }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{has_extension, skip_dotfiles};
        if skip_dotfiles(name) {
            return true;
        }
        !is_dir && !has_extension(name, self.indexable_extensions())
    }

    fn discover_packages(&self, _source: &crate::PackageSource) -> Vec<(String, PathBuf)> {
        Vec::new()
    }

    fn package_module_name(&self, entry_name: &str) -> String {
        entry_name
            .strip_suffix(".sql")
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

impl Sql {
    fn extract_sql_name(&self, node: &Node, content: &str) -> Option<String> {
        // Look for identifier after CREATE TABLE/VIEW/FUNCTION etc.
        let mut cursor = node.walk();
        let mut found_create = false;
        for child in node.children(&mut cursor) {
            if child.kind() == "keyword" {
                let text = &content[child.byte_range()].to_uppercase();
                if text == "CREATE" {
                    found_create = true;
                }
            }
            if found_create && (child.kind() == "identifier" || child.kind() == "object_reference")
            {
                return Some(content[child.byte_range()].to_string());
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
            "alter_type", "array_size_definition", "between_expression", "binary_expression",
            "block", "column_definition", "column_definitions", "comment_statement",
            "drop_function", "drop_type", "enum", "enum_elements", "filter_expression",
            "frame_definition", "function_argument", "function_arguments",
            "function_body", "function_cost", "function_declaration", "function_language",
            "function_leakproof", "function_rows", "function_safety", "function_security",
            "function_strictness", "function_support", "function_volatility", "identifier",
            "keyword_before", "keyword_case", "keyword_else", "keyword_enum",
            "keyword_except", "keyword_for", "keyword_force", "keyword_force_not_null",
            "keyword_force_null", "keyword_force_quote", "keyword_foreign",
            "keyword_format", "keyword_function", "keyword_geometry", "keyword_if",
            "keyword_match", "keyword_matched", "keyword_modify", "keyword_regclass",
            "keyword_regtype", "keyword_return", "keyword_returning", "keyword_returns",
            "keyword_statement", "keyword_type", "keyword_while", "keyword_with",
            "keyword_without", "modify_column", "parenthesized_expression",
            "reset_statement", "returning", "row_format", "select_expression",
            "set_statement", "statement", "unary_expression", "var_declaration",
            "var_declarations", "when_clause", "while_statement", "window_clause",
            "window_function", "window_specification",
        ];
        validate_unused_kinds_audit(&Sql, documented_unused)
            .expect("SQL unused node kinds audit failed");
    }
}
