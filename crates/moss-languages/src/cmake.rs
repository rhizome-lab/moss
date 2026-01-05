//! CMake language support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// CMake language support.
pub struct CMake;

impl Language for CMake {
    fn name(&self) -> &'static str {
        "CMake"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["cmake"]
    }
    fn grammar_name(&self) -> &'static str {
        "cmake"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["function_def", "macro_def"]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_def", "macro_def"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &[]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["normal_command"] // include(), find_package()
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["function_def", "macro_def"]
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
            "function_def" | "macro_def" => SymbolKind::Function,
            _ => return Vec::new(),
        };

        vec![Export {
            name,
            kind,
            line: node.start_position().row + 1,
        }]
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["function_def", "macro_def"]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &["if_condition", "foreach_loop", "while_loop"]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &[
            "if_condition",
            "elseif_command",
            "foreach_loop",
            "while_loop",
        ]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &["function_def", "macro_def", "if_condition", "foreach_loop"]
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
            visibility: Visibility::Public,
            children: Vec::new(),
            is_interface_impl: false,
            implements: Vec::new(),
        })
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        self.extract_function(node, content, false)
    }

    fn extract_type(&self, _node: &Node, _content: &str) -> Option<Symbol> {
        None
    }

    fn extract_docstring(&self, node: &Node, content: &str) -> Option<String> {
        // CMake uses # for comments
        let mut prev = node.prev_sibling();
        let mut doc_lines = Vec::new();

        while let Some(sibling) = prev {
            let text = &content[sibling.byte_range()];
            if sibling.kind() == "line_comment" {
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
        if node.kind() != "normal_command" {
            return Vec::new();
        }

        let text = &content[node.byte_range()];
        let line = node.start_position().row + 1;

        // include(file), find_package(pkg)
        if text.starts_with("include(") || text.starts_with("find_package(") {
            let inner = text
                .split('(')
                .nth(1)
                .and_then(|s| s.split(')').next())
                .map(|s| s.trim().to_string());

            if let Some(module) = inner {
                return vec![Import {
                    module,
                    names: Vec::new(),
                    alias: None,
                    is_wildcard: false,
                    is_relative: text.starts_with("include("),
                    line,
                }];
            }
        }

        Vec::new()
    }

    fn format_import(&self, import: &Import, _names: Option<&[&str]>) -> String {
        // CMake: include(file) or find_package(pkg)
        format!("include({})", import.module)
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

    fn container_body<'a>(&self, node: &'a Node<'a>) -> Option<Node<'a>> {
        node.child_by_field_name("body")
    }

    fn body_has_docstring(&self, _body: &Node, _content: &str) -> bool {
        false
    }

    fn node_name<'a>(&self, node: &Node, content: &'a str) -> Option<&'a str> {
        // function(name args...) - name is first argument
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "argument" {
                return Some(&content[child.byte_range()]);
            }
        }
        None
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let name = path.file_name()?.to_str()?;
        if name == "CMakeLists.txt" || name.ends_with(".cmake") {
            let stem = path.file_stem()?.to_str()?;
            return Some(stem.to_string());
        }
        None
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![
            format!("{}.cmake", module),
            format!("cmake/{}.cmake", module),
        ]
    }

    fn lang_key(&self) -> &'static str {
        "cmake"
    }

    fn is_stdlib_import(&self, _import_name: &str, _project_root: &Path) -> bool {
        false
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
        let candidates = [
            project_root.join("cmake").join(format!("{}.cmake", import)),
            project_root.join(format!("{}.cmake", import)),
        ];
        for c in &candidates {
            if c.is_file() {
                return Some(c.clone());
            }
        }
        None
    }

    fn resolve_external_import(
        &self,
        _import_name: &str,
        _project_root: &Path,
    ) -> Option<ResolvedPackage> {
        None
    }

    fn get_version(&self, project_root: &Path) -> Option<String> {
        if project_root.join("CMakeLists.txt").is_file() {
            return Some("cmake".to_string());
        }
        None
    }

    fn find_package_cache(&self, _project_root: &Path) -> Option<PathBuf> {
        None
    }
    fn indexable_extensions(&self) -> &'static [&'static str] {
        &["cmake"]
    }
    fn package_sources(&self, _project_root: &Path) -> Vec<crate::PackageSource> {
        Vec::new()
    }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::skip_dotfiles;
        if skip_dotfiles(name) {
            return true;
        }
        if is_dir && name == "build" {
            return true;
        }
        !is_dir && !name.ends_with(".cmake") && name != "CMakeLists.txt"
    }

    fn discover_packages(&self, _source: &crate::PackageSource) -> Vec<(String, PathBuf)> {
        Vec::new()
    }

    fn package_module_name(&self, entry_name: &str) -> String {
        entry_name
            .strip_suffix(".cmake")
            .unwrap_or(entry_name)
            .to_string()
    }

    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() {
            return Some(path.to_path_buf());
        }
        let cmakelists = path.join("CMakeLists.txt");
        if cmakelists.is_file() {
            return Some(cmakelists);
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
            "block", "block_command", "block_def", "body", "else", "else_command",
            "elseif", "endblock", "endblock_command", "endforeach", "endforeach_command",
            "endfunction", "endfunction_command", "endif", "endif_command", "endwhile",
            "endwhile_command", "foreach", "foreach_command", "function",
            "function_command", "identifier", "if", "if_command", "while",
            "while_command",
        ];
        validate_unused_kinds_audit(&CMake, documented_unused)
            .expect("CMake unused node kinds audit failed");
    }
}
