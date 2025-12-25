//! Fish shell language support.

use std::path::{Path, PathBuf};
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use crate::external_packages::ResolvedPackage;
use arborium::tree_sitter::Node;

/// Fish shell language support.
pub struct Fish;

impl Language for Fish {
    fn name(&self) -> &'static str { "Fish" }
    fn extensions(&self) -> &'static [&'static str] { &["fish"] }
    fn grammar_name(&self) -> &'static str { "fish" }

    fn has_symbols(&self) -> bool { true }

    fn container_kinds(&self) -> &'static [&'static str] { &[] }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_definition"]
    }

    fn type_kinds(&self) -> &'static [&'static str] { &[] }
    fn import_kinds(&self) -> &'static [&'static str] { &["command"] } // source command

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["function_definition"]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::AllPublic
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        if node.kind() != "function_definition" {
            return Vec::new();
        }

        let name = match self.node_name(node, content) {
            Some(n) => n.to_string(),
            None => return Vec::new(),
        };

        vec![Export {
            name,
            kind: SymbolKind::Function,
            line: node.start_position().row + 1,
        }]
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["function_definition", "begin_statement"]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &["if_statement", "while_statement", "for_statement", "switch_statement"]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &["if_statement", "else_if_clause", "while_statement", "for_statement",
          "switch_statement", "case_clause"]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &["function_definition", "if_statement", "while_statement", "for_statement"]
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

    fn extract_container(&self, _node: &Node, _content: &str) -> Option<Symbol> { None }
    fn extract_type(&self, _node: &Node, _content: &str) -> Option<Symbol> { None }

    fn extract_docstring(&self, node: &Node, content: &str) -> Option<String> {
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

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        if node.kind() != "command" {
            return Vec::new();
        }

        let text = &content[node.byte_range()];
        if !text.starts_with("source ") {
            return Vec::new();
        }

        let module = text.strip_prefix("source ")
            .map(|s| s.trim().to_string());

        if let Some(module) = module {
            return vec![Import {
                module,
                names: Vec::new(),
                alias: None,
                is_wildcard: false,
                is_relative: true,
                line: node.start_position().row + 1,
            }];
        }

        Vec::new()
    }

    fn is_public(&self, _node: &Node, _content: &str) -> bool { true }
    fn get_visibility(&self, _node: &Node, _content: &str) -> Visibility { Visibility::Public }

    fn embedded_content(&self, _node: &Node, _content: &str) -> Option<crate::EmbeddedBlock> { None }

    fn container_body<'a>(&self, _node: &'a Node<'a>) -> Option<Node<'a>> { None }
    fn body_has_docstring(&self, _body: &Node, _content: &str) -> bool { false }

    fn node_name<'a>(&self, node: &Node, content: &'a str) -> Option<&'a str> {
        node.child_by_field_name("name")
            .map(|n| &content[n.byte_range()])
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        if ext != "fish" { return None; }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![
            format!("{}.fish", module),
            format!("functions/{}.fish", module),
        ]
    }

    fn lang_key(&self) -> &'static str { "fish" }

    fn is_stdlib_import(&self, _import_name: &str, _project_root: &Path) -> bool { false }
    fn find_stdlib(&self, _project_root: &Path) -> Option<PathBuf> { None }
    fn resolve_local_import(&self, import: &str, current_file: &Path, _: &Path) -> Option<PathBuf> {
        let dir = current_file.parent()?;
        let full = dir.join(import);
        if full.is_file() { Some(full) } else { None }
    }
    fn resolve_external_import(&self, _: &str, _: &Path) -> Option<ResolvedPackage> { None }
    fn get_version(&self, _: &Path) -> Option<String> { None }

    fn find_package_cache(&self, _project_root: &Path) -> Option<PathBuf> {
        if let Some(home) = std::env::var_os("HOME") {
            let config = PathBuf::from(home).join(".config/fish/functions");
            if config.is_dir() {
                return Some(config);
            }
        }
        None
    }

    fn indexable_extensions(&self) -> &'static [&'static str] { &["fish"] }
    fn package_sources(&self, _: &Path) -> Vec<crate::PackageSource> { Vec::new() }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{skip_dotfiles, has_extension};
        if skip_dotfiles(name) { return true; }
        !is_dir && !has_extension(name, &["fish"])
    }

    fn discover_packages(&self, _: &crate::PackageSource) -> Vec<(String, PathBuf)> { Vec::new() }

    fn package_module_name(&self, entry_name: &str) -> String {
        entry_name.strip_suffix(".fish").unwrap_or(entry_name).to_string()
    }

    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() { Some(path.to_path_buf()) } else { None }
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
            "else_clause", "negated_statement", "redirect_statement", "return",
        ];
        validate_unused_kinds_audit(&Fish, documented_unused)
            .expect("Fish unused node kinds audit failed");
    }
}
