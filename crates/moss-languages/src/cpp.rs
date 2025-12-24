//! C++ language support.

use std::path::{Path, PathBuf};
use crate::{Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use crate::external_packages::ResolvedPackage;
use crate::c_cpp;
use moss_core::tree_sitter::Node;

/// C++ language support.
pub struct Cpp;

impl Language for Cpp {
    fn name(&self) -> &'static str { "C++" }
    fn extensions(&self) -> &'static [&'static str] { &["cpp", "cc", "cxx", "hpp", "hh", "hxx"] }
    fn grammar_name(&self) -> &'static str { "cpp" }

    fn container_kinds(&self) -> &'static [&'static str] { &["class_specifier", "struct_specifier"] }
    fn function_kinds(&self) -> &'static [&'static str] { &["function_definition"] }
    fn type_kinds(&self) -> &'static [&'static str] { &["class_specifier", "struct_specifier", "enum_specifier", "type_definition"] }
    fn import_kinds(&self) -> &'static [&'static str] { &["preproc_include"] }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["function_definition", "class_specifier", "struct_specifier"]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::HeaderBased // Also has public/private in classes, but header-based is primary
    }
    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &[
            "for_statement",
            "for_range_loop",
            "while_statement",
            "compound_statement",
            "lambda_expression",
            "namespace_definition",
        ]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &[
            "if_statement",
            "for_statement",
            "for_range_loop",
            "while_statement",
            "do_statement",
            "switch_statement",
            "return_statement",
            "break_statement",
            "continue_statement",
            "throw_statement",
            "goto_statement",
            "try_statement",
        ]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &[
            "if_statement",
            "for_statement",
            "for_range_loop",
            "while_statement",
            "do_statement",
            "switch_statement",
            "case_statement",
            "try_statement",
            "catch_clause",
            "throw_statement",
            "&&",
            "||",
            "conditional_expression",
        ]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &[
            "if_statement",
            "for_statement",
            "for_range_loop",
            "while_statement",
            "do_statement",
            "switch_statement",
            "try_statement",
            "function_definition",
            "class_specifier",
            "struct_specifier",
            "namespace_definition",
            "lambda_expression",
        ]
    }

    fn extract_function(&self, node: &Node, content: &str, in_container: bool) -> Option<Symbol> {
        let declarator = node.child_by_field_name("declarator")?;
        let name = find_identifier(&declarator, content)?;

        Some(Symbol {
            name: name.to_string(),
            kind: if in_container { SymbolKind::Method } else { SymbolKind::Function },
            signature: name.to_string(),
            docstring: None,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
            children: Vec::new(),
        })
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        let name = self.node_name(node, content)?;
        let kind = if node.kind() == "class_specifier" { SymbolKind::Class } else { SymbolKind::Struct };

        Some(Symbol {
            name: name.to_string(),
            kind,
            signature: format!("{} {}", kind.as_str(), name),
            docstring: None,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
            children: Vec::new(),
        })
    }

    // === Import Resolution ===

    fn lang_key(&self) -> &'static str { "cpp" }

    fn resolve_local_import(
        &self,
        include: &str,
        current_file: &Path,
        _project_root: &Path,
    ) -> Option<PathBuf> {
        // Strip quotes if present
        let header = include
            .trim_start_matches('"')
            .trim_end_matches('"')
            .trim_start_matches('<')
            .trim_end_matches('>');

        let current_dir = current_file.parent()?;

        // Try relative to current file's directory
        let relative = current_dir.join(header);
        if relative.is_file() {
            return Some(relative);
        }

        // Try with common extensions if none specified
        if !header.contains('.') {
            for ext in &[".h", ".hpp", ".hxx", ".hh"] {
                let with_ext = current_dir.join(format!("{}{}", header, ext));
                if with_ext.is_file() {
                    return Some(with_ext);
                }
            }
        }

        None
    }

    fn resolve_external_import(&self, include: &str, _project_root: &Path) -> Option<ResolvedPackage> {
        let include_paths = c_cpp::find_cpp_include_paths();
        c_cpp::resolve_cpp_include(include, &include_paths)
    }

    fn get_version(&self, _project_root: &Path) -> Option<String> {
        c_cpp::get_gcc_version()
    }

    fn indexable_extensions(&self) -> &'static [&'static str] {
        &["cpp", "hpp", "cc", "hh", "cxx", "hxx", "h"]
    }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::skip_dotfiles;
        if skip_dotfiles(name) { return true; }
        // Skip the "bits" directory (C++ internal headers)
        if is_dir && name == "bits" { return true; }
        if is_dir { return false; }
        // Check if it's a valid header: explicit extensions or extensionless stdlib headers
        let is_header = name.ends_with(".h")
            || name.ends_with(".hpp")
            || name.ends_with(".hxx")
            || name.ends_with(".hh")
            // C++ standard library headers (no extension, like vector, iostream)
            || (!name.contains('.') && !name.contains('-')
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        !is_header
    }
}

fn find_identifier<'a>(node: &Node, content: &'a str) -> Option<&'a str> {
    if node.kind() == "identifier" || node.kind() == "field_identifier" {
        return Some(&content[node.byte_range()]);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(id) = find_identifier(&child, content) {
            return Some(id);
        }
    }
    None
}
