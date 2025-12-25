//! PHP language support.

use std::path::{Path, PathBuf};
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use crate::external_packages::ResolvedPackage;
use moss_core::tree_sitter::Node;

/// PHP language support.
pub struct Php;

impl Language for Php {
    fn name(&self) -> &'static str { "PHP" }
    fn extensions(&self) -> &'static [&'static str] { &["php", "phtml"] }
    fn grammar_name(&self) -> &'static str { "php" }

    fn has_symbols(&self) -> bool { true }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["class_declaration", "interface_declaration", "trait_declaration",
          "enum_declaration", "namespace_definition"]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_definition", "method_declaration", "arrow_function"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["class_declaration", "interface_declaration", "trait_declaration", "enum_declaration"]
    }

    fn import_kinds(&self) -> &'static [&'static str] { &["namespace_use_declaration"] }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["class_declaration", "interface_declaration", "trait_declaration",
          "function_definition", "method_declaration"]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::AccessModifier
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        if self.get_visibility(node, content) != Visibility::Public {
            return Vec::new();
        }

        let name = match self.node_name(node, content) {
            Some(n) => n.to_string(),
            None => return Vec::new(),
        };

        let kind = match node.kind() {
            "class_declaration" => SymbolKind::Class,
            "interface_declaration" => SymbolKind::Interface,
            "trait_declaration" => SymbolKind::Class, // traits are like mixins
            "enum_declaration" => SymbolKind::Enum,
            "function_definition" => SymbolKind::Function,
            "method_declaration" => SymbolKind::Method,
            _ => return Vec::new(),
        };

        vec![Export {
            name,
            kind,
            line: node.start_position().row + 1,
        }]
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["for_statement", "foreach_statement", "while_statement", "do_statement",
          "try_statement", "catch_clause", "switch_statement"]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &["if_statement", "for_statement", "foreach_statement", "while_statement",
          "do_statement", "switch_statement", "try_statement", "return_statement",
          "break_statement", "continue_statement", "throw_expression"]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &["if_statement", "for_statement", "foreach_statement", "while_statement",
          "do_statement", "case_statement", "catch_clause", "conditional_expression"]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &["if_statement", "for_statement", "foreach_statement", "while_statement",
          "do_statement", "switch_statement", "try_statement", "function_definition",
          "method_declaration", "class_declaration", "arrow_function"]
    }

    fn extract_function(&self, node: &Node, content: &str, _in_container: bool) -> Option<Symbol> {
        let name = self.node_name(node, content)?;

        let params = node.child_by_field_name("parameters")
            .map(|p| content[p.byte_range()].to_string())
            .unwrap_or_else(|| "()".to_string());

        let return_type = node.child_by_field_name("return_type")
            .map(|t| format!(": {}", content[t.byte_range()].trim()));

        let kind = if node.kind() == "method_declaration" {
            SymbolKind::Method
        } else {
            SymbolKind::Function
        };

        let signature = format!("function {}{}{}", name, params, return_type.unwrap_or_default());

        Some(Symbol {
            name: name.to_string(),
            kind,
            signature,
            docstring: self.extract_docstring(node, content),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: self.get_visibility(node, content),
            children: Vec::new(),
        })
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        let name = self.node_name(node, content)?;
        let (kind, keyword) = match node.kind() {
            "interface_declaration" => (SymbolKind::Interface, "interface"),
            "trait_declaration" => (SymbolKind::Class, "trait"),
            "enum_declaration" => (SymbolKind::Enum, "enum"),
            "namespace_definition" => (SymbolKind::Module, "namespace"),
            _ => (SymbolKind::Class, "class"),
        };

        Some(Symbol {
            name: name.to_string(),
            kind,
            signature: format!("{} {}", keyword, name),
            docstring: self.extract_docstring(node, content),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: self.get_visibility(node, content),
            children: Vec::new(),
        })
    }

    fn extract_type(&self, node: &Node, content: &str) -> Option<Symbol> {
        self.extract_container(node, content)
    }

    fn extract_docstring(&self, node: &Node, content: &str) -> Option<String> {
        // PHP uses /** */ for PHPDoc comments
        let mut prev = node.prev_sibling();
        while let Some(sibling) = prev {
            let text = &content[sibling.byte_range()];
            if sibling.kind() == "comment" {
                if text.starts_with("/**") {
                    let inner = text
                        .strip_prefix("/**").unwrap_or(text)
                        .strip_suffix("*/").unwrap_or(text);
                    let lines: Vec<&str> = inner.lines()
                        .map(|l| l.trim().strip_prefix("*").unwrap_or(l).trim())
                        .filter(|l| !l.is_empty() && !l.starts_with('@'))
                        .collect();
                    if !lines.is_empty() {
                        return Some(lines.join(" "));
                    }
                }
                break;
            } else if sibling.kind() != "text" {
                break;
            }
            prev = sibling.prev_sibling();
        }
        None
    }

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        if node.kind() != "namespace_use_declaration" {
            return Vec::new();
        }

        let line = node.start_position().row + 1;
        let mut imports = Vec::new();

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "namespace_use_clause" {
                let text = content[child.byte_range()].to_string();
                imports.push(Import {
                    module: text,
                    names: Vec::new(),
                    alias: None,
                    is_wildcard: false,
                    is_relative: false,
                    line,
                });
            }
        }

        imports
    }

    fn is_public(&self, node: &Node, content: &str) -> bool {
        self.get_visibility(node, content) == Visibility::Public
    }

    fn embedded_content(&self, _node: &Node, _content: &str) -> Option<crate::EmbeddedBlock> { None }

    fn container_body<'a>(&self, node: &'a Node<'a>) -> Option<Node<'a>> {
        node.child_by_field_name("body")
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
        if ext != "php" && ext != "phtml" {
            return None;
        }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        let path = module.replace('\\', "/");
        vec![
            format!("{}.php", path),
            format!("src/{}.php", path),
            format!("app/{}.php", path),
        ]
    }

    fn is_stdlib_import(&self, _import_name: &str, _project_root: &Path) -> bool {
        // PHP doesn't have namespaced stdlib in the same way
        false
    }

    fn find_stdlib(&self, _project_root: &Path) -> Option<PathBuf> {
        None
    }

    fn get_visibility(&self, node: &Node, content: &str) -> Visibility {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "visibility_modifier" {
                let mod_text = &content[child.byte_range()];
                if mod_text == "private" { return Visibility::Private; }
                if mod_text == "protected" { return Visibility::Protected; }
                if mod_text == "public" { return Visibility::Public; }
            }
        }
        // PHP default visibility for methods/properties in classes is public
        Visibility::Public
    }

    fn lang_key(&self) -> &'static str { "php" }

    fn resolve_local_import(
        &self,
        import: &str,
        _current_file: &Path,
        project_root: &Path,
    ) -> Option<PathBuf> {
        // PSR-4 style: namespace maps to directory structure
        let path_part = import.replace('\\', "/");

        let paths = [
            format!("{}.php", path_part),
            format!("src/{}.php", path_part),
            format!("app/{}.php", path_part),
            format!("lib/{}.php", path_part),
        ];

        for path in &paths {
            let full_path = project_root.join(path);
            if full_path.is_file() {
                return Some(full_path);
            }
        }

        None
    }

    fn resolve_external_import(&self, _import_name: &str, _project_root: &Path) -> Option<ResolvedPackage> {
        // Composer package resolution would go here
        None
    }

    fn get_version(&self, project_root: &Path) -> Option<String> {
        // Check composer.json for PHP version requirement
        let composer = project_root.join("composer.json");
        if composer.is_file() {
            if let Ok(content) = std::fs::read_to_string(&composer) {
                // Quick parse for "php": "^8.1" or similar
                if let Some(idx) = content.find("\"php\"") {
                    let rest = &content[idx..];
                    if let Some(start) = rest.find(':') {
                        let after_colon = rest[start+1..].trim();
                        if let Some(ver_start) = after_colon.find('"') {
                            let ver_rest = &after_colon[ver_start+1..];
                            if let Some(ver_end) = ver_rest.find('"') {
                                return Some(ver_rest[..ver_end].to_string());
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn find_package_cache(&self, project_root: &Path) -> Option<PathBuf> {
        // Composer vendor directory
        let vendor = project_root.join("vendor");
        if vendor.is_dir() {
            return Some(vendor);
        }
        None
    }

    fn indexable_extensions(&self) -> &'static [&'static str] {
        &["php"]
    }

    fn package_sources(&self, _project_root: &Path) -> Vec<crate::PackageSource> {
        Vec::new()
    }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{skip_dotfiles, has_extension};
        if skip_dotfiles(name) { return true; }
        if is_dir && (name == "vendor" || name == "cache" || name == "tests") {
            return true;
        }
        !is_dir && !has_extension(name, &["php"])
    }

    fn discover_packages(&self, _source: &crate::PackageSource) -> Vec<(String, PathBuf)> {
        Vec::new()
    }

    fn package_module_name(&self, entry_name: &str) -> String {
        entry_name.strip_suffix(".php").unwrap_or(entry_name).to_string()
    }

    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() {
            return Some(path.to_path_buf());
        }
        None
    }
}
