//! F# language support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// F# language support.
pub struct FSharp;

impl Language for FSharp {
    fn name(&self) -> &'static str {
        "F#"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["fs", "fsi", "fsx"]
    }
    fn grammar_name(&self) -> &'static str {
        "fsharp"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["module_defn", "type_definition"]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_or_value_defn", "member_defn"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["type_definition", "record_type_defn", "union_type_defn"]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["import_decl"]
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["function_or_value_defn", "type_definition", "module_defn"]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::AccessModifier // public, private, internal
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
            "function_or_value_defn" => SymbolKind::Function,
            "member_defn" => SymbolKind::Method,
            "type_definition" | "record_type_defn" => SymbolKind::Struct,
            "union_type_defn" => SymbolKind::Enum,
            "module_defn" => SymbolKind::Module,
            _ => return Vec::new(),
        };

        vec![Export {
            name,
            kind,
            line: node.start_position().row + 1,
        }]
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &[
            "for_expression",
            "while_expression",
            "try_expression",
            "match_expression",
        ]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &[
            "if_expression",
            "match_expression",
            "for_expression",
            "while_expression",
            "try_expression",
            "application_expression",
        ]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &[
            "if_expression",
            "rule",
            "for_expression",
            "while_expression",
            "try_expression",
            "infix_expression",
        ]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &[
            "if_expression",
            "match_expression",
            "for_expression",
            "while_expression",
            "try_expression",
            "function_or_value_defn",
            "module_defn",
        ]
    }

    fn signature_suffix(&self) -> &'static str {
        ""
    }

    fn extract_function(&self, node: &Node, content: &str, _in_container: bool) -> Option<Symbol> {
        let name = self.node_name(node, content)?;

        // Extract first line as signature
        let text = &content[node.byte_range()];
        let first_line = text.lines().next().unwrap_or(text);

        let is_member = node.kind() == "member_defn";
        let kind = if is_member {
            SymbolKind::Method
        } else {
            SymbolKind::Function
        };

        Some(Symbol {
            name: name.to_string(),
            kind,
            signature: first_line.trim().to_string(),
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
        let (kind, keyword) = match node.kind() {
            "union_type_defn" => (SymbolKind::Enum, "type"),
            "record_type_defn" => (SymbolKind::Struct, "type"),
            "module_defn" => (SymbolKind::Module, "module"),
            _ => (SymbolKind::Struct, "type"),
        };

        Some(Symbol {
            name: name.to_string(),
            kind,
            signature: format!("{} {}", keyword, name),
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
        // F# uses /// for XML doc comments
        let mut prev = node.prev_sibling();
        let mut doc_lines = Vec::new();

        while let Some(sibling) = prev {
            let text = &content[sibling.byte_range()];
            if text.starts_with("///") {
                let line = text.strip_prefix("///").unwrap_or(text).trim();
                // Strip XML tags
                let clean = line
                    .replace("<summary>", "")
                    .replace("</summary>", "")
                    .replace("<param name=\"", "")
                    .replace("</param>", "")
                    .replace("<returns>", "")
                    .replace("</returns>", "")
                    .trim()
                    .to_string();
                if !clean.is_empty() {
                    doc_lines.push(clean);
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

    fn extract_attributes(&self, _node: &Node, _content: &str) -> Vec<String> {
        Vec::new()
    }

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        let text = &content[node.byte_range()];
        let line = node.start_position().row + 1;

        if let Some(rest) = text.strip_prefix("open ") {
            let module = rest.trim().to_string();
            return vec![Import {
                module,
                names: Vec::new(),
                alias: None,
                is_wildcard: true,
                is_relative: false,
                line,
            }];
        }

        Vec::new()
    }

    fn format_import(&self, import: &Import, _names: Option<&[&str]>) -> String {
        // F#: open Namespace
        format!("open {}", import.module)
    }

    fn is_public(&self, node: &Node, content: &str) -> bool {
        let text = &content[node.byte_range()];
        // F# defaults to public in modules
        !text.contains("private ") && !text.contains("internal ")
    }

    fn get_visibility(&self, node: &Node, content: &str) -> Visibility {
        let text = &content[node.byte_range()];
        if text.contains("private ") {
            Visibility::Private
        } else if text.contains("internal ") {
            Visibility::Protected // Using Protected for internal
        } else {
            Visibility::Public
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
            .or_else(|| node.child_by_field_name("identifier"))
            .map(|n| &content[n.byte_range()])
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        if ext != "fs" && ext != "fsi" && ext != "fsx" {
            return None;
        }
        let stem = path.file_stem()?.to_str()?;
        // F# typically uses PascalCase module names
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        let parts: Vec<&str> = module.split('.').collect();
        let file_name = parts.last().unwrap_or(&module);
        vec![format!("{}.fs", file_name), format!("src/{}.fs", file_name)]
    }

    fn lang_key(&self) -> &'static str {
        "fsharp"
    }

    fn is_stdlib_import(&self, import_name: &str, _project_root: &Path) -> bool {
        // .NET BCL namespaces
        import_name.starts_with("System")
            || import_name.starts_with("Microsoft.FSharp")
            || import_name.starts_with("FSharp.")
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
        let parts: Vec<&str> = import.split('.').collect();
        let file_name = parts.last()?;

        let paths = [format!("{}.fs", file_name), format!("src/{}.fs", file_name)];

        for p in &paths {
            let full = project_root.join(p);
            if full.is_file() {
                return Some(full);
            }
        }

        None
    }

    fn resolve_external_import(
        &self,
        _import_name: &str,
        _project_root: &Path,
    ) -> Option<ResolvedPackage> {
        // NuGet package resolution (similar to C#)
        None
    }

    fn get_version(&self, project_root: &Path) -> Option<String> {
        // Check .fsproj for version
        for entry in std::fs::read_dir(project_root).ok()? {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "fsproj")
                && let Ok(content) = std::fs::read_to_string(&path)
                && let Some(start) = content.find("<Version>")
            {
                let rest = &content[start + 9..];
                if let Some(end) = rest.find("</Version>") {
                    return Some(rest[..end].to_string());
                }
            }
        }
        None
    }

    fn find_package_cache(&self, _project_root: &Path) -> Option<PathBuf> {
        // NuGet cache
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            let cache = PathBuf::from(home).join(".nuget/packages");
            if cache.is_dir() {
                return Some(cache);
            }
        }
        None
    }

    fn indexable_extensions(&self) -> &'static [&'static str] {
        &["fs"]
    }
    fn package_sources(&self, _project_root: &Path) -> Vec<crate::PackageSource> {
        Vec::new()
    }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{has_extension, skip_dotfiles};
        if skip_dotfiles(name) {
            return true;
        }
        if is_dir && (name == "bin" || name == "obj" || name == "packages") {
            return true;
        }
        !is_dir && !has_extension(name, self.indexable_extensions())
    }

    fn discover_packages(&self, _source: &crate::PackageSource) -> Vec<(String, PathBuf)> {
        Vec::new()
    }

    fn package_module_name(&self, entry_name: &str) -> String {
        entry_name
            .strip_suffix(".fs")
            .or_else(|| entry_name.strip_suffix(".fsi"))
            .unwrap_or(entry_name)
            .to_string()
    }

    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() {
            return Some(path.to_path_buf());
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
            "access_modifier", "anon_record_expression", "anon_record_type",
            "anon_type_defn", "array_expression", "atomic_type", "begin_end_expression",
            "block_comment", "block_comment_content", "brace_expression",
            "ce_expression", "class_as_reference", "class_inherits_decl",
            "compound_type", "constrained_type", "declaration_expression",
            "delegate_type_defn", "do_expression", "dot_expression", "elif_expression",
            "enum_type_case", "enum_type_cases", "enum_type_defn",
            "exception_definition", "flexible_type", "format_string",
            "format_string_eval", "format_triple_quoted_string", "fun_expression",
            "function_declaration_left", "function_expression", "function_type",
            "generic_type", "identifier", "identifier_pattern", "index_expression", "interface_implementation",
            "interface_type_defn", "list_expression", "list_type", "literal_expression",
            "long_identifier", "long_identifier_or_op", "method_or_prop_defn",
            "module_abbrev", "mutate_expression", "named_module", "object_expression",
            "op_identifier", "paren_expression", "paren_type", "postfix_type",
            "prefixed_expression", "preproc_else", "preproc_if", "range_expression",
            "sequential_expression", "short_comp_expression", "simple_type",
            "static_type", "trait_member_constraint", "tuple_expression",
            "type_abbrev_defn", "type_argument", "type_argument_constraints",
            "type_argument_defn", "type_arguments", "type_attribute", "type_attributes",
            "type_check_pattern", "type_extension", "type_extension_elements",
            "type_name", "typed_expression", "typed_pattern", "typecast_expression",
            "types", "union_type_case", "union_type_cases", "union_type_field",
            "union_type_fields", "value_declaration", "value_declaration_left",
            "with_field_expression",
        ];
        validate_unused_kinds_audit(&FSharp, documented_unused)
            .expect("F# unused node kinds audit failed");
    }
}
