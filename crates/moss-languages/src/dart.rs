//! Dart language support.

use std::path::{Path, PathBuf};
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use crate::external_packages::ResolvedPackage;
use moss_core::tree_sitter::Node;

/// Dart language support.
pub struct Dart;

impl Language for Dart {
    fn name(&self) -> &'static str { "Dart" }
    fn extensions(&self) -> &'static [&'static str] { &["dart"] }
    fn grammar_name(&self) -> &'static str { "dart" }

    fn has_symbols(&self) -> bool { true }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["class_definition", "enum_declaration", "mixin_declaration", "extension_declaration"]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_signature", "method_signature", "function_body", "getter_signature", "setter_signature"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["class_definition", "enum_declaration", "mixin_declaration", "type_alias"]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["import_specification", "library_export"]
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["class_definition", "function_signature", "method_signature", "enum_declaration"]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::NamingConvention // _ prefix = private
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        let name = match self.node_name(node, content) {
            Some(n) => n,
            None => return Vec::new(),
        };

        // _ prefix means private
        if name.starts_with('_') {
            return Vec::new();
        }

        let kind = match node.kind() {
            "class_definition" => SymbolKind::Class,
            "enum_declaration" => SymbolKind::Enum,
            "mixin_declaration" => SymbolKind::Class,
            "function_signature" | "function_body" => SymbolKind::Function,
            "method_signature" => SymbolKind::Method,
            _ => return Vec::new(),
        };

        vec![Export {
            name: name.to_string(),
            kind,
            line: node.start_position().row + 1,
        }]
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["block", "for_statement", "while_statement", "do_statement", "switch_statement", "try_statement"]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &["if_statement", "for_statement", "while_statement", "do_statement",
          "switch_statement", "try_statement", "return_statement", "break_statement",
          "continue_statement", "throw_expression", "rethrow_expression"]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &["if_statement", "for_statement", "while_statement", "do_statement",
          "switch_statement_case", "catch_clause", "conditional_expression", "logical_and_expression", "logical_or_expression"]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &["if_statement", "for_statement", "while_statement", "do_statement",
          "switch_statement", "try_statement", "function_body", "class_definition",
          "function_expression"]
    }

    fn extract_function(&self, node: &Node, content: &str, _in_container: bool) -> Option<Symbol> {
        let name = self.node_name(node, content)?;

        let return_type = node.child_by_field_name("return_type")
            .map(|t| content[t.byte_range()].to_string());

        let params = node.child_by_field_name("formal_parameters")
            .or_else(|| node.child_by_field_name("parameters"))
            .map(|p| content[p.byte_range()].to_string())
            .unwrap_or_else(|| "()".to_string());

        let is_method = node.kind().contains("method");
        let kind = if is_method { SymbolKind::Method } else { SymbolKind::Function };

        let signature = if let Some(ret) = return_type {
            format!("{} {}{}", ret, name, params)
        } else {
            format!("{}{}", name, params)
        };

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
            "enum_declaration" => (SymbolKind::Enum, "enum"),
            "mixin_declaration" => (SymbolKind::Class, "mixin"),
            "extension_declaration" => (SymbolKind::Class, "extension"),
            _ => (SymbolKind::Class, "class"),
        };

        // Check for abstract
        let is_abstract = node.parent()
            .map(|p| {
                let text = &content[p.byte_range()];
                text.contains("abstract ")
            })
            .unwrap_or(false);

        let prefix = if is_abstract { format!("abstract {}", keyword) } else { keyword.to_string() };

        Some(Symbol {
            name: name.to_string(),
            kind,
            signature: format!("{} {}", prefix, name),
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
        // Dart uses /// for doc comments
        let mut prev = node.prev_sibling();
        let mut doc_lines = Vec::new();

        while let Some(sibling) = prev {
            let text = &content[sibling.byte_range()];
            if sibling.kind() == "documentation_comment" || text.starts_with("///") {
                let line = text.strip_prefix("///").unwrap_or(text).trim();
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
        if node.kind() != "import_specification" && node.kind() != "library_export" {
            return Vec::new();
        }

        let text = &content[node.byte_range()];
        let line = node.start_position().row + 1;

        // Extract the import URI
        if let Some(start) = text.find('\'').or_else(|| text.find('"')) {
            let quote = text.chars().nth(start).unwrap();
            let rest = &text[start + 1..];
            if let Some(end) = rest.find(quote) {
                let module = rest[..end].to_string();
                let is_relative = module.starts_with('.') || module.starts_with('/');

                // Check for 'as' alias
                let alias = if text.contains(" as ") {
                    text.split(" as ")
                        .nth(1)
                        .and_then(|s| s.split(';').next())
                        .map(|s| s.trim().to_string())
                } else {
                    None
                };

                return vec![Import {
                    module,
                    names: Vec::new(),
                    alias,
                    is_wildcard: text.contains(" show ") || text.contains(" hide "),
                    is_relative,
                    line,
                }];
            }
        }

        Vec::new()
    }

    fn is_public(&self, node: &Node, content: &str) -> bool {
        if let Some(name) = self.node_name(node, content) {
            !name.starts_with('_')
        } else {
            true
        }
    }

    fn get_visibility(&self, node: &Node, content: &str) -> Visibility {
        if self.is_public(node, content) {
            Visibility::Public
        } else {
            Visibility::Private
        }
    }

    fn embedded_content(&self, _node: &Node, _content: &str) -> Option<crate::EmbeddedBlock> { None }

    fn container_body<'a>(&self, node: &'a Node<'a>) -> Option<Node<'a>> {
        node.child_by_field_name("body")
    }

    fn body_has_docstring(&self, _body: &Node, _content: &str) -> bool { false }

    fn node_name<'a>(&self, node: &Node, content: &'a str) -> Option<&'a str> {
        node.child_by_field_name("name")
            .map(|n| &content[n.byte_range()])
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        if ext != "dart" { return None; }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![
            format!("lib/{}.dart", module),
            format!("lib/src/{}.dart", module),
            format!("{}.dart", module),
        ]
    }

    fn lang_key(&self) -> &'static str { "dart" }

    fn is_stdlib_import(&self, import_name: &str, _project_root: &Path) -> bool {
        import_name.starts_with("dart:")
    }

    fn find_stdlib(&self, _project_root: &Path) -> Option<PathBuf> { None }

    fn resolve_local_import(&self, import: &str, current_file: &Path, project_root: &Path) -> Option<PathBuf> {
        // Handle package: imports
        if import.starts_with("package:") {
            let path_part = import.strip_prefix("package:")?;
            let parts: Vec<&str> = path_part.splitn(2, '/').collect();
            if parts.len() == 2 {
                // Look in lib/
                let full = project_root.join("lib").join(parts[1]);
                if full.is_file() {
                    return Some(full);
                }
            }
            return None;
        }

        // Relative imports
        if import.starts_with('.') || import.starts_with('/') {
            if let Some(dir) = current_file.parent() {
                let full = dir.join(import);
                if full.is_file() {
                    return Some(full);
                }
            }
        }

        None
    }

    fn resolve_external_import(&self, _import_name: &str, _project_root: &Path) -> Option<ResolvedPackage> {
        // pub.dev package resolution would go here
        None
    }

    fn get_version(&self, project_root: &Path) -> Option<String> {
        let pubspec = project_root.join("pubspec.yaml");
        if pubspec.is_file() {
            if let Ok(content) = std::fs::read_to_string(&pubspec) {
                for line in content.lines() {
                    if line.starts_with("version:") {
                        return Some(line.strip_prefix("version:")?.trim().to_string());
                    }
                }
            }
        }
        None
    }

    fn find_package_cache(&self, project_root: &Path) -> Option<PathBuf> {
        // Check .dart_tool/package_config.json for package locations
        let packages = project_root.join(".dart_tool/package_config.json");
        if packages.is_file() {
            return Some(project_root.join(".dart_tool"));
        }
        None
    }

    fn indexable_extensions(&self) -> &'static [&'static str] { &["dart"] }
    fn package_sources(&self, _project_root: &Path) -> Vec<crate::PackageSource> { Vec::new() }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{skip_dotfiles, has_extension};
        if skip_dotfiles(name) { return true; }
        if is_dir && (name == "build" || name == ".dart_tool" || name == ".pub-cache") {
            return true;
        }
        !is_dir && !has_extension(name, &["dart"])
    }

    fn discover_packages(&self, _source: &crate::PackageSource) -> Vec<(String, PathBuf)> { Vec::new() }

    fn package_module_name(&self, entry_name: &str) -> String {
        entry_name.strip_suffix(".dart").unwrap_or(entry_name).to_string()
    }

    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() {
            return Some(path.to_path_buf());
        }
        // Look for lib/<name>.dart
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            let lib = path.join("lib").join(format!("{}.dart", name));
            if lib.is_file() {
                return Some(lib);
            }
        }
        None
    }
}
