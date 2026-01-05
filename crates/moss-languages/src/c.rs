//! C language support.

use crate::c_cpp;
use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// C language support.
pub struct C;

impl Language for C {
    fn name(&self) -> &'static str {
        "C"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["c", "h"]
    }
    fn grammar_name(&self) -> &'static str {
        "c"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &[]
    } // C doesn't have containers
    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_definition"]
    }
    fn type_kinds(&self) -> &'static [&'static str] {
        &["struct_specifier", "enum_specifier", "type_definition"]
    }
    fn import_kinds(&self) -> &'static [&'static str] {
        &["preproc_include"]
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["function_definition"]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::HeaderBased
    }
    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["for_statement", "while_statement", "compound_statement"]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &[
            "if_statement",
            "for_statement",
            "while_statement",
            "do_statement",
            "switch_statement",
            "return_statement",
            "break_statement",
            "continue_statement",
            "goto_statement",
        ]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &[
            "if_statement",
            "for_statement",
            "while_statement",
            "do_statement",
            "switch_statement",
            "case_statement",
            "&&",
            "||",
            "conditional_expression",
        ]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &[
            "if_statement",
            "for_statement",
            "while_statement",
            "do_statement",
            "switch_statement",
            "function_definition",
        ]
    }

    fn signature_suffix(&self) -> &'static str {
        " {}"
    }

    fn extract_function(&self, node: &Node, content: &str, _in_container: bool) -> Option<Symbol> {
        let declarator = node.child_by_field_name("declarator")?;
        let name = self.find_identifier(&declarator, content)?;

        Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Function,
            signature: name.to_string(),
            docstring: None,
            attributes: Vec::new(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
            children: Vec::new(),
            is_interface_impl: false,
            implements: Vec::new(),
        })
    }

    fn extract_container(&self, _node: &Node, _content: &str) -> Option<Symbol> {
        None // C doesn't have containers in the same sense
    }

    fn extract_type(&self, node: &Node, content: &str) -> Option<Symbol> {
        let name = self.node_name(node, content)?;
        let kind = match node.kind() {
            "struct_specifier" => SymbolKind::Struct,
            "enum_specifier" => SymbolKind::Enum,
            _ => SymbolKind::Type,
        };

        Some(Symbol {
            name: name.to_string(),
            kind,
            signature: format!("{} {}", kind.as_str(), name),
            docstring: None,
            attributes: Vec::new(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
            children: Vec::new(),
            is_interface_impl: false,
            implements: Vec::new(),
        })
    }

    fn extract_docstring(&self, _node: &Node, _content: &str) -> Option<String> {
        None
    }

    fn extract_attributes(&self, _node: &Node, _content: &str) -> Vec<String> {
        Vec::new()
    }

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        if node.kind() != "preproc_include" {
            return Vec::new();
        }

        let line = node.start_position().row + 1;
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "string_literal" || child.kind() == "system_lib_string" {
                let text = &content[child.byte_range()];
                let module = text
                    .trim_matches(|c| c == '"' || c == '<' || c == '>')
                    .to_string();
                let is_relative = text.starts_with('"');
                return vec![Import {
                    module,
                    names: Vec::new(),
                    alias: None,
                    is_wildcard: false,
                    is_relative,
                    line,
                }];
            }
        }
        Vec::new()
    }

    fn format_import(&self, import: &Import, _names: Option<&[&str]>) -> String {
        // C doesn't have multi-imports; each #include is a single header
        if import.module.starts_with('<') || import.module.ends_with('>') {
            format!("#include {}", import.module)
        } else {
            format!("#include \"{}\"", import.module)
        }
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        if node.kind() != "function_definition" {
            return Vec::new();
        }

        if let Some(name) = self.node_name(node, content) {
            vec![Export {
                name: name.to_string(),
                kind: SymbolKind::Function,
                line: node.start_position().row + 1,
            }]
        } else {
            Vec::new()
        }
    }

    fn is_public(&self, _node: &Node, _content: &str) -> bool {
        true // C doesn't have visibility modifiers
    }

    fn get_visibility(&self, _node: &Node, _content: &str) -> Visibility {
        Visibility::Public
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
        // Try "name" field first
        if let Some(name_node) = node.child_by_field_name("name") {
            return Some(&content[name_node.byte_range()]);
        }
        // For functions, look in the declarator
        if let Some(declarator) = node.child_by_field_name("declarator") {
            return self.find_identifier(&declarator, content);
        }
        None
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        if !["c", "h"].contains(&ext) {
            return None;
        }
        Some(path.to_string_lossy().to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![module.to_string()]
    }

    fn is_stdlib_import(&self, include: &str, _project_root: &Path) -> bool {
        // Standard C headers
        let stdlib = [
            "stdio.h", "stdlib.h", "string.h", "math.h", "time.h", "ctype.h", "errno.h", "float.h",
            "limits.h", "locale.h", "setjmp.h", "signal.h", "stdarg.h", "stddef.h", "assert.h",
        ];
        stdlib.contains(&include)
    }

    fn find_package_cache(&self, _project_root: &Path) -> Option<PathBuf> {
        None // C uses include paths, not a package cache
    }

    fn find_stdlib(&self, _project_root: &Path) -> Option<PathBuf> {
        // Return the first include path as stdlib location
        c_cpp::find_cpp_include_paths().into_iter().next()
    }

    fn package_sources(&self, _project_root: &Path) -> Vec<crate::PackageSource> {
        use crate::{PackageSource, PackageSourceKind};
        c_cpp::find_cpp_include_paths()
            .into_iter()
            .map(|path| PackageSource {
                name: "includes",
                path,
                kind: PackageSourceKind::Recursive,
                version_specific: false,
            })
            .collect()
    }

    fn package_module_name(&self, entry_name: &str) -> String {
        entry_name.to_string()
    }

    fn discover_packages(&self, source: &crate::PackageSource) -> Vec<(String, PathBuf)> {
        self.discover_recursive_packages(&source.path, &source.path)
    }

    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() {
            Some(path.to_path_buf())
        } else {
            None
        }
    }

    // === Import Resolution ===

    fn lang_key(&self) -> &'static str {
        "c"
    }

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
            for ext in &[".h", ".c"] {
                let with_ext = current_dir.join(format!("{}{}", header, ext));
                if with_ext.is_file() {
                    return Some(with_ext);
                }
            }
        }

        None
    }

    fn resolve_external_import(
        &self,
        include: &str,
        _project_root: &Path,
    ) -> Option<ResolvedPackage> {
        let include_paths = c_cpp::find_cpp_include_paths();
        c_cpp::resolve_cpp_include(include, &include_paths)
    }

    fn get_version(&self, _project_root: &Path) -> Option<String> {
        c_cpp::get_gcc_version()
    }

    fn indexable_extensions(&self) -> &'static [&'static str] {
        &["c", "h"]
    }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{has_extension, skip_dotfiles};
        if skip_dotfiles(name) {
            return true;
        }
        !is_dir && !has_extension(name, self.indexable_extensions())
    }
}

