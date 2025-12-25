//! Erlang language support.

use std::path::{Path, PathBuf};
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use crate::external_packages::ResolvedPackage;
use moss_core::tree_sitter::Node;

/// Erlang language support.
pub struct Erlang;

impl Language for Erlang {
    fn name(&self) -> &'static str { "Erlang" }
    fn extensions(&self) -> &'static [&'static str] { &["erl", "hrl"] }
    fn grammar_name(&self) -> &'static str { "erlang" }

    fn has_symbols(&self) -> bool { true }

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
        &["case_expr", "if_expr", "receive_expr", "try_expr", "fun_clause"]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &["case_expr", "if_expr", "receive_expr", "try_expr"]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &["cr_clause", "if_clause", "catch_clause", "guard"]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &["case_expr", "if_expr", "receive_expr", "try_expr",
          "function_clause", "fun_clause"]
    }

    fn extract_function(&self, node: &Node, content: &str, _in_container: bool) -> Option<Symbol> {
        if node.kind() != "function_clause" {
            return None;
        }

        let name = self.node_name(node, content)?;

        // Get arity from parameters
        let arity = node.child_by_field_name("arguments")
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
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public, // Would need export analysis for accuracy
            children: Vec::new(),
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
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    visibility: Visibility::Public,
                    children: Vec::new(),
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
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
            children: Vec::new(),
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

    fn is_public(&self, _node: &Node, _content: &str) -> bool {
        // Would need module-level export analysis
        true
    }

    fn get_visibility(&self, _node: &Node, _content: &str) -> Visibility {
        Visibility::Public
    }

    fn container_body<'a>(&self, _node: &'a Node<'a>) -> Option<Node<'a>> { None }

    fn body_has_docstring(&self, _body: &Node, _content: &str) -> bool { false }

    fn node_name<'a>(&self, node: &Node, content: &'a str) -> Option<&'a str> {
        node.child_by_field_name("name")
            .map(|n| &content[n.byte_range()])
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        if ext != "erl" && ext != "hrl" { return None; }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![
            format!("src/{}.erl", module),
            format!("{}.erl", module),
        ]
    }

    fn lang_key(&self) -> &'static str { "erlang" }

    fn is_stdlib_import(&self, import_name: &str, _project_root: &Path) -> bool {
        // Erlang OTP modules
        matches!(import_name,
            "lists" | "maps" | "io" | "file" | "gen_server" | "gen_statem" |
            "supervisor" | "application" | "ets" | "dets" | "mnesia" |
            "string" | "binary" | "proplists" | "dict" | "queue" | "sets" |
            "erlang" | "kernel" | "stdlib" | "crypto" | "ssl" | "inets" |
            "cowboy" | "ranch" | "logger"
        )
    }

    fn find_stdlib(&self, _project_root: &Path) -> Option<PathBuf> { None }

    fn resolve_local_import(&self, import: &str, _current_file: &Path, project_root: &Path) -> Option<PathBuf> {
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

    fn resolve_external_import(&self, _import_name: &str, _project_root: &Path) -> Option<ResolvedPackage> {
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

    fn indexable_extensions(&self) -> &'static [&'static str] { &["erl"] }
    fn package_sources(&self, _project_root: &Path) -> Vec<crate::PackageSource> { Vec::new() }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{skip_dotfiles, has_extension};
        if skip_dotfiles(name) { return true; }
        if is_dir && (name == "_build" || name == "deps" || name == ".rebar3") {
            return true;
        }
        !is_dir && !has_extension(name, &["erl", "hrl"])
    }

    fn discover_packages(&self, _source: &crate::PackageSource) -> Vec<(String, PathBuf)> { Vec::new() }

    fn package_module_name(&self, entry_name: &str) -> String {
        entry_name.strip_suffix(".erl")
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
