//! Julia language support.

use std::path::{Path, PathBuf};
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use crate::external_packages::ResolvedPackage;
use arborium::tree_sitter::Node;

/// Julia language support.
pub struct Julia;

impl Language for Julia {
    fn name(&self) -> &'static str { "Julia" }
    fn extensions(&self) -> &'static [&'static str] { &["jl"] }
    fn grammar_name(&self) -> &'static str { "julia" }

    fn has_symbols(&self) -> bool { true }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["module_definition", "struct_definition", "abstract_definition"]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_definition", "arrow_function_expression", "macro_definition"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["struct_definition", "abstract_definition", "primitive_definition"]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["import_statement", "using_statement"]
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["function_definition", "struct_definition", "const_statement"]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::ExplicitExport
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        let name = match self.node_name(node, content) {
            Some(n) => n.to_string(),
            None => return Vec::new(),
        };

        let kind = match node.kind() {
            "function_definition" | "arrow_function_expression" => SymbolKind::Function,
            "macro_definition" => SymbolKind::Function,
            "struct_definition" => SymbolKind::Struct,
            "abstract_definition" => SymbolKind::Interface,
            "module_definition" => SymbolKind::Module,
            _ => return Vec::new(),
        };

        vec![Export {
            name,
            kind,
            line: node.start_position().row + 1,
        }]
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["function_definition", "let_statement", "do_clause", "module_definition"]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &["if_statement", "for_statement", "while_statement", "try_statement"]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &["if_statement", "for_statement", "while_statement", "elseif_clause",
          "ternary_expression"]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &["function_definition", "module_definition", "struct_definition",
          "if_statement", "for_statement"]
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
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
            children: Vec::new(),
        })
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        let name = self.node_name(node, content)?;

        let (kind, keyword) = match node.kind() {
            "module_definition" => (SymbolKind::Module, "module"),
            "struct_definition" => (SymbolKind::Struct, "struct"),
            "abstract_definition" => (SymbolKind::Interface, "abstract type"),
            _ => return None,
        };

        Some(Symbol {
            name: name.to_string(),
            kind,
            signature: format!("{} {}", keyword, name),
            docstring: self.extract_docstring(node, content),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
            children: Vec::new(),
        })
    }

    fn extract_type(&self, node: &Node, content: &str) -> Option<Symbol> {
        self.extract_container(node, content)
    }

    fn extract_docstring(&self, node: &Node, content: &str) -> Option<String> {
        // Julia uses """ docstrings before definitions
        let mut prev = node.prev_sibling();
        while let Some(sibling) = prev {
            let text = &content[sibling.byte_range()];
            if sibling.kind() == "string_literal" && text.starts_with("\"\"\"") {
                let inner = text.trim_start_matches("\"\"\"")
                    .trim_end_matches("\"\"\"")
                    .trim();
                if !inner.is_empty() {
                    return Some(inner.lines().next().unwrap_or(inner).to_string());
                }
            }
            if sibling.kind() == "comment" {
                prev = sibling.prev_sibling();
            } else {
                break;
            }
        }
        None
    }

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        let text = &content[node.byte_range()];
        let line = node.start_position().row + 1;

        let (keyword, is_wildcard) = if text.starts_with("using ") {
            ("using ", true)
        } else if text.starts_with("import ") {
            ("import ", false)
        } else {
            return Vec::new();
        };

        let rest = text.strip_prefix(keyword).unwrap_or("");
        let module = rest.split(|c| c == ':' || c == ',')
            .next()
            .map(|s| s.trim().to_string())
            .unwrap_or_default();

        if module.is_empty() {
            return Vec::new();
        }

        vec![Import {
            module,
            names: Vec::new(),
            alias: None,
            is_wildcard,
            is_relative: false,
            line,
        }]
    }

    fn is_public(&self, _node: &Node, _content: &str) -> bool { true }
    fn get_visibility(&self, _node: &Node, _content: &str) -> Visibility { Visibility::Public }

    fn embedded_content(&self, _node: &Node, _content: &str) -> Option<crate::EmbeddedBlock> { None }

    fn container_body<'a>(&self, node: &'a Node<'a>) -> Option<Node<'a>> {
        node.child_by_field_name("body")
    }

    fn body_has_docstring(&self, _body: &Node, _content: &str) -> bool { false }

    fn node_name<'a>(&self, node: &Node, content: &'a str) -> Option<&'a str> {
        node.child_by_field_name("name")
            .map(|n| &content[n.byte_range()])
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        if ext != "jl" { return None; }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![
            format!("{}.jl", module),
            format!("src/{}.jl", module),
        ]
    }

    fn lang_key(&self) -> &'static str { "julia" }

    fn is_stdlib_import(&self, import_name: &str, _project_root: &Path) -> bool {
        matches!(import_name, "Base" | "Core" | "Main" | "LinearAlgebra" |
            "Statistics" | "Random" | "Dates" | "Printf" | "Test" | "Pkg")
    }

    fn find_stdlib(&self, _project_root: &Path) -> Option<PathBuf> { None }

    fn resolve_local_import(&self, import: &str, _current_file: &Path, project_root: &Path) -> Option<PathBuf> {
        let candidates = [
            project_root.join("src").join(format!("{}.jl", import)),
            project_root.join(format!("{}.jl", import)),
        ];
        for c in &candidates {
            if c.is_file() {
                return Some(c.clone());
            }
        }
        None
    }

    fn resolve_external_import(&self, _import_name: &str, _project_root: &Path) -> Option<ResolvedPackage> {
        None
    }

    fn get_version(&self, project_root: &Path) -> Option<String> {
        if project_root.join("Project.toml").is_file() {
            return Some("Project.toml".to_string());
        }
        None
    }

    fn find_package_cache(&self, _project_root: &Path) -> Option<PathBuf> {
        if let Some(home) = std::env::var_os("HOME") {
            let depot = PathBuf::from(home).join(".julia/packages");
            if depot.is_dir() {
                return Some(depot);
            }
        }
        None
    }

    fn indexable_extensions(&self) -> &'static [&'static str] { &["jl"] }
    fn package_sources(&self, _project_root: &Path) -> Vec<crate::PackageSource> { Vec::new() }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{skip_dotfiles, has_extension};
        if skip_dotfiles(name) { return true; }
        if is_dir && (name == "test" || name == "docs" || name == "benchmark") {
            return true;
        }
        !is_dir && !has_extension(name, &["jl"])
    }

    fn discover_packages(&self, _source: &crate::PackageSource) -> Vec<(String, PathBuf)> { Vec::new() }

    fn package_module_name(&self, entry_name: &str) -> String {
        entry_name.strip_suffix(".jl").unwrap_or(entry_name).to_string()
    }

    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() {
            return Some(path.to_path_buf());
        }
        let src = path.join("src").join(format!("{}.jl", path.file_name()?.to_str()?));
        if src.is_file() {
            return Some(src);
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
            "adjoint_expression", "binary_expression", "block",
            "block_comment", "break_statement", "broadcast_call_expression", "call_expression",
            "catch_clause", "compound_assignment_expression", "compound_statement",
            "comprehension_expression", "continue_statement", "curly_expression", "else_clause",
            "export_statement", "field_expression", "finally_clause", "for_binding", "for_clause",
            "generator", "global_statement", "identifier", "if_clause", "import_alias",
            "import_path", "index_expression", "interpolation_expression",
            "juxtaposition_expression", "local_statement", "macro_identifier",
            "macrocall_expression", "matrix_expression", "operator", "parametrized_type_expression",
            "parenthesized_expression", "public_statement", "quote_expression", "quote_statement",
            "range_expression", "return_statement", "selected_import", "splat_expression",
            "tuple_expression", "type_head", "typed_expression", "unary_expression",
            "unary_typed_expression", "vector_expression", "where_expression",
        ];
        validate_unused_kinds_audit(&Julia, documented_unused)
            .expect("Julia unused node kinds audit failed");
    }
}
