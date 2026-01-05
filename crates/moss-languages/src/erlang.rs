//! Erlang language support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// Erlang language support.
pub struct Erlang;

impl Language for Erlang {
    fn name(&self) -> &'static str {
        "Erlang"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["erl", "hrl"]
    }
    fn grammar_name(&self) -> &'static str {
        "erlang"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["module_attribute"] // -module(name).
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_clause"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["type_alias", "record_decl"]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["module_attribute"] // -import(module, [...]).
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["function_clause"] // Only exported functions are public
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::ExplicitExport // -export([...]).
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        // Functions are only public if listed in -export
        // For now, return all functions as we'd need module-level analysis
        if node.kind() == "function_clause" {
            if let Some(name) = self.node_name(node, content) {
                return vec![Export {
                    name: name.to_string(),
                    kind: SymbolKind::Function,
                    line: node.start_position().row + 1,
                }];
            }
        }
        Vec::new()
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &[
            "case_expr",
            "if_expr",
            "receive_expr",
            "try_expr",
            "fun_clause",
        ]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &["case_expr", "if_expr", "receive_expr", "try_expr"]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &["cr_clause", "if_clause", "catch_clause", "guard"]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &[
            "case_expr",
            "if_expr",
            "receive_expr",
            "try_expr",
            "function_clause",
            "fun_clause",
        ]
    }

    fn signature_suffix(&self) -> &'static str {
        ""
    }

    fn extract_function(&self, node: &Node, content: &str, _in_container: bool) -> Option<Symbol> {
        if node.kind() != "function_clause" {
            return None;
        }

        let name = self.node_name(node, content)?;

        // Get arity from parameters
        let arity = node
            .child_by_field_name("arguments")
            .map(|args| {
                let mut cursor = args.walk();
                args.children(&mut cursor).count()
            })
            .unwrap_or(0);

        let signature = format!("{}/{}", name, arity);

        Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Function,
            signature,
            docstring: self.extract_docstring(node, content),
            attributes: Vec::new(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public, // Would need export analysis for accuracy
            children: Vec::new(),
            is_interface_impl: false,
            implements: Vec::new(),
        })
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        if node.kind() != "module_attribute" {
            return None;
        }

        let text = &content[node.byte_range()];
        if !text.starts_with("-module(") {
            return None;
        }

        // Extract module name from -module(name).
        if let Some(start) = text.find('(') {
            let rest = &text[start + 1..];
            if let Some(end) = rest.find(')') {
                let name = rest[..end].trim().to_string();
                return Some(Symbol {
                    name: name.clone(),
                    kind: SymbolKind::Module,
                    signature: format!("-module({}).", name),
                    docstring: None,
                    attributes: Vec::new(),
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    visibility: Visibility::Public,
                    children: Vec::new(),
                    is_interface_impl: false,
                    implements: Vec::new(),
                });
            }
        }

        None
    }

    fn extract_type(&self, node: &Node, content: &str) -> Option<Symbol> {
        if node.kind() != "type_alias" && node.kind() != "record_decl" {
            return None;
        }

        let name = self.node_name(node, content)?;
        let kind = if node.kind() == "record_decl" {
            SymbolKind::Struct
        } else {
            SymbolKind::Type
        };

        Some(Symbol {
            name: name.to_string(),
            kind,
            signature: content[node.byte_range()].lines().next()?.to_string(),
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
        // Erlang uses %% or %%% for documentation comments
        let mut prev = node.prev_sibling();
        let mut doc_lines = Vec::new();

        while let Some(sibling) = prev {
            let text = &content[sibling.byte_range()];
            if sibling.kind() == "comment" && text.starts_with("%%") {
                let line = text.trim_start_matches('%').trim();
                if !line.starts_with('@') {
                    doc_lines.push(line.to_string());
                }
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
        if node.kind() != "module_attribute" {
            return Vec::new();
        }

        let text = &content[node.byte_range()];
        let line = node.start_position().row + 1;

        // Handle -import(module, [...]).
        if text.starts_with("-import(") {
            if let Some(start) = text.find('(') {
                let rest = &text[start + 1..];
                if let Some(comma) = rest.find(',') {
                    let module = rest[..comma].trim().to_string();
                    return vec![Import {
                        module,
                        names: Vec::new(),
                        alias: None,
                        is_wildcard: false,
                        is_relative: false,
                        line,
                    }];
                }
            }
        }

        // Handle -include("file.hrl"). or -include_lib("app/include/file.hrl").
        if text.starts_with("-include") {
            if let Some(start) = text.find('"') {
                let rest = &text[start + 1..];
                if let Some(end) = rest.find('"') {
                    let module = rest[..end].to_string();
                    return vec![Import {
                        module,
                        names: Vec::new(),
                        alias: None,
                        is_wildcard: false,
                        is_relative: text.starts_with("-include("),
                        line,
                    }];
                }
            }
        }

        Vec::new()
    }

    fn format_import(&self, import: &Import, names: Option<&[&str]>) -> String {
        // Erlang: -import(module, [func/arity, ...]).
        let names_to_use: Vec<&str> = names
            .map(|n| n.to_vec())
            .unwrap_or_else(|| import.names.iter().map(|s| s.as_str()).collect());
        if names_to_use.is_empty() {
            format!("-import({}, []).", import.module)
        } else {
            format!("-import({}, [{}]).", import.module, names_to_use.join(", "))
        }
    }

    fn is_public(&self, _node: &Node, _content: &str) -> bool {
        // Would need module-level export analysis
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

    fn container_body<'a>(&self, _node: &'a Node<'a>) -> Option<Node<'a>> {
        None
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
        if ext != "erl" && ext != "hrl" {
            return None;
        }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![format!("src/{}.erl", module), format!("{}.erl", module)]
    }

    fn lang_key(&self) -> &'static str {
        "erlang"
    }

    fn is_stdlib_import(&self, import_name: &str, _project_root: &Path) -> bool {
        // Erlang OTP modules
        matches!(
            import_name,
            "lists"
                | "maps"
                | "io"
                | "file"
                | "gen_server"
                | "gen_statem"
                | "supervisor"
                | "application"
                | "ets"
                | "dets"
                | "mnesia"
                | "string"
                | "binary"
                | "proplists"
                | "dict"
                | "queue"
                | "sets"
                | "erlang"
                | "kernel"
                | "stdlib"
                | "crypto"
                | "ssl"
                | "inets"
                | "cowboy"
                | "ranch"
                | "logger"
        )
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
        let paths = [
            format!("src/{}.erl", import),
            format!("include/{}.hrl", import),
            format!("{}.erl", import),
        ];

        for p in &paths {
            let full = project_root.join(p);
            if full.is_file() {
                return Some(full);
            }
        }

        None
    }

    fn resolve_external_import(
        &self,
        _import_name: &str,
        _project_root: &Path,
    ) -> Option<ResolvedPackage> {
        // Hex/rebar3 package resolution would go here
        None
    }

    fn get_version(&self, _project_root: &Path) -> Option<String> {
        // Check rebar.config or .app.src for version
        // Would need glob to find *.app.src files
        None
    }

    fn find_package_cache(&self, project_root: &Path) -> Option<PathBuf> {
        let deps = project_root.join("_build/default/lib");
        if deps.is_dir() {
            return Some(deps);
        }
        let deps = project_root.join("deps");
        if deps.is_dir() {
            return Some(deps);
        }
        None
    }

    fn indexable_extensions(&self) -> &'static [&'static str] {
        &["erl"]
    }
    fn package_sources(&self, _project_root: &Path) -> Vec<crate::PackageSource> {
        Vec::new()
    }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{has_extension, skip_dotfiles};
        if skip_dotfiles(name) {
            return true;
        }
        if is_dir && (name == "_build" || name == "deps" || name == ".rebar3") {
            return true;
        }
        !is_dir && !has_extension(name, self.indexable_extensions())
    }

    fn discover_packages(&self, _source: &crate::PackageSource) -> Vec<(String, PathBuf)> {
        Vec::new()
    }

    fn package_module_name(&self, entry_name: &str) -> String {
        entry_name
            .strip_suffix(".erl")
            .or_else(|| entry_name.strip_suffix(".hrl"))
            .unwrap_or(entry_name)
            .to_string()
    }

    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() {
            return Some(path.to_path_buf());
        }
        // Look for src/<name>.erl
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            let src = path.join("src").join(format!("{}.erl", name));
            if src.is_file() {
                return Some(src);
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
            "ann_type", "b_generator", "binary_comprehension", "bit_type_list",
            "bit_type_unit", "block_expr", "catch_expr", "clause_body",
            "cond_match_expr", "deprecated_module", "export_attribute",
            "export_type_attribute", "field_type", "fun_type", "fun_type_sig",
            "generator", "guard_clause", "import_attribute", "list_comprehension",
            "map_comprehension", "map_generator", "match_expr", "module",
            "pp_elif", "pp_else", "pp_endif", "pp_if", "pp_ifdef", "pp_ifndef",
            "range_type", "remote_module", "replacement_cr_clauses",
            "replacement_function_clauses", "ssr_definition", "try_after",
            "try_class", "try_stack", "type_guards", "type_name", "type_sig",
        ];
        validate_unused_kinds_audit(&Erlang, documented_unused)
            .expect("Erlang unused node kinds audit failed");
    }
}
