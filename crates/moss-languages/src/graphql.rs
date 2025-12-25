//! GraphQL language support.

use std::path::{Path, PathBuf};
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use crate::external_packages::ResolvedPackage;
use moss_core::tree_sitter::Node;

/// GraphQL language support.
pub struct GraphQL;

impl Language for GraphQL {
    fn name(&self) -> &'static str { "GraphQL" }
    fn extensions(&self) -> &'static [&'static str] { &["graphql", "gql"] }
    fn grammar_name(&self) -> &'static str { "graphql" }

    fn has_symbols(&self) -> bool { true }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["object_type_definition", "interface_type_definition", "enum_type_definition",
          "union_type_definition", "input_object_type_definition"]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["field_definition", "operation_definition"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["object_type_definition", "interface_type_definition", "enum_type_definition",
          "union_type_definition", "input_object_type_definition", "scalar_type_definition"]
    }

    fn import_kinds(&self) -> &'static [&'static str] { &[] }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["object_type_definition", "interface_type_definition", "operation_definition"]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::NotApplicable
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        let name = match self.node_name(node, content) {
            Some(n) => n.to_string(),
            None => return Vec::new(),
        };

        let kind = match node.kind() {
            "object_type_definition" => SymbolKind::Struct,
            "interface_type_definition" => SymbolKind::Interface,
            "enum_type_definition" | "union_type_definition" => SymbolKind::Enum,
            "input_object_type_definition" => SymbolKind::Struct,
            "scalar_type_definition" => SymbolKind::Type,
            "operation_definition" => SymbolKind::Function,
            "field_definition" => SymbolKind::Method,
            _ => return Vec::new(),
        };

        vec![Export {
            name,
            kind,
            line: node.start_position().row + 1,
        }]
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["selection_set"]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] { &[] }
    fn complexity_nodes(&self) -> &'static [&'static str] { &["selection_set"] }
    fn nesting_nodes(&self) -> &'static [&'static str] { &["selection_set", "object_type_definition"] }

    fn extract_function(&self, node: &Node, content: &str, _in_container: bool) -> Option<Symbol> {
        let name = self.node_name(node, content)?;

        let text = &content[node.byte_range()];
        let first_line = text.lines().next().unwrap_or(text);

        Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Method,
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
            "interface_type_definition" => (SymbolKind::Interface, "interface"),
            "enum_type_definition" => (SymbolKind::Enum, "enum"),
            "union_type_definition" => (SymbolKind::Enum, "union"),
            "input_object_type_definition" => (SymbolKind::Struct, "input"),
            "scalar_type_definition" => (SymbolKind::Type, "scalar"),
            _ => (SymbolKind::Struct, "type"),
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
        // GraphQL uses """ for descriptions
        let mut prev = node.prev_sibling();
        while let Some(sibling) = prev {
            let text = &content[sibling.byte_range()];
            if sibling.kind() == "description" || text.starts_with("\"\"\"") {
                let inner = text
                    .trim_start_matches("\"\"\"")
                    .trim_end_matches("\"\"\"")
                    .trim();
                if !inner.is_empty() {
                    return Some(inner.to_string());
                }
            }
            prev = sibling.prev_sibling();
        }
        None
    }

    fn extract_imports(&self, _node: &Node, _content: &str) -> Vec<Import> { Vec::new() }

    fn is_public(&self, _node: &Node, _content: &str) -> bool { true }
    fn get_visibility(&self, _node: &Node, _content: &str) -> Visibility { Visibility::Public }

    fn container_body<'a>(&self, node: &'a Node<'a>) -> Option<Node<'a>> {
        node.child_by_field_name("fields_definition")
    }

    fn body_has_docstring(&self, _body: &Node, _content: &str) -> bool { false }

    fn node_name<'a>(&self, node: &Node, content: &'a str) -> Option<&'a str> {
        node.child_by_field_name("name")
            .map(|n| &content[n.byte_range()])
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        if ext != "graphql" && ext != "gql" { return None; }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![
            format!("{}.graphql", module),
            format!("{}.gql", module),
        ]
    }

    fn lang_key(&self) -> &'static str { "graphql" }

    fn is_stdlib_import(&self, _import_name: &str, _project_root: &Path) -> bool { false }
    fn find_stdlib(&self, _project_root: &Path) -> Option<PathBuf> { None }
    fn resolve_local_import(&self, _import: &str, _current_file: &Path, _project_root: &Path) -> Option<PathBuf> { None }
    fn resolve_external_import(&self, _import_name: &str, _project_root: &Path) -> Option<ResolvedPackage> { None }
    fn get_version(&self, _project_root: &Path) -> Option<String> { None }
    fn find_package_cache(&self, _project_root: &Path) -> Option<PathBuf> { None }
    fn indexable_extensions(&self) -> &'static [&'static str] { &["graphql", "gql"] }
    fn package_sources(&self, _project_root: &Path) -> Vec<crate::PackageSource> { Vec::new() }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{skip_dotfiles, has_extension};
        if skip_dotfiles(name) { return true; }
        !is_dir && !has_extension(name, &["graphql", "gql"])
    }

    fn discover_packages(&self, _source: &crate::PackageSource) -> Vec<(String, PathBuf)> { Vec::new() }

    fn package_module_name(&self, entry_name: &str) -> String {
        entry_name.strip_suffix(".graphql")
            .or_else(|| entry_name.strip_suffix(".gql"))
            .unwrap_or(entry_name)
            .to_string()
    }

    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() { Some(path.to_path_buf()) } else { None }
    }
}
