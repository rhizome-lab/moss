//! Swift language support.

use std::path::{Path, PathBuf};
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use crate::external_packages::ResolvedPackage;
use moss_core::tree_sitter::Node;

/// Swift language support.
pub struct Swift;

impl Language for Swift {
    fn name(&self) -> &'static str { "Swift" }
    fn extensions(&self) -> &'static [&'static str] { &["swift"] }
    fn grammar_name(&self) -> &'static str { "swift" }

    fn has_symbols(&self) -> bool { true }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["class_declaration", "protocol_declaration"]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_declaration", "init_declaration", "subscript_declaration",
          "computed_property", "lambda_literal"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["class_declaration", "protocol_declaration", "typealias_declaration"]
    }

    fn import_kinds(&self) -> &'static [&'static str] { &["import_declaration"] }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["class_declaration", "protocol_declaration", "function_declaration"]
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
            "struct_declaration" => SymbolKind::Struct,
            "protocol_declaration" => SymbolKind::Interface,
            "enum_declaration" => SymbolKind::Enum,
            "actor_declaration" => SymbolKind::Class,
            "function_declaration" | "init_declaration" => SymbolKind::Function,
            _ => return Vec::new(),
        };

        vec![Export {
            name,
            kind,
            line: node.start_position().row + 1,
        }]
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["for_statement", "while_statement", "repeat_while_statement",
          "do_statement", "catch_block", "switch_statement", "guard_statement"]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &["if_statement", "for_statement", "while_statement", "repeat_while_statement",
          "switch_statement", "guard_statement", "do_statement", "control_transfer_statement"]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &["if_statement", "for_statement", "while_statement", "repeat_while_statement",
          "switch_statement", "catch_block", "ternary_expression", "nil_coalescing_expression"]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &["if_statement", "for_statement", "while_statement", "repeat_while_statement",
          "switch_statement", "do_statement", "function_declaration", "class_declaration",
          "lambda_literal"]
    }

    fn extract_function(&self, node: &Node, content: &str, _in_container: bool) -> Option<Symbol> {
        let name = self.node_name(node, content)?;

        let params = node.child_by_field_name("parameters")
            .map(|p| content[p.byte_range()].to_string())
            .unwrap_or_else(|| "()".to_string());

        let return_type = node.child_by_field_name("return_type")
            .map(|t| format!(" -> {}", content[t.byte_range()].trim()));

        let signature = format!("func {}{}{}", name, params, return_type.unwrap_or_default());

        Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Function,
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
            "struct_declaration" => (SymbolKind::Struct, "struct"),
            "protocol_declaration" => (SymbolKind::Interface, "protocol"),
            "enum_declaration" => (SymbolKind::Enum, "enum"),
            "extension_declaration" => (SymbolKind::Module, "extension"),
            "actor_declaration" => (SymbolKind::Class, "actor"),
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
        if node.kind() == "typealias_declaration" {
            let name = self.node_name(node, content)?;
            let target = node.child_by_field_name("value")
                .map(|t| content[t.byte_range()].to_string())
                .unwrap_or_default();
            return Some(Symbol {
                name: name.to_string(),
                kind: SymbolKind::Type,
                signature: format!("typealias {} = {}", name, target),
                docstring: None,
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                visibility: self.get_visibility(node, content),
                children: Vec::new(),
            });
        }
        self.extract_container(node, content)
    }

    fn extract_docstring(&self, node: &Node, content: &str) -> Option<String> {
        // Swift uses /// or /** */ for documentation
        let mut prev = node.prev_sibling();
        let mut doc_lines = Vec::new();

        while let Some(sibling) = prev {
            let text = &content[sibling.byte_range()];
            if sibling.kind() == "comment" || sibling.kind() == "multiline_comment" {
                if text.starts_with("///") {
                    let line = text.strip_prefix("///").unwrap_or(text).trim();
                    if !line.is_empty() {
                        doc_lines.insert(0, line.to_string());
                    }
                } else if text.starts_with("/**") {
                    let inner = text
                        .strip_prefix("/**").unwrap_or(text)
                        .strip_suffix("*/").unwrap_or(text);
                    for line in inner.lines() {
                        let clean = line.trim().strip_prefix("*").unwrap_or(line).trim();
                        if !clean.is_empty() {
                            doc_lines.push(clean.to_string());
                        }
                    }
                    break;
                } else {
                    break;
                }
            } else {
                break;
            }
            prev = sibling.prev_sibling();
        }

        if doc_lines.is_empty() {
            None
        } else {
            Some(doc_lines.join(" "))
        }
    }

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        if node.kind() != "import_declaration" {
            return Vec::new();
        }

        let line = node.start_position().row + 1;

        // Get the module name
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" || child.kind() == "simple_identifier" {
                let module = content[child.byte_range()].to_string();
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

        Vec::new()
    }

    fn is_public(&self, node: &Node, content: &str) -> bool {
        self.get_visibility(node, content) == Visibility::Public
    }

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
        if path.extension()?.to_str()? != "swift" {
            return None;
        }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![
            format!("{}.swift", module),
            format!("Sources/{}.swift", module),
            format!("Sources/{}/{}.swift", module, module),
        ]
    }

    fn is_stdlib_import(&self, import_name: &str, _project_root: &Path) -> bool {
        matches!(import_name, "Foundation" | "UIKit" | "AppKit" | "SwiftUI" |
                 "Combine" | "CoreData" | "CoreGraphics" | "CoreFoundation" |
                 "Darwin" | "Dispatch" | "ObjectiveC" | "Swift")
    }

    fn find_stdlib(&self, _project_root: &Path) -> Option<PathBuf> {
        None
    }

    fn get_visibility(&self, node: &Node, content: &str) -> Visibility {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "modifiers" || child.kind() == "modifier" {
                let mod_text = &content[child.byte_range()];
                if mod_text.contains("private") || mod_text.contains("fileprivate") {
                    return Visibility::Private;
                }
                if mod_text.contains("internal") {
                    return Visibility::Protected;
                }
                if mod_text.contains("public") || mod_text.contains("open") {
                    return Visibility::Public;
                }
            }
        }
        // Swift default is internal
        Visibility::Protected
    }

    fn embedded_content(&self, _node: &Node, _content: &str) -> Option<crate::EmbeddedBlock> { None }

    fn lang_key(&self) -> &'static str { "swift" }

    fn resolve_local_import(
        &self,
        import: &str,
        _current_file: &Path,
        project_root: &Path,
    ) -> Option<PathBuf> {
        // Swift Package Manager structure
        let paths = [
            format!("Sources/{}/{}.swift", import, import),
            format!("Sources/{}.swift", import),
            format!("{}.swift", import),
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
        // Swift Package Manager resolution would go here
        None
    }

    fn get_version(&self, project_root: &Path) -> Option<String> {
        // Check Package.swift for swift-tools-version
        let package_swift = project_root.join("Package.swift");
        if package_swift.is_file() {
            if let Ok(content) = std::fs::read_to_string(&package_swift) {
                if let Some(line) = content.lines().next() {
                    if line.contains("swift-tools-version:") {
                        let version = line.split(':').nth(1)?.trim();
                        return Some(version.to_string());
                    }
                }
            }
        }
        None
    }

    fn find_package_cache(&self, _project_root: &Path) -> Option<PathBuf> {
        // Swift Package Manager cache
        if let Ok(home) = std::env::var("HOME") {
            let cache = PathBuf::from(home).join("Library/Caches/org.swift.swiftpm");
            if cache.is_dir() {
                return Some(cache);
            }
        }
        None
    }

    fn indexable_extensions(&self) -> &'static [&'static str] {
        &["swift"]
    }

    fn package_sources(&self, _project_root: &Path) -> Vec<crate::PackageSource> {
        Vec::new()
    }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{skip_dotfiles, has_extension};
        if skip_dotfiles(name) { return true; }
        if is_dir && (name == "build" || name == ".build" || name == "Pods") {
            return true;
        }
        !is_dir && !has_extension(name, &["swift"])
    }

    fn discover_packages(&self, _source: &crate::PackageSource) -> Vec<(String, PathBuf)> {
        Vec::new()
    }

    fn package_module_name(&self, entry_name: &str) -> String {
        entry_name.strip_suffix(".swift").unwrap_or(entry_name).to_string()
    }

    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() {
            return Some(path.to_path_buf());
        }
        None
    }
}
