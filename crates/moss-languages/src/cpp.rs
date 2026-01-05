//! C++ language support.

use crate::c_cpp;
use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// C++ language support.
pub struct Cpp;

impl Language for Cpp {
    fn name(&self) -> &'static str {
        "C++"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["cpp", "cc", "cxx", "hpp", "hh", "hxx"]
    }
    fn grammar_name(&self) -> &'static str {
        "cpp"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["class_specifier", "struct_specifier"]
    }
    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_definition"]
    }
    fn type_kinds(&self) -> &'static [&'static str] {
        &[
            "class_specifier",
            "struct_specifier",
            "enum_specifier",
            "type_definition",
        ]
    }
    fn import_kinds(&self) -> &'static [&'static str] {
        &["preproc_include"]
    }

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

    fn signature_suffix(&self) -> &'static str {
        " {}"
    }

    fn extract_function(&self, node: &Node, content: &str, in_container: bool) -> Option<Symbol> {
        let declarator = node.child_by_field_name("declarator")?;
        let name = find_identifier(&declarator, content)?;

        Some(Symbol {
            name: name.to_string(),
            kind: if in_container {
                SymbolKind::Method
            } else {
                SymbolKind::Function
            },
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

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        let name = self.node_name(node, content)?;
        let kind = if node.kind() == "class_specifier" {
            SymbolKind::Class
        } else {
            SymbolKind::Struct
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

    fn extract_type(&self, node: &Node, content: &str) -> Option<Symbol> {
        self.extract_container(node, content)
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
        // C++ uses #include, no multi-imports
        if import.module.starts_with('<') || import.module.ends_with('>') {
            format!("#include {}", import.module)
        } else {
            format!("#include \"{}\"", import.module)
        }
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        let kind = match node.kind() {
            "function_definition" => SymbolKind::Function,
            "class_specifier" => SymbolKind::Class,
            "struct_specifier" => SymbolKind::Struct,
            _ => return Vec::new(),
        };

        if let Some(name) = self.node_name(node, content) {
            vec![Export {
                name: name.to_string(),
                kind,
                line: node.start_position().row + 1,
            }]
        } else {
            Vec::new()
        }
    }

    fn is_public(&self, _node: &Node, _content: &str) -> bool {
        true // Header-based visibility
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
        if let Some(name_node) = node.child_by_field_name("name") {
            return Some(&content[name_node.byte_range()]);
        }
        if let Some(declarator) = node.child_by_field_name("declarator") {
            return find_identifier(&declarator, content);
        }
        None
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        if !["cpp", "cc", "cxx", "hpp", "hh", "hxx", "h"].contains(&ext) {
            return None;
        }
        Some(path.to_string_lossy().to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![module.to_string()]
    }

    fn is_stdlib_import(&self, include: &str, _project_root: &Path) -> bool {
        // C++ standard library headers (no extension)
        let stdlib = [
            "iostream",
            "vector",
            "string",
            "map",
            "set",
            "algorithm",
            "memory",
            "utility",
            "functional",
            "iterator",
            "numeric",
            "cstdio",
            "cstdlib",
            "cstring",
            "cmath",
            "climits",
        ];
        stdlib.contains(&include)
    }

    fn find_package_cache(&self, _project_root: &Path) -> Option<PathBuf> {
        None
    }

    fn find_stdlib(&self, _project_root: &Path) -> Option<PathBuf> {
        c_cpp::find_cpp_include_paths().into_iter().next()
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
        "cpp"
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
            for ext in &[".h", ".hpp", ".hxx", ".hh"] {
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
        &["cpp", "hpp", "cc", "hh", "cxx", "hxx", "h"]
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

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::skip_dotfiles;
        if skip_dotfiles(name) {
            return true;
        }
        // Skip the "bits" directory (C++ internal headers)
        if is_dir && name == "bits" {
            return true;
        }
        if is_dir {
            return false;
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate_unused_kinds_audit;

    /// Documents node kinds that exist in the C++ grammar but aren't used in trait methods.
    /// Run `cross_check_node_kinds` in registry.rs to see all potentially useful kinds.
    #[test]
    fn unused_node_kinds_audit() {
        #[rustfmt::skip]
        let documented_unused: &[&str] = &[
            // STRUCTURAL (C++ adds many to C)
            "access_specifier",        // public:, private:
            "base_class_clause",       // : public Base
            "bitfield_clause",         // : width
            "condition_clause",        // if condition
            "declaration",             // declaration
            "declaration_list",        // decl list
            "default_method_clause",   // = default
            "delete_method_clause",    // = delete
            "dependent_type",          // typename T::X
            "destructor_name",         // ~Foo
            "enumerator",              // enum value
            "enumerator_list",         // enum body
            "field_declaration",       // struct field
            "field_declaration_list",  // struct body
            "field_expression",        // foo.bar
            "field_identifier",        // field name
            "identifier",              // too common
            "init_statement",          // for init
            "linkage_specification",   // extern "C"
            "module_name",             // module name
            "module_partition",        // module partition
            "namespace_identifier",    // namespace name
            "nested_namespace_specifier", // ns1::ns2
            "operator_name",           // operator+
            "parameter_declaration",   // param decl
            "primitive_type",          // int, char
            "pure_virtual_clause",     // = 0
            "ref_qualifier",           // &, &&
            "sized_type_specifier",    // unsigned int
            "statement_identifier",    // label name
            "static_assert_declaration", // static_assert
            "storage_class_specifier", // static, extern
            "structured_binding_declarator", // auto [a, b]
            "type_descriptor",         // type desc
            "type_identifier",         // type name
            "type_parameter_declaration", // template param
            "type_qualifier",          // const, volatile
            "union_specifier",         // union
            "using_declaration",       // using ns::name
            "variadic_parameter_declaration", // T...
            "variadic_type_parameter_declaration", // typename...
            "virtual_specifier",       // override, final

            // CLAUSE
            "else_clause",             // else
            "noexcept",                // noexcept

            // EXPRESSION (C++ adds many to C)
            "alignof_expression",      // alignof(T)
            "assignment_expression",   // x = y
            "binary_expression",       // a + b
            "call_expression",         // foo()
            "cast_expression",         // (T)x
            "co_await_expression",     // co_await x
            "co_return_statement",     // co_return
            "co_yield_statement",      // co_yield x
            "comma_expression",        // a, b
            "compound_literal_expression", // (T){...}
            "delete_expression",       // delete x
            "extension_expression",    // __extension__
            "fold_expression",         // (... + args)
            "generic_expression",      // _Generic
            "gnu_asm_expression",      // asm()
            "new_expression",          // new T
            "offsetof_expression",     // offsetof
            "parenthesized_expression",// (expr)
            "pointer_expression",      // *p, &x
            "reflect_expression",      // reflexpr
            "sizeof_expression",       // sizeof(T)
            "splice_expression",       // [:expr:]
            "subscript_expression",    // arr[i]
            "unary_expression",        // -x, !x
            "update_expression",       // x++

            // TEMPLATE
            "template_declaration",    // template<>
            "template_function",       // template func
            "template_method",         // template method
            "template_template_parameter_declaration", // template template
            "template_type",           // T<U>

            // LAMBDA
            "lambda_capture_initializer", // [x = y]
            "lambda_capture_specifier",   // [=], [&]
            "lambda_declarator",       // lambda params
            "lambda_default_capture",  // =, &
            "lambda_specifier",        // mutable

            // FUNCTION
            "abstract_function_declarator", // abstract func
            "explicit_function_specifier", // explicit
            "explicit_object_parameter_declaration", // this param
            "function_declarator",     // func decl
            "operator_cast",           // operator T()
            "optional_parameter_declaration", // param = default
            "optional_type_parameter_declaration", // T = U
            "placeholder_type_specifier", // auto, decltype(auto)
            "pointer_type_declarator", // ptr declarator
            "trailing_return_type",    // -> T

            // CONCEPTS/REQUIRES
            "concept_definition",      // concept
            "requires_clause",         // requires
            "requires_expression",     // requires {}
            "type_requirement",        // typename T

            // MODULE
            "export_declaration",      // export
            "global_module_fragment_declaration", // module;
            "import_declaration",      // import
            "module_declaration",      // module
            "private_module_fragment_declaration", // module :private

            // PREPROCESSOR
            "preproc_elif",            // #elif
            "preproc_elifdef",         // #elifdef
            "preproc_else",            // #else
            "preproc_function_def",    // function macro
            "preproc_if",              // #if
            "preproc_ifdef",           // #ifdef

            // SPLICE
            "splice_specifier",        // [: :] specifier
            "splice_type_specifier",   // [: :] type

            // OTHER
            "alias_declaration",       // using X = Y
            "alignas_qualifier",       // alignas
            "attribute_declaration",   // [[attr]]
            "attribute_specifier",     // __attribute__
            "attributed_statement",    // stmt with attr
            "consteval_block_declaration", // consteval
            "decltype",                // decltype
            "expansion_statement",     // pack expansion stmt
            "expression_statement",    // expr;
            "friend_declaration",      // friend
            "gnu_asm_qualifier",       // asm qualifiers
            "labeled_statement",       // label:
            "namespace_alias_definition", // namespace X = Y
            "qualified_identifier",    // ns::name
            "throw_specifier",         // throw()

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

        validate_unused_kinds_audit(&Cpp, documented_unused)
            .expect("C++ unused node kinds audit failed");
    }
}
