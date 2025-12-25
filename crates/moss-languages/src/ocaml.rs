//! OCaml language support.

use std::path::{Path, PathBuf};
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use crate::external_packages::ResolvedPackage;
use moss_core::tree_sitter::Node;

/// OCaml language support.
pub struct OCaml;

impl Language for OCaml {
    fn name(&self) -> &'static str { "OCaml" }
    fn extensions(&self) -> &'static [&'static str] { &["ml", "mli"] }
    fn grammar_name(&self) -> &'static str { "ocaml" }

    fn has_symbols(&self) -> bool { true }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["module_definition", "module_type_definition", "type_definition"]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["value_definition", "let_binding"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["type_definition"]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["open_statement"]
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["value_definition", "type_definition", "module_definition"]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::ExplicitExport // .mli interface files
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        let name = match self.node_name(node, content) {
            Some(n) => n.to_string(),
            None => return Vec::new(),
        };

        let kind = match node.kind() {
            "value_definition" | "let_binding" => SymbolKind::Function,
            "type_definition" => SymbolKind::Type,
            "module_definition" => SymbolKind::Module,
            "module_type_definition" => SymbolKind::Interface,
            _ => return Vec::new(),
        };

        vec![Export {
            name,
            kind,
            line: node.start_position().row + 1,
        }]
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["let_expression", "function_expression", "match_expression"]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &["if_expression", "match_expression", "try_expression"]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &["if_expression", "match_expression", "match_case"]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &["let_expression", "module_definition", "match_expression"]
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
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
            children: Vec::new(),
        })
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        let name = self.node_name(node, content)?;

        let (kind, keyword) = match node.kind() {
            "module_definition" => (SymbolKind::Module, "module"),
            "module_type_definition" => (SymbolKind::Interface, "module type"),
            "type_definition" => (SymbolKind::Type, "type"),
            _ => return None,
        };

        Some(Symbol {
            name: name.to_string(),
            kind,
            signature: format!("{} {}", keyword, name),
            docstring: self.extract_docstring(node, content),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
            children: Vec::new(),
        })
    }

    fn extract_type(&self, node: &Node, content: &str) -> Option<Symbol> {
        if node.kind() != "type_definition" {
            return None;
        }
        self.extract_container(node, content)
    }

    fn extract_docstring(&self, node: &Node, content: &str) -> Option<String> {
        // OCaml uses (** ... *) for ocamldoc
        let mut prev = node.prev_sibling();
        while let Some(sibling) = prev {
            let text = &content[sibling.byte_range()];
            if sibling.kind() == "comment" && text.starts_with("(**") {
                let inner = text
                    .strip_prefix("(**").unwrap_or(text)
                    .strip_suffix("*)").unwrap_or(text)
                    .trim();
                if !inner.is_empty() {
                    return Some(inner.to_string());
                }
            }
            prev = sibling.prev_sibling();
        }
        None
    }

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        if node.kind() != "open_statement" {
            return Vec::new();
        }

        let text = &content[node.byte_range()];
        let line = node.start_position().row + 1;

        // Extract module name: "open Module.Path"
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

    fn is_public(&self, _node: &Node, _content: &str) -> bool { true }
    fn get_visibility(&self, _node: &Node, _content: &str) -> Visibility { Visibility::Public }

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
        if ext != "ml" && ext != "mli" { return None; }
        let stem = path.file_stem()?.to_str()?;
        // OCaml module names are capitalized
        let mut chars: Vec<char> = stem.chars().collect();
        if let Some(c) = chars.first_mut() {
            *c = c.to_ascii_uppercase();
        }
        Some(chars.into_iter().collect())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        let lower = module.to_lowercase();
        vec![
            format!("{}.ml", lower),
            format!("{}.mli", lower),
        ]
    }

    fn lang_key(&self) -> &'static str { "ocaml" }

    fn is_stdlib_import(&self, import_name: &str, _project_root: &Path) -> bool {
        // Core OCaml modules
        matches!(import_name, "Stdlib" | "Pervasives" | "Printf" | "List" | "Array" |
            "String" | "Bytes" | "Char" | "Int" | "Float" | "Bool" | "Unit" |
            "Fun" | "Option" | "Result" | "Seq" | "Map" | "Set" | "Hashtbl" |
            "Stack" | "Queue" | "Stream" | "Buffer" | "Format" | "Scanf" |
            "Arg" | "Filename" | "Sys" | "Unix")
    }

    fn find_stdlib(&self, _project_root: &Path) -> Option<PathBuf> { None }

    fn resolve_local_import(&self, import: &str, _current_file: &Path, project_root: &Path) -> Option<PathBuf> {
        let lower = import.to_lowercase();
        for ext in &["ml", "mli"] {
            let candidates = [
                project_root.join("lib").join(format!("{}.{}", lower, ext)),
                project_root.join("src").join(format!("{}.{}", lower, ext)),
                project_root.join(format!("{}.{}", lower, ext)),
            ];
            for c in &candidates {
                if c.is_file() {
                    return Some(c.clone());
                }
            }
        }
        None
    }

    fn resolve_external_import(&self, _import_name: &str, _project_root: &Path) -> Option<ResolvedPackage> {
        None
    }

    fn get_version(&self, project_root: &Path) -> Option<String> {
        // Check for dune or opam files
        if project_root.join("dune-project").is_file() {
            return Some("dune".to_string());
        }
        let opam_files: Vec<_> = std::fs::read_dir(project_root)
            .ok()?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "opam"))
            .collect();
        if !opam_files.is_empty() {
            return Some("opam".to_string());
        }
        None
    }

    fn find_package_cache(&self, _project_root: &Path) -> Option<PathBuf> {
        if let Some(home) = std::env::var_os("HOME") {
            let opam = PathBuf::from(home).join(".opam");
            if opam.is_dir() {
                return Some(opam);
            }
        }
        None
    }

    fn indexable_extensions(&self) -> &'static [&'static str] { &["ml", "mli"] }
    fn package_sources(&self, _project_root: &Path) -> Vec<crate::PackageSource> { Vec::new() }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{skip_dotfiles, has_extension};
        if skip_dotfiles(name) { return true; }
        if is_dir && name == "_build" { return true; }
        !is_dir && !has_extension(name, &["ml", "mli"])
    }

    fn discover_packages(&self, _source: &crate::PackageSource) -> Vec<(String, PathBuf)> { Vec::new() }

    fn package_module_name(&self, entry_name: &str) -> String {
        let stem = entry_name
            .strip_suffix(".ml")
            .or_else(|| entry_name.strip_suffix(".mli"))
            .unwrap_or(entry_name);
        // Capitalize for OCaml module name
        let mut chars: Vec<char> = stem.chars().collect();
        if let Some(c) = chars.first_mut() {
            *c = c.to_ascii_uppercase();
        }
        chars.into_iter().collect()
    }

    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() { Some(path.to_path_buf()) } else { None }
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
            "abstract_type", "add_operator", "aliased_type", "and_operator",
            "application_expression", "array_expression", "array_get_expression",
            "assert_expression", "assign_operator", "bigarray_get_expression",
            "class_application", "class_binding", "class_body_type",
            "class_definition", "class_function", "class_function_type",
            "class_initializer", "class_name", "class_path", "class_type_binding",
            "class_type_definition", "class_type_name", "class_type_path",
            "coercion_expression", "concat_operator", "cons_expression",
            "constrain_module", "constrain_module_type", "constrain_type",
            "constructed_type", "constructor_declaration", "constructor_name",
            "constructor_path", "constructor_pattern", "conversion_specification",
            "do_clause", "else_clause", "exception_definition", "exception_pattern",
            "expression_item", "extended_module_path", "field_declaration",
            "field_expression", "field_get_expression", "for_expression",
            "fun_expression", "function_type", "functor_type", "hash_expression",
            "hash_operator", "hash_type", "include_module", "include_module_type", "infix_expression",
            "indexing_operator", "indexing_operator_path", "inheritance_definition",
            "inheritance_specification", "instance_variable_definition",
            "instance_variable_expression", "instance_variable_specification",
            "instantiated_class", "instantiated_class_type", "labeled_argument_type",
            "labeled_tuple_element_type", "lazy_expression", "let_and_operator",
            "let_class_expression", "let_exception_expression",
            "let_module_expression", "let_open_class_expression",
            "let_open_class_type", "let_open_expression", "let_operator",
            "list_expression", "local_open_expression", "local_open_type",
            "match_operator", "method_definition", "method_invocation",
            "method_name", "method_specification", "method_type", "module_application",
            "module_binding", "module_name", "module_parameter", "module_path",
            "module_type_constraint", "module_type_name", "module_type_of",
            "module_type_path", "mult_operator", "new_expression", "object_copy_expression",
            "object_expression", "object_type", "open_module", "or_operator",
            "package_expression", "package_type", "packed_module",
            "parenthesized_class_expression", "parenthesized_expression",
            "parenthesized_module_expression", "parenthesized_module_type",
            "parenthesized_operator", "parenthesized_type", "polymorphic_type",
            "polymorphic_variant_type", "pow_operator", "prefix_expression",
            "prefix_operator", "record_declaration", "record_expression",
            "refutation_case", "rel_operator", "sequence_expression",
            "set_expression", "sign_expression", "sign_operator",
            "string_get_expression", "structure", "tag_specification",
            "then_clause", "tuple_expression", "tuple_type", "type_binding",
            "type_constraint", "type_constructor", "type_constructor_path",
            "type_parameter_constraint", "type_variable", "typed_class_expression",
            "typed_expression", "typed_module_expression", "typed_pattern",
            "value_specification", "variant_declaration", "while_expression",
        ];
        validate_unused_kinds_audit(&OCaml, documented_unused)
            .expect("OCaml unused node kinds audit failed");
    }
}
