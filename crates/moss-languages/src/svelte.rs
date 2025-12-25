//! Svelte language support.

use std::path::{Path, PathBuf};
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use crate::external_packages::ResolvedPackage;
use moss_core::tree_sitter::Node;

/// Svelte language support.
pub struct Svelte;

impl Language for Svelte {
    fn name(&self) -> &'static str { "Svelte" }
    fn extensions(&self) -> &'static [&'static str] { &["svelte"] }
    fn grammar_name(&self) -> &'static str { "svelte" }

    fn has_symbols(&self) -> bool { true }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["script_element", "style_element"]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &[] // JS functions are in embedded script, not Svelte grammar
    }

    fn type_kinds(&self) -> &'static [&'static str] { &[] }

    fn import_kinds(&self) -> &'static [&'static str] {
        &[] // JS imports are in embedded script, not Svelte grammar
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &[] // JS exports are in embedded script, not Svelte grammar
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::ExplicitExport
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        // Look for export let/const/function
        let text = &content[node.byte_range()];

        if node.kind() == "export_statement" || text.contains("export ") {
            if let Some(name) = self.node_name(node, content) {
                let kind = if text.contains("function") {
                    SymbolKind::Function
                } else {
                    SymbolKind::Variable
                };

                return vec![Export {
                    name: name.to_string(),
                    kind,
                    line: node.start_position().row + 1,
                }];
            }
        }

        Vec::new()
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["if_statement", "each_statement", "await_statement"]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &["if_statement", "each_statement", "await_statement"]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &["if_statement", "each_statement", "else_if_block"]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &["if_statement", "each_statement", "await_statement", "script_element"]
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
            visibility: self.get_visibility(node, content),
            children: Vec::new(),
        })
    }

    fn extract_container(&self, node: &Node, _content: &str) -> Option<Symbol> {
        let kind = match node.kind() {
            "script_element" => SymbolKind::Module,
            "style_element" => SymbolKind::Class,
            _ => return None,
        };

        let name = if node.kind() == "script_element" {
            "<script>".to_string()
        } else {
            "<style>".to_string()
        };

        Some(Symbol {
            name: name.clone(),
            kind,
            signature: name,
            docstring: None,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
            children: Vec::new(),
        })
    }

    fn extract_type(&self, _node: &Node, _content: &str) -> Option<Symbol> { None }

    fn extract_docstring(&self, node: &Node, content: &str) -> Option<String> {
        // JavaScript-style comments
        let mut prev = node.prev_sibling();
        let mut doc_lines = Vec::new();

        while let Some(sibling) = prev {
            let text = &content[sibling.byte_range()];
            if sibling.kind() == "comment" {
                if text.starts_with("/**") {
                    let inner = text
                        .strip_prefix("/**").unwrap_or(text)
                        .strip_suffix("*/").unwrap_or(text);
                    let lines: Vec<&str> = inner.lines()
                        .map(|l| l.trim().trim_start_matches('*').trim())
                        .filter(|l| !l.is_empty() && !l.starts_with('@'))
                        .collect();
                    if !lines.is_empty() {
                        return Some(lines.join(" "));
                    }
                } else if text.starts_with("//") {
                    doc_lines.push(text.strip_prefix("//").unwrap_or(text).trim().to_string());
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
        if node.kind() != "import_statement" {
            return Vec::new();
        }

        let text = &content[node.byte_range()];
        let line = node.start_position().row + 1;

        // Extract from clause
        if let Some(from_idx) = text.find(" from ") {
            let rest = &text[from_idx + 6..];
            if let Some(start) = rest.find('"').or_else(|| rest.find('\'')) {
                let quote = rest.chars().nth(start).unwrap();
                let inner = &rest[start + 1..];
                if let Some(end) = inner.find(quote) {
                    let module = inner[..end].to_string();
                    return vec![Import {
                        module: module.clone(),
                        names: Vec::new(),
                        alias: None,
                        is_wildcard: text.contains(" * "),
                        is_relative: module.starts_with('.'),
                        line,
                    }];
                }
            }
        }

        Vec::new()
    }

    fn is_public(&self, node: &Node, content: &str) -> bool {
        let text = &content[node.byte_range()];
        text.contains("export ")
    }

    fn get_visibility(&self, node: &Node, content: &str) -> Visibility {
        if self.is_public(node, content) {
            Visibility::Public
        } else {
            Visibility::Private
        }
    }

    fn container_body<'a>(&self, node: &'a Node<'a>) -> Option<Node<'a>> {
        // Find the content of script/style elements
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "raw_text" {
                return Some(child);
            }
        }
        None
    }

    fn body_has_docstring(&self, _body: &Node, _content: &str) -> bool { false }

    fn node_name<'a>(&self, node: &Node, content: &'a str) -> Option<&'a str> {
        node.child_by_field_name("name")
            .or_else(|| node.child_by_field_name("function"))
            .map(|n| &content[n.byte_range()])
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        if ext != "svelte" { return None; }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![
            format!("{}.svelte", module),
            format!("src/lib/{}.svelte", module),
            format!("src/routes/{}.svelte", module),
        ]
    }

    fn lang_key(&self) -> &'static str { "svelte" }

    fn is_stdlib_import(&self, import_name: &str, _project_root: &Path) -> bool {
        import_name == "svelte" ||
        import_name.starts_with("svelte/") ||
        import_name.starts_with("$app/") ||
        import_name.starts_with("$lib/")
    }

    fn find_stdlib(&self, _project_root: &Path) -> Option<PathBuf> { None }

    fn resolve_local_import(&self, import: &str, current_file: &Path, project_root: &Path) -> Option<PathBuf> {
        // Handle relative imports
        if import.starts_with('.') {
            if let Some(dir) = current_file.parent() {
                let candidates = [
                    import.to_string(),
                    format!("{}.svelte", import),
                    format!("{}/index.svelte", import),
                ];
                for c in &candidates {
                    let full = dir.join(c);
                    if full.is_file() {
                        return Some(full);
                    }
                }
            }
        }

        // Handle $lib alias (SvelteKit convention)
        if import.starts_with("$lib/") {
            let rest = import.strip_prefix("$lib/")?;
            let lib_dir = project_root.join("src/lib");
            let candidates = [
                rest.to_string(),
                format!("{}.svelte", rest),
                format!("{}.js", rest),
                format!("{}.ts", rest),
            ];
            for c in &candidates {
                let full = lib_dir.join(c);
                if full.is_file() {
                    return Some(full);
                }
            }
        }

        None
    }

    fn resolve_external_import(&self, _import_name: &str, _project_root: &Path) -> Option<ResolvedPackage> {
        // npm package resolution would go here
        None
    }

    fn get_version(&self, project_root: &Path) -> Option<String> {
        let pkg_json = project_root.join("package.json");
        if pkg_json.is_file() {
            if let Ok(content) = std::fs::read_to_string(&pkg_json) {
                // Look for svelte version in dependencies
                if let Some(idx) = content.find("\"svelte\"") {
                    let rest = &content[idx..];
                    if let Some(colon) = rest.find(':') {
                        let after = rest[colon+1..].trim();
                        if let Some(start) = after.find('"') {
                            let inner = &after[start+1..];
                            if let Some(end) = inner.find('"') {
                                return Some(inner[..end].to_string());
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn find_package_cache(&self, project_root: &Path) -> Option<PathBuf> {
        let node_modules = project_root.join("node_modules");
        if node_modules.is_dir() {
            return Some(node_modules);
        }
        None
    }

    fn indexable_extensions(&self) -> &'static [&'static str] { &["svelte"] }
    fn package_sources(&self, _project_root: &Path) -> Vec<crate::PackageSource> { Vec::new() }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{skip_dotfiles, has_extension};
        if skip_dotfiles(name) { return true; }
        if is_dir && (name == "node_modules" || name == ".svelte-kit" || name == "build") {
            return true;
        }
        !is_dir && !has_extension(name, &["svelte"])
    }

    fn discover_packages(&self, _source: &crate::PackageSource) -> Vec<(String, PathBuf)> { Vec::new() }

    fn package_module_name(&self, entry_name: &str) -> String {
        entry_name.strip_suffix(".svelte").unwrap_or(entry_name).to_string()
    }

    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() { Some(path.to_path_buf()) } else { None }
    }
}
