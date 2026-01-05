//! Zig language support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// Zig language support.
pub struct Zig;

impl Language for Zig {
    fn name(&self) -> &'static str {
        "Zig"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["zig"]
    }
    fn grammar_name(&self) -> &'static str {
        "zig"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["ContainerDecl"]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["FnProto", "TestDecl"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["ContainerDecl"]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["SuffixExpr"] // @import("module") is a builtin call suffix
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["FnProto", "ContainerDecl"]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::ExplicitExport // pub keyword
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        if !self.is_public(node, content) {
            return Vec::new();
        }

        let name = match self.node_name(node, content) {
            Some(n) => n.to_string(),
            None => return Vec::new(),
        };

        let kind = match node.kind() {
            "FnProto" | "TestDecl" => SymbolKind::Function,
            "ContainerDecl" => SymbolKind::Struct, // Could be struct/enum/union
            _ => return Vec::new(),
        };

        vec![Export {
            name,
            kind,
            line: node.start_position().row + 1,
        }]
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["Block", "ForStatement", "WhileStatement"]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &[
            "IfStatement",
            "ForStatement",
            "WhileStatement",
            "SwitchExpr",
        ]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &[
            "IfStatement",
            "ForStatement",
            "WhileStatement",
            "SwitchExpr",
            "ErrorUnionExpr",
            "BinaryExpr",
        ]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &[
            "IfStatement",
            "ForStatement",
            "WhileStatement",
            "SwitchExpr",
            "FnProto",
            "ContainerDecl",
        ]
    }

    fn signature_suffix(&self) -> &'static str {
        ""
    }

    fn extract_function(&self, node: &Node, content: &str, _in_container: bool) -> Option<Symbol> {
        let name = self.node_name(node, content)?;

        let params = node
            .child_by_field_name("parameters")
            .map(|p| content[p.byte_range()].to_string())
            .unwrap_or_else(|| "()".to_string());

        let return_type = node
            .child_by_field_name("return_type")
            .map(|t| content[t.byte_range()].to_string());

        let is_pub = self.is_public(node, content);
        let prefix = if is_pub { "pub fn" } else { "fn" };

        let signature = if let Some(ret) = return_type {
            format!("{} {}{} {}", prefix, name, params, ret)
        } else {
            format!("{} {}{}", prefix, name, params)
        };

        Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Function,
            signature,
            docstring: self.extract_docstring(node, content),
            attributes: Vec::new(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: self.get_visibility(node, content),
            children: Vec::new(),
            is_interface_impl: false,
            implements: Vec::new(),
        })
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        let name = self.node_name(node, content)?;

        // Detect struct/enum/union from ContainerDeclType child's first token
        let mut cursor = node.walk();
        let mut kind = SymbolKind::Struct;
        let mut keyword = "struct";
        for child in node.children(&mut cursor) {
            if child.kind() == "ContainerDeclType" {
                // First child of ContainerDeclType is the keyword token
                if let Some(keyword_node) = child.child(0) {
                    let kw = &content[keyword_node.byte_range()];
                    if kw == "enum" {
                        kind = SymbolKind::Enum;
                        keyword = "enum";
                    } else if kw == "union" {
                        keyword = "union";
                    }
                }
                break;
            }
        }

        let is_pub = self.is_public(node, content);
        let prefix = if is_pub {
            format!("pub {}", keyword)
        } else {
            keyword.to_string()
        };

        Some(Symbol {
            name: name.to_string(),
            kind,
            signature: format!("{} {}", prefix, name),
            docstring: self.extract_docstring(node, content),
            attributes: Vec::new(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: self.get_visibility(node, content),
            children: Vec::new(),
            is_interface_impl: false,
            implements: Vec::new(),
        })
    }

    fn extract_type(&self, node: &Node, content: &str) -> Option<Symbol> {
        self.extract_container(node, content)
    }

    fn extract_docstring(&self, node: &Node, content: &str) -> Option<String> {
        // Zig uses /// for doc comments
        let mut prev = node.prev_sibling();
        let mut doc_lines = Vec::new();

        while let Some(sibling) = prev {
            let text = &content[sibling.byte_range()];
            if sibling.kind() == "doc_comment" || text.starts_with("///") {
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

    fn extract_attributes(&self, _node: &Node, _content: &str) -> Vec<String> {
        Vec::new()
    }

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        // Look for @import("module")
        if node.kind() != "builtin_call_expression" {
            return Vec::new();
        }

        let text = &content[node.byte_range()];
        if !text.starts_with("@import") {
            return Vec::new();
        }

        // Extract the string argument
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "string_literal" {
                let module = content[child.byte_range()].trim_matches('"').to_string();
                let is_relative = module.starts_with('.');
                return vec![Import {
                    module,
                    names: Vec::new(),
                    alias: None,
                    is_wildcard: false,
                    is_relative,
                    line: node.start_position().row + 1,
                }];
            }
        }

        Vec::new()
    }

    fn format_import(&self, import: &Import, _names: Option<&[&str]>) -> String {
        // Zig: @import("module")
        format!("@import(\"{}\")", import.module)
    }

    fn is_public(&self, node: &Node, content: &str) -> bool {
        // Check for pub keyword before the declaration
        if let Some(prev) = node.prev_sibling() {
            let text = &content[prev.byte_range()];
            if text == "pub" {
                return true;
            }
        }
        // Also check if node starts with pub
        let text = &content[node.byte_range()];
        text.starts_with("pub ")
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
        if ext != "zig" {
            return None;
        }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![format!("{}.zig", module)]
    }

    fn lang_key(&self) -> &'static str {
        "zig"
    }

    fn is_stdlib_import(&self, import_name: &str, _project_root: &Path) -> bool {
        import_name == "std" || import_name == "builtin"
    }

    fn find_stdlib(&self, _project_root: &Path) -> Option<PathBuf> {
        // Could look for zig installation
        None
    }

    fn resolve_local_import(
        &self,
        import: &str,
        current_file: &Path,
        project_root: &Path,
    ) -> Option<PathBuf> {
        if !import.ends_with(".zig") {
            return None;
        }

        // Relative imports
        if import.starts_with('.') {
            if let Some(dir) = current_file.parent() {
                let full = dir.join(import);
                if full.is_file() {
                    return Some(full);
                }
            }
        }

        // Absolute path from project root
        let full = project_root.join(import);
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
        // Zig package manager resolution would go here
        None
    }

    fn get_version(&self, project_root: &Path) -> Option<String> {
        // Check build.zig.zon for version
        let zon = project_root.join("build.zig.zon");
        if zon.is_file() {
            if let Ok(content) = std::fs::read_to_string(&zon) {
                // Quick parse for .version = "x.y.z"
                for line in content.lines() {
                    if line.contains(".version") && line.contains('"') {
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

    fn find_package_cache(&self, _project_root: &Path) -> Option<PathBuf> {
        None
    }
    fn indexable_extensions(&self) -> &'static [&'static str] {
        &["zig"]
    }
    fn package_sources(&self, _project_root: &Path) -> Vec<crate::PackageSource> {
        Vec::new()
    }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{has_extension, skip_dotfiles};
        if skip_dotfiles(name) {
            return true;
        }
        if is_dir && name == "zig-cache" {
            return true;
        }
        !is_dir && !has_extension(name, self.indexable_extensions())
    }

    fn discover_packages(&self, _source: &crate::PackageSource) -> Vec<(String, PathBuf)> {
        Vec::new()
    }

    fn package_module_name(&self, entry_name: &str) -> String {
        entry_name
            .strip_suffix(".zig")
            .unwrap_or(entry_name)
            .to_string()
    }

    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() {
            return Some(path.to_path_buf());
        }
        // Check for src/main.zig or src/root.zig
        for name in &["src/main.zig", "src/root.zig", "main.zig"] {
            let entry = path.join(name);
            if entry.is_file() {
                return Some(entry);
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
            // Zig grammar uses PascalCase node kinds
            "ArrayTypeStart", "BUILTINIDENTIFIER", "BitShiftOp", "BlockExpr",
            "BlockExprStatement", "BlockLabel", "BuildinTypeExpr", "ContainerDeclType",
            "ForArgumentsList", "ForExpr", "ForItem", "ForPrefix", "ForTypeExpr",
            "FormatSequence", "IDENTIFIER", "IfExpr", "IfPrefix", "IfTypeExpr",
            "LabeledStatement", "LabeledTypeExpr", "LoopExpr", "LoopStatement",
            "LoopTypeExpr", "ParamType", "PrefixTypeOp", "PtrTypeStart",
            "SliceTypeStart", "Statement", "SwitchCase", "WhileContinueExpr",
            "WhileExpr", "WhilePrefix", "WhileTypeExpr",
        ];
        validate_unused_kinds_audit(&Zig, documented_unused)
            .expect("Zig unused node kinds audit failed");
    }
}
