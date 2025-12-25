//! Haskell language support.

use std::path::{Path, PathBuf};
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use crate::external_packages::ResolvedPackage;
use moss_core::tree_sitter::Node;

/// Haskell language support.
pub struct Haskell;

impl Language for Haskell {
    fn name(&self) -> &'static str { "Haskell" }
    fn extensions(&self) -> &'static [&'static str] { &["hs", "lhs"] }
    fn grammar_name(&self) -> &'static str { "haskell" }

    fn has_symbols(&self) -> bool { true }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["data_type", "newtype", "type_synomym", "class", "instance"]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["function", "signature"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["data_type", "newtype", "type_synomym"]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["import"]
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["function", "data_type", "newtype", "class"]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::ExplicitExport // module export list
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        let name = match self.node_name(node, content) {
            Some(n) => n.to_string(),
            None => return Vec::new(),
        };

        let kind = match node.kind() {
            "function" | "signature" => SymbolKind::Function,
            "data_type" | "newtype" => SymbolKind::Struct,
            "type_synomym" => SymbolKind::Type,
            "class" => SymbolKind::Interface,
            _ => return Vec::new(),
        };

        vec![Export {
            name,
            kind,
            line: node.start_position().row + 1,
        }]
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["let", "where", "do", "lambda"]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &["conditional", "case", "match", "guard"]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &["conditional", "case", "match", "guard", "lambda"]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &["function", "let", "where", "do", "case"]
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
            "data_type" => (SymbolKind::Struct, "data"),
            "newtype" => (SymbolKind::Struct, "newtype"),
            "type_synomym" => (SymbolKind::Type, "type"),
            "class" => (SymbolKind::Interface, "class"),
            "instance" => (SymbolKind::Class, "instance"),
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
        self.extract_container(node, content)
    }

    fn extract_docstring(&self, node: &Node, content: &str) -> Option<String> {
        // Haskell uses -- | or {- | -} for Haddock docs
        let mut prev = node.prev_sibling();
        let mut doc_lines = Vec::new();

        while let Some(sibling) = prev {
            let text = &content[sibling.byte_range()];
            if sibling.kind() == "comment" {
                if text.starts_with("-- |") || text.starts_with("-- ^") {
                    let line = text.strip_prefix("-- |")
                        .or_else(|| text.strip_prefix("-- ^"))
                        .unwrap_or(text)
                        .trim();
                    doc_lines.push(line.to_string());
                } else if text.starts_with("--") {
                    let line = text.strip_prefix("--").unwrap_or(text).trim();
                    doc_lines.push(line.to_string());
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

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        if node.kind() != "import" {
            return Vec::new();
        }

        let text = &content[node.byte_range()];
        let line = node.start_position().row + 1;

        // Extract module name after "import" keyword
        // import qualified Data.Map as M
        let parts: Vec<&str> = text.split_whitespace().collect();
        let mut idx = 1;
        if parts.get(idx) == Some(&"qualified") {
            idx += 1;
        }

        if let Some(module) = parts.get(idx) {
            return vec![Import {
                module: module.to_string(),
                names: Vec::new(),
                alias: None,
                is_wildcard: !text.contains('('),
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
        node.child_by_field_name("where")
    }

    fn body_has_docstring(&self, _body: &Node, _content: &str) -> bool { false }

    fn node_name<'a>(&self, node: &Node, content: &'a str) -> Option<&'a str> {
        node.child_by_field_name("name")
            .map(|n| &content[n.byte_range()])
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        if ext != "hs" && ext != "lhs" { return None; }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        let path = module.replace('.', "/");
        vec![
            format!("{}.hs", path),
            format!("{}.lhs", path),
        ]
    }

    fn lang_key(&self) -> &'static str { "haskell" }

    fn is_stdlib_import(&self, import_name: &str, _project_root: &Path) -> bool {
        // Common base libraries
        import_name.starts_with("Prelude") ||
        import_name.starts_with("Data.") ||
        import_name.starts_with("Control.") ||
        import_name.starts_with("System.") ||
        import_name.starts_with("GHC.")
    }

    fn find_stdlib(&self, _project_root: &Path) -> Option<PathBuf> { None }

    fn resolve_local_import(&self, import: &str, _current_file: &Path, project_root: &Path) -> Option<PathBuf> {
        let path = import.replace('.', "/");
        for ext in &["hs", "lhs"] {
            let candidates = [
                project_root.join("src").join(format!("{}.{}", path, ext)),
                project_root.join("lib").join(format!("{}.{}", path, ext)),
                project_root.join(format!("{}.{}", path, ext)),
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
        // Check cabal or package.yaml
        let cabal_files: Vec<_> = std::fs::read_dir(project_root)
            .ok()?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "cabal"))
            .collect();

        if !cabal_files.is_empty() {
            return Some("cabal".to_string());
        }

        if project_root.join("package.yaml").is_file() {
            return Some("stack".to_string());
        }
        None
    }

    fn find_package_cache(&self, _project_root: &Path) -> Option<PathBuf> {
        if let Some(home) = std::env::var_os("HOME") {
            let cabal = PathBuf::from(&home).join(".cabal/store");
            if cabal.is_dir() {
                return Some(cabal);
            }
            let stack = PathBuf::from(&home).join(".stack");
            if stack.is_dir() {
                return Some(stack);
            }
        }
        None
    }

    fn indexable_extensions(&self) -> &'static [&'static str] { &["hs", "lhs"] }
    fn package_sources(&self, _project_root: &Path) -> Vec<crate::PackageSource> { Vec::new() }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{skip_dotfiles, has_extension};
        if skip_dotfiles(name) { return true; }
        if is_dir && (name == "dist" || name == "dist-newstyle" || name == ".stack-work") {
            return true;
        }
        !is_dir && !has_extension(name, &["hs", "lhs"])
    }

    fn discover_packages(&self, _source: &crate::PackageSource) -> Vec<(String, PathBuf)> { Vec::new() }

    fn package_module_name(&self, entry_name: &str) -> String {
        entry_name
            .strip_suffix(".hs")
            .or_else(|| entry_name.strip_suffix(".lhs"))
            .unwrap_or(entry_name)
            .to_string()
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
            "associated_type", "class_declarations", "constructor",
            "constructor_operator", "constructor_synonym", "constructor_synonyms",
            "data_constructor", "data_constructors", "declarations",
            "default_types", "do_module", "explicit_type", "export", "exports",
            "forall", "forall_required", "foreign_export", "foreign_import",
            "function_head_parens", "gadt_constructor", "gadt_constructors",
            "generator", "import_list", "import_name", "import_package", "imports",
            "instance_declarations", "lambda_case", "lambda_cases",
            "linear_function", "list_comprehension", "modifier", "module",
            "module_export", "module_id", "multi_way_if", "newtype_constructor",
            "operator", "qualified", "qualifiers", "quantified_variables",
            "quasiquote_body", "quoted_expression", "quoted_type", "transform",
            "type_application", "type_binder", "type_family",
            "type_family_injectivity", "type_family_result", "type_instance",
            "type_params", "type_patterns", "type_role",
            "typed_quote",
        ];
        validate_unused_kinds_audit(&Haskell, documented_unused)
            .expect("Haskell unused node kinds audit failed");
    }
}