impl C {
    fn find_identifier<'a>(&self, node: &Node, content: &'a str) -> Option<&'a str> {
        if node.kind() == "identifier" {
            return Some(&content[node.byte_range()]);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(id) = self.find_identifier(&child, content) {
                return Some(id);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate_unused_kinds_audit;

    /// Documents node kinds that exist in the C grammar but aren't used in trait methods.
    /// Run `cross_check_node_kinds` in registry.rs to see all potentially useful kinds.
    #[test]
    fn unused_node_kinds_audit() {
        #[rustfmt::skip]
        let documented_unused: &[&str] = &[
            // STRUCTURAL
            "bitfield_clause",         // : width
            "declaration",             // declaration
            "declaration_list",        // decl list
            "enumerator",              // enum value
            "enumerator_list",         // enum body
            "field_declaration",       // struct field
            "field_declaration_list",  // struct body
            "field_expression",        // foo.bar
            "field_identifier",        // field name
            "identifier",              // too common
            "linkage_specification",   // extern "C"
            "parameter_declaration",   // param decl
            "primitive_type",          // int, char
            "sized_type_specifier",    // unsigned int
            "statement_identifier",    // label name
            "storage_class_specifier", // static, extern
            "type_descriptor",         // type desc
            "type_identifier",         // type name
            "type_qualifier",          // const, volatile
            "union_specifier",         // union

            // CLAUSE
            "else_clause",             // else

            // EXPRESSION
            "alignof_expression",      // alignof(T)
            "assignment_expression",   // x = y
            "binary_expression",       // a + b
            "call_expression",         // foo()
            "cast_expression",         // (T)x
            "comma_expression",        // a, b
            "compound_literal_expression", // (T){...}
            "extension_expression",    // __extension__
            "generic_expression",      // _Generic
            "gnu_asm_expression",      // asm()
            "offsetof_expression",     // offsetof
            "parenthesized_expression",// (expr)
            "pointer_expression",      // *p, &x
            "sizeof_expression",       // sizeof(T)
            "subscript_expression",    // arr[i]
            "unary_expression",        // -x, !x
            "update_expression",       // x++

            // FUNCTION
            "abstract_function_declarator", // abstract func
            "function_declarator",     // func decl

            // PREPROCESSOR
            "preproc_elif",            // #elif
            "preproc_elifdef",         // #elifdef
            "preproc_else",            // #else
            "preproc_function_def",    // function macro
            "preproc_if",              // #if
            "preproc_ifdef",           // #ifdef

            // OTHER
            "alignas_qualifier",       // alignas
            "attribute_declaration",   // [[attr]]
            "attribute_specifier",     // __attribute__
            "attributed_statement",    // stmt with attr
            "expression_statement",    // expr;
            "gnu_asm_qualifier",       // asm qualifiers
            "labeled_statement",       // label:
            "macro_type_specifier",    // macro type

            // MS EXTENSIONS
            "ms_based_modifier",       // __based
            "ms_call_modifier",        // __cdecl
            "ms_declspec_modifier",    // __declspec
            "ms_pointer_modifier",     // __ptr32
            "ms_restrict_modifier",    // __restrict
            "ms_signed_ptr_modifier",  // __sptr
            "ms_unaligned_ptr_modifier", // __unaligned
            "ms_unsigned_ptr_modifier", // __uptr

            // SEH
            "seh_except_clause",       // __except
            "seh_finally_clause",      // __finally
            "seh_leave_statement",     // __leave
            "seh_try_statement",       // __try
        ];

        validate_unused_kinds_audit(&C, documented_unused)
            .expect("C unused node kinds audit failed");
    }
}
