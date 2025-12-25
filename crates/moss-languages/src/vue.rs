//! Vue language support.

use std::path::{Path, PathBuf};
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use crate::external_packages::ResolvedPackage;
use moss_core::tree_sitter::Node;

/// Vue language support.
pub struct Vue;

impl Language for Vue {
    fn name(&self) -> &'static str { "Vue" }
    fn extensions(&self) -> &'static [&'static str] { &["vue"] }
    fn grammar_name(&self) -> &'static str { "vue" }

    fn has_symbols(&self) -> bool { true }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["script_element", "template_element", "style_element"]
    }
    fn function_kinds(&self) -> &'static [&'static str] {
        &[] // JS functions are in embedded script, not Vue grammar
    }
    fn type_kinds(&self) -> &'static [&'static str] { &[] }
    fn import_kinds(&self) -> &'static [&'static str] {
        &[] // JS imports are in embedded script, not Vue grammar
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &[] // JS exports are in embedded script, not Vue grammar
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::ExplicitExport
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["element"] // Vue template elements create scope
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &["directive_attribute"] // v-if, v-for, v-show are directives
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &["directive_attribute", "interpolation"]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &["element", "template_element", "script_element"]
    }

    fn extract_function(&self, node: &Node, content: &str, _in_container: bool) -> Option<Symbol> {
        let name = self.node_name(node, content)?;
        Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Function,
            signature: format!("function {}", name),
            docstring: None,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
            children: Vec::new(),
        })
    }

    fn extract_container(&self, _node: &Node, _content: &str) -> Option<Symbol> {
        None
    }

    fn extract_type(&self, _node: &Node, _content: &str) -> Option<Symbol> { None }
    fn extract_docstring(&self, _node: &Node, _content: &str) -> Option<String> { None }
    fn extract_imports(&self, _node: &Node, _content: &str) -> Vec<Import> { Vec::new() }
    fn extract_public_symbols(&self, _node: &Node, _content: &str) -> Vec<Export> { Vec::new() }

    fn is_public(&self, _node: &Node, _content: &str) -> bool { true }
    fn get_visibility(&self, _node: &Node, _content: &str) -> Visibility { Visibility::Public }

    fn embedded_content(&self, node: &Node, content: &str) -> Option<crate::EmbeddedBlock> {
        match node.kind() {
            "script_element" => {
                let raw = find_raw_text_child(node)?;
                let grammar = detect_script_lang(node, content);
                Some(crate::EmbeddedBlock {
                    grammar,
                    content: content[raw.byte_range()].to_string(),
                    start_line: raw.start_position().row + 1,
                })
            }
            "style_element" => {
                let raw = find_raw_text_child(node)?;
                let grammar = detect_style_lang(node, content);
                Some(crate::EmbeddedBlock {
                    grammar,
                    content: content[raw.byte_range()].to_string(),
                    start_line: raw.start_position().row + 1,
                })
            }
            _ => None,
        }
    }

    fn container_body<'a>(&self, node: &'a Node<'a>) -> Option<Node<'a>> { node.child_by_field_name("body") }
    fn body_has_docstring(&self, _body: &Node, _content: &str) -> bool { false }

    fn node_name<'a>(&self, node: &Node, content: &'a str) -> Option<&'a str> {
        let name_node = node.child_by_field_name("name")?;
        Some(&content[name_node.byte_range()])
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        if path.extension()?.to_str()? != "vue" { return None; }
        Some(path.to_string_lossy().to_string())
    }
    fn module_name_to_paths(&self, module: &str) -> Vec<String> { vec![format!("{}.vue", module)] }

    fn lang_key(&self) -> &'static str { "vue" }
    fn resolve_local_import(&self, _: &str, _: &Path, _: &Path) -> Option<PathBuf> { None }
    fn resolve_external_import(&self, _: &str, _: &Path) -> Option<ResolvedPackage> { None }
    fn is_stdlib_import(&self, _: &str, _: &Path) -> bool { false }
    fn get_version(&self, _: &Path) -> Option<String> { None }
    fn find_package_cache(&self, _: &Path) -> Option<PathBuf> { None }
    fn indexable_extensions(&self) -> &'static [&'static str] { &["vue"] }
    fn find_stdlib(&self, _: &Path) -> Option<PathBuf> { None }
    fn package_module_name(&self, name: &str) -> String { name.strip_suffix(".vue").unwrap_or(name).to_string() }
    fn package_sources(&self, _: &Path) -> Vec<crate::PackageSource> { Vec::new() }
    fn discover_packages(&self, _: &crate::PackageSource) -> Vec<(String, PathBuf)> { Vec::new() }
    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() { Some(path.to_path_buf()) } else { None }
    }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{skip_dotfiles, has_extension};
        if skip_dotfiles(name) { return true; }
        if is_dir && name == "node_modules" { return true; }
        !is_dir && !has_extension(name, &["vue"])
    }
}

/// Find the raw_text child of a script/style element.
fn find_raw_text_child<'a>(node: &'a Node<'a>) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "raw_text" {
            return Some(child);
        }
    }
    None
}

/// Detect script language from the lang attribute (e.g., <script lang="ts">).
fn detect_script_lang(node: &Node, content: &str) -> &'static str {
    if let Some(lang) = get_lang_attribute(node, content) {
        match lang {
            "ts" | "typescript" => return "typescript",
            "tsx" => return "tsx",
            _ => {}
        }
    }
    "javascript"
}

/// Detect style language from the lang attribute (e.g., <style lang="scss">).
fn detect_style_lang(node: &Node, content: &str) -> &'static str {
    if let Some(lang) = get_lang_attribute(node, content) {
        match lang {
            "scss" | "sass" => return "scss",
            "less" => return "css", // No less grammar, fall back to CSS
            _ => {}
        }
    }
    "css"
}

/// Get the lang attribute value from a script/style element.
fn get_lang_attribute<'a>(node: &Node, content: &'a str) -> Option<&'a str> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        // Look for start_tag which contains the attributes
        if child.kind() == "start_tag" {
            let mut inner_cursor = child.walk();
            for attr in child.children(&mut inner_cursor) {
                if attr.kind() == "attribute" {
                    // Check if this is a lang attribute
                    let mut attr_cursor = attr.walk();
                    let mut is_lang = false;
                    for part in attr.children(&mut attr_cursor) {
                        if part.kind() == "attribute_name" {
                            let name = &content[part.byte_range()];
                            is_lang = name == "lang";
                        } else if is_lang && part.kind() == "quoted_attribute_value" {
                            // Get the value inside quotes
                            let value = &content[part.byte_range()];
                            return Some(value.trim_matches('"').trim_matches('\''));
                        }
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate_unused_kinds_audit;

    #[test]
    fn unused_node_kinds_audit() {
        #[rustfmt::skip]
        let documented_unused: &[&str] = &[
            "directive_modifier", "directive_modifiers", "doctype",
        ];

        validate_unused_kinds_audit(&Vue, documented_unused)
            .expect("Vue unused node kinds audit failed");
    }
}
