//! Elixir language support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// Elixir language support.
pub struct Elixir;

impl Language for Elixir {
    fn name(&self) -> &'static str {
        "Elixir"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["ex", "exs"]
    }
    fn grammar_name(&self) -> &'static str {
        "elixir"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["call"] // defmodule, defprotocol, defimpl
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["call"] // def, defp, defmacro, defmacrop
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["call"] // defstruct, @type
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["call"] // import, alias, require, use
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["call"] // def, defmacro (not defp, defmacrop)
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::NamingConvention // def = public, defp = private
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        if node.kind() != "call" {
            return Vec::new();
        }

        let text = &content[node.byte_range()];

        // Check for def (not defp)
        if text.starts_with("def ") && !text.starts_with("defp") {
            if let Some(name) = self.extract_def_name(node, content) {
                return vec![Export {
                    name,
                    kind: SymbolKind::Function,
                    line: node.start_position().row + 1,
                }];
            }
        }

        // Check for defmacro (not defmacrop)
        if text.starts_with("defmacro ") && !text.starts_with("defmacrop") {
            if let Some(name) = self.extract_def_name(node, content) {
                return vec![Export {
                    name,
                    kind: SymbolKind::Function,
                    line: node.start_position().row + 1,
                }];
            }
        }

        // Check for defmodule
        if text.starts_with("defmodule ") {
            if let Some(name) = self.extract_module_name(node, content) {
                return vec![Export {
                    name,
                    kind: SymbolKind::Module,
                    line: node.start_position().row + 1,
                }];
            }
        }

        Vec::new()
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["do_block", "anonymous_function"]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &["call"] // if, case, cond, with, for, try
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &["call", "binary_operator"] // if, case, cond, and/or
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &["call", "do_block", "anonymous_function"]
    }

    fn signature_suffix(&self) -> &'static str {
        " end"
    }

    fn extract_function(&self, node: &Node, content: &str, _in_container: bool) -> Option<Symbol> {
        if node.kind() != "call" {
            return None;
        }

        let text = &content[node.byte_range()];
        let is_private = if text.starts_with("defp ") || text.starts_with("defmacrop ") {
            true
        } else if text.starts_with("def ") || text.starts_with("defmacro ") {
            false
        } else {
            return None;
        };

        let name = self.extract_def_name(node, content)?;

        // Extract first line as signature
        let first_line = text.lines().next().unwrap_or(text);
        let signature = first_line.trim_end_matches(" do").to_string();

        Some(Symbol {
            name,
            kind: SymbolKind::Function,
            signature,
            docstring: self.extract_docstring(node, content),
            attributes: Vec::new(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: if is_private {
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
        if node.kind() != "call" {
            return None;
        }

        let text = &content[node.byte_range()];
        if !text.starts_with("defmodule ") {
            return None;
        }

        let name = self.extract_module_name(node, content)?;

        Some(Symbol {
            name: name.clone(),
            kind: SymbolKind::Module,
            signature: format!("defmodule {}", name),
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

    fn extract_type(&self, _node: &Node, _content: &str) -> Option<Symbol> {
        None
    }

    fn extract_docstring(&self, node: &Node, content: &str) -> Option<String> {
        // Look for @doc or @moduledoc before the node
        let mut prev = node.prev_sibling();
        while let Some(sibling) = prev {
            let text = &content[sibling.byte_range()];
            if text.contains("@doc") || text.contains("@moduledoc") {
                // Extract the string content
                if let Some(start) = text.find("\"\"\"") {
                    let rest = &text[start + 3..];
                    if let Some(end) = rest.find("\"\"\"") {
                        return Some(rest[..end].trim().to_string());
                    }
                }
                if let Some(start) = text.find('"') {
                    let rest = &text[start + 1..];
                    if let Some(end) = rest.find('"') {
                        return Some(rest[..end].to_string());
                    }
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
        if node.kind() != "call" {
            return Vec::new();
        }

        let text = &content[node.byte_range()];
        let line = node.start_position().row + 1;

        // Handle import, alias, require, use
        for keyword in &["import ", "alias ", "require ", "use "] {
            if text.starts_with(keyword) {
                let rest = text[keyword.len()..].trim();
                let module = rest
                    .split(|c: char| c.is_whitespace() || c == ',')
                    .next()
                    .unwrap_or(rest)
                    .to_string();

                if !module.is_empty() {
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

        Vec::new()
    }

    fn format_import(&self, import: &Import, names: Option<&[&str]>) -> String {
        // Elixir: import Module or import Module, only: [a: 1, b: 2]
        let names_to_use: Vec<&str> = names
            .map(|n| n.to_vec())
            .unwrap_or_else(|| import.names.iter().map(|s| s.as_str()).collect());
        if names_to_use.is_empty() {
            format!("import {}", import.module)
        } else {
            format!(
                "import {}, only: [{}]",
                import.module,
                names_to_use.join(", ")
            )
        }
    }

    fn is_public(&self, node: &Node, content: &str) -> bool {
        if node.kind() != "call" {
            return false;
        }
        let text = &content[node.byte_range()];
        (text.starts_with("def ") && !text.starts_with("defp"))
            || (text.starts_with("defmacro ") && !text.starts_with("defmacrop"))
            || text.starts_with("defmodule ")
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
        // Look for do_block child
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "do_block" {
                return Some(child);
            }
        }
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
        if ext != "ex" && ext != "exs" {
            return None;
        }
        let stem = path.file_stem()?.to_str()?;
        // Convert snake_case to PascalCase
        Some(
            stem.split('_')
                .map(|s| {
                    let mut c = s.chars();
                    match c.next() {
                        None => String::new(),
                        Some(f) => f.to_uppercase().chain(c).collect(),
                    }
                })
                .collect::<String>(),
        )
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        // Convert PascalCase to snake_case
        let snake = module
            .chars()
            .enumerate()
            .map(|(i, c)| {
                if c.is_uppercase() && i > 0 {
                    format!("_{}", c.to_lowercase())
                } else {
                    c.to_lowercase().to_string()
                }
            })
            .collect::<String>();

        vec![format!("lib/{}.ex", snake), format!("{}.ex", snake)]
    }

    fn lang_key(&self) -> &'static str {
        "elixir"
    }

    fn is_stdlib_import(&self, import_name: &str, _project_root: &Path) -> bool {
        // Elixir stdlib modules
        matches!(
            import_name,
            "Kernel"
                | "Enum"
                | "List"
                | "Map"
                | "String"
                | "IO"
                | "File"
                | "Path"
                | "System"
                | "Process"
                | "Agent"
                | "GenServer"
                | "Supervisor"
                | "Task"
                | "Stream"
                | "Regex"
                | "DateTime"
                | "Date"
                | "Time"
                | "Integer"
                | "Float"
                | "Tuple"
                | "Keyword"
                | "Access"
                | "Protocol"
                | "Macro"
                | "Code"
                | "Module"
                | "Application"
                | "Logger"
                | "Mix"
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
        // Convert module name to path
        let parts: Vec<&str> = import.split('.').collect();
        let snake_parts: Vec<String> = parts
            .iter()
            .map(|p| {
                p.chars()
                    .enumerate()
                    .map(|(i, c)| {
                        if c.is_uppercase() && i > 0 {
                            format!("_{}", c.to_lowercase())
                        } else {
                            c.to_lowercase().to_string()
                        }
                    })
                    .collect::<String>()
            })
            .collect();

        let path = snake_parts.join("/");
        let full = project_root.join("lib").join(format!("{}.ex", path));
        if full.is_file() {
            return Some(full);
        }

        None
    }

    fn resolve_external_import(
        &self,
        _import_name: &str,
        _project_root: &Path,
    ) -> Option<ResolvedPackage> {
        // Hex package resolution would go here
        None
    }

    fn get_version(&self, project_root: &Path) -> Option<String> {
        let mix_exs = project_root.join("mix.exs");
        if mix_exs.is_file() {
            if let Ok(content) = std::fs::read_to_string(&mix_exs) {
                // Look for version: "x.y.z"
                for line in content.lines() {
                    if line.contains("version:") && line.contains('"') {
                        if let Some(start) = line.find('"') {
                            let rest = &line[start + 1..];
                            if let Some(end) = rest.find('"') {
                                return Some(rest[..end].to_string());
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn find_package_cache(&self, project_root: &Path) -> Option<PathBuf> {
        let deps = project_root.join("deps");
        if deps.is_dir() {
            return Some(deps);
        }
        None
    }

    fn indexable_extensions(&self) -> &'static [&'static str] {
        &["ex"]
    }
    fn package_sources(&self, _project_root: &Path) -> Vec<crate::PackageSource> {
        Vec::new()
    }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{has_extension, skip_dotfiles};
        if skip_dotfiles(name) {
            return true;
        }
        if is_dir && (name == "_build" || name == "deps" || name == ".elixir_ls") {
            return true;
        }
        !is_dir && !has_extension(name, self.indexable_extensions())
    }

    fn discover_packages(&self, _source: &crate::PackageSource) -> Vec<(String, PathBuf)> {
        Vec::new()
    }

    fn package_module_name(&self, entry_name: &str) -> String {
        entry_name
            .strip_suffix(".ex")
            .or_else(|| entry_name.strip_suffix(".exs"))
            .unwrap_or(entry_name)
            .to_string()
    }

    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() {
            return Some(path.to_path_buf());
        }
        let lib = path
            .join("lib")
            .join(format!("{}.ex", path.file_name()?.to_str()?));
        if lib.is_file() {
            return Some(lib);
        }
        None
    }
}

impl Elixir {
    fn extract_def_name(&self, node: &Node, content: &str) -> Option<String> {
        // Look for the function name after def/defp
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "call" || child.kind() == "identifier" {
                let text = &content[child.byte_range()];
                // Extract just the name (before parentheses)
                let name = text.split('(').next().unwrap_or(text).trim();
                if !name.is_empty()
                    && name != "def"
                    && name != "defp"
                    && name != "defmacro"
                    && name != "defmacrop"
                {
                    return Some(name.to_string());
                }
            }
        }
        None
    }

    fn extract_module_name(&self, node: &Node, content: &str) -> Option<String> {
        // Look for the module name after defmodule
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "alias" || child.kind() == "atom" {
                let text = &content[child.byte_range()];
                if !text.is_empty() && text != "defmodule" {
                    return Some(text.to_string());
                }
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
            "after_block", "block", "body", "catch_block", "charlist",
            "else_block", "identifier", "interpolation", "operator_identifier",
            "rescue_block", "sigil_modifiers", "stab_clause", "struct",
            "unary_operator",
        ];
        validate_unused_kinds_audit(&Elixir, documented_unused)
            .expect("Elixir unused node kinds audit failed");
    }
}
