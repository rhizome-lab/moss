//! SQL language support.

use std::path::{Path, PathBuf};
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use crate::external_packages::ResolvedPackage;
use moss_core::tree_sitter::Node;

/// SQL language support.
pub struct Sql;

impl Language for Sql {
    fn name(&self) -> &'static str { "SQL" }
    fn extensions(&self) -> &'static [&'static str] { &["sql"] }
    fn grammar_name(&self) -> &'static str { "sql" }

    fn has_symbols(&self) -> bool { true }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["create_table_statement", "create_view_statement", "create_schema_statement"]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["create_function_statement", "create_procedure_statement"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["create_type_statement"]
    }

    fn import_kinds(&self) -> &'static [&'static str] { &[] }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["create_table_statement", "create_view_statement", "create_function_statement"]
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
            "create_table_statement" => SymbolKind::Struct,
            "create_view_statement" => SymbolKind::Struct,
            "create_function_statement" => SymbolKind::Function,
            "create_procedure_statement" => SymbolKind::Function,
            "create_type_statement" => SymbolKind::Type,
            "create_index_statement" => SymbolKind::Variable,
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
        &["case_expression", "if_statement"]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &["case_expression", "join_clause", "where_clause", "having_clause"]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &["subquery", "case_expression"]
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
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
            children: Vec::new(),
        })
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        let name = self.extract_sql_name(node, content)?;
        let (kind, keyword) = match node.kind() {
            "create_view_statement" => (SymbolKind::Struct, "VIEW"),
            "create_schema_statement" => (SymbolKind::Module, "SCHEMA"),
            _ => (SymbolKind::Struct, "TABLE"),
        };

        Some(Symbol {
            name: name.clone(),
            kind,
            signature: format!("CREATE {} {}", keyword, name),
            docstring: self.extract_docstring(node, content),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
            children: Vec::new(),
        })
    }

    fn extract_type(&self, node: &Node, content: &str) -> Option<Symbol> {
        let name = self.extract_sql_name(node, content)?;

        Some(Symbol {
            name: name.clone(),
            kind: SymbolKind::Type,
            signature: format!("CREATE TYPE {}", name),
            docstring: None,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
            children: Vec::new(),
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

    fn extract_imports(&self, _node: &Node, _content: &str) -> Vec<Import> { Vec::new() }

    fn is_public(&self, _node: &Node, _content: &str) -> bool { true }
    fn get_visibility(&self, _node: &Node, _content: &str) -> Visibility { Visibility::Public }

    fn container_body<'a>(&self, _node: &'a Node<'a>) -> Option<Node<'a>> { None }
    fn body_has_docstring(&self, _body: &Node, _content: &str) -> bool { false }
    fn node_name<'a>(&self, _node: &Node, _content: &'a str) -> Option<&'a str> { None }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        if ext != "sql" { return None; }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![format!("{}.sql", module)]
    }

    fn lang_key(&self) -> &'static str { "sql" }

    fn is_stdlib_import(&self, _import_name: &str, _project_root: &Path) -> bool { false }
    fn find_stdlib(&self, _project_root: &Path) -> Option<PathBuf> { None }
    fn resolve_local_import(&self, _import: &str, _current_file: &Path, _project_root: &Path) -> Option<PathBuf> { None }
    fn resolve_external_import(&self, _import_name: &str, _project_root: &Path) -> Option<ResolvedPackage> { None }
    fn get_version(&self, _project_root: &Path) -> Option<String> { None }
    fn find_package_cache(&self, _project_root: &Path) -> Option<PathBuf> { None }
    fn indexable_extensions(&self) -> &'static [&'static str] { &["sql"] }
    fn package_sources(&self, _project_root: &Path) -> Vec<crate::PackageSource> { Vec::new() }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{skip_dotfiles, has_extension};
        if skip_dotfiles(name) { return true; }
        !is_dir && !has_extension(name, &["sql"])
    }

    fn discover_packages(&self, _source: &crate::PackageSource) -> Vec<(String, PathBuf)> { Vec::new() }

    fn package_module_name(&self, entry_name: &str) -> String {
        entry_name.strip_suffix(".sql").unwrap_or(entry_name).to_string()
    }

    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() { Some(path.to_path_buf()) } else { None }
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
            if found_create && (child.kind() == "identifier" || child.kind() == "object_reference") {
                return Some(content[child.byte_range()].to_string());
            }
        }
        None
    }
}
