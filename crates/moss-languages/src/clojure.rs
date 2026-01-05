//! Clojure language support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// Clojure language support.
pub struct Clojure;

impl Language for Clojure {
    fn name(&self) -> &'static str {
        "Clojure"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["clj", "cljs", "cljc", "edn"]
    }
    fn grammar_name(&self) -> &'static str {
        "clojure"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["list_lit"] // (defn ...), (ns ...), etc.
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["list_lit"] // (defn name [...] ...)
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["list_lit"] // (defrecord ...), (defprotocol ...)
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["list_lit"] // (require ...), (import ...)
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["list_lit"]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::NamingConvention // defn- for private
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        if node.kind() != "list_lit" {
            return Vec::new();
        }

        let (form, name) = match self.extract_def_form(node, content) {
            Some(info) => info,
            None => return Vec::new(),
        };

        // defn- is private
        if form == "defn-" || form == "def-" {
            return Vec::new();
        }

        let kind = match form.as_str() {
            "defn" | "defmacro" | "defmethod" => SymbolKind::Function,
            "defrecord" | "deftype" | "defprotocol" => SymbolKind::Struct,
            "def" | "defonce" => SymbolKind::Variable,
            _ => return Vec::new(),
        };

        vec![Export {
            name,
            kind,
            line: node.start_position().row + 1,
        }]
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &["list_lit"] // let, fn, loop, etc.
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &["list_lit"] // if, cond, case, when
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &["list_lit"]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &["list_lit", "vec_lit", "map_lit"]
    }

    fn signature_suffix(&self) -> &'static str {
        ""
    }

    fn extract_function(&self, node: &Node, content: &str, _in_container: bool) -> Option<Symbol> {
        if node.kind() != "list_lit" {
            return None;
        }

        let (form, name) = self.extract_def_form(node, content)?;

        if !matches!(form.as_str(), "defn" | "defn-" | "defmacro" | "defmethod") {
            return None;
        }

        let text = &content[node.byte_range()];
        let first_line = text.lines().next().unwrap_or(text);

        Some(Symbol {
            name,
            kind: SymbolKind::Function,
            signature: first_line.trim().to_string(),
            docstring: self.extract_docstring(node, content),
            attributes: Vec::new(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: if form == "defn-" {
                Visibility::Private
            } else {
                Visibility::Public
            },
            children: Vec::new(),
            is_interface_impl: false,
            implements: Vec::new(),
        })
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        if node.kind() != "list_lit" {
            return None;
        }

        let (form, name) = self.extract_def_form(node, content)?;

        let kind = match form.as_str() {
            "ns" => SymbolKind::Module,
            "defrecord" | "deftype" => SymbolKind::Struct,
            "defprotocol" => SymbolKind::Interface,
            _ => return None,
        };

        Some(Symbol {
            name: name.clone(),
            kind,
            signature: format!("({} {})", form, name),
            docstring: self.extract_docstring(node, content),
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

    fn extract_docstring(&self, node: &Node, content: &str) -> Option<String> {
        // Clojure docstrings are the third element in defn forms
        // (defn name "docstring" [...] ...)
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();

        // Skip first (form name) and second (symbol name), check if third is string
        if children.len() > 2 {
            let third = &children[2];
            if third.kind() == "str_lit" {
                let text = &content[third.byte_range()];
                return Some(text.trim_matches('"').to_string());
            }
        }
        None
    }

    fn extract_attributes(&self, _node: &Node, _content: &str) -> Vec<String> {
        Vec::new()
    }

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        if node.kind() != "list_lit" {
            return Vec::new();
        }

        let (form, _) = match self.extract_def_form(node, content) {
            Some(info) => info,
            None => return Vec::new(),
        };

        if form != "require" && form != "use" && form != "import" {
            return Vec::new();
        }

        // Basic extraction - just note the require/import exists
        vec![Import {
            module: form,
            names: Vec::new(),
            alias: None,
            is_wildcard: false,
            is_relative: false,
            line: node.start_position().row + 1,
        }]
    }

    fn format_import(&self, import: &Import, names: Option<&[&str]>) -> String {
        // Clojure: (require '[namespace]) or (require '[namespace :refer [a b c]])
        let names_to_use: Vec<&str> = names
            .map(|n| n.to_vec())
            .unwrap_or_else(|| import.names.iter().map(|s| s.as_str()).collect());
        if names_to_use.is_empty() {
            format!("(require '[{}])", import.module)
        } else {
            format!(
                "(require '[{} :refer [{}]])",
                import.module,
                names_to_use.join(" ")
            )
        }
    }

    fn is_public(&self, node: &Node, content: &str) -> bool {
        if let Some((form, _)) = self.extract_def_form(node, content) {
            !form.ends_with('-')
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

    fn container_body<'a>(&self, _node: &'a Node<'a>) -> Option<Node<'a>> {
        None
    }
    fn body_has_docstring(&self, _body: &Node, _content: &str) -> bool {
        false
    }
    fn node_name<'a>(&self, _node: &Node, _content: &'a str) -> Option<&'a str> {
        None
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        if !["clj", "cljs", "cljc"].contains(&ext) {
            return None;
        }
        let stem = path.file_stem()?.to_str()?;
        Some(stem.replace('_', "-"))
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        let path = module.replace('-', "_").replace('.', "/");
        vec![
            format!("{}.clj", path),
            format!("{}.cljs", path),
            format!("{}.cljc", path),
        ]
    }

    fn lang_key(&self) -> &'static str {
        "clojure"
    }

    fn is_stdlib_import(&self, import_name: &str, _project_root: &Path) -> bool {
        import_name.starts_with("clojure.") || import_name.starts_with("cljs.")
    }

    fn find_stdlib(&self, _project_root: &Path) -> Option<PathBuf> {
        None
    }

    fn resolve_local_import(
        &self,
        import: &str,
        current_file: &Path,
        _project_root: &Path,
    ) -> Option<PathBuf> {
        let dir = current_file.parent()?;
        let path = import.replace('-', "_").replace('.', "/");
        for ext in &["clj", "cljs", "cljc"] {
            let full = dir.join(format!("{}.{}", path, ext));
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
        None
    }

    fn get_version(&self, project_root: &Path) -> Option<String> {
        // Check project.clj or deps.edn
        let project_clj = project_root.join("project.clj");
        if project_clj.is_file() {
            return Some("leiningen".to_string());
        }
        let deps_edn = project_root.join("deps.edn");
        if deps_edn.is_file() {
            return Some("deps.edn".to_string());
        }
        None
    }

    fn find_package_cache(&self, _project_root: &Path) -> Option<PathBuf> {
        if let Some(home) = std::env::var_os("HOME") {
            let m2 = PathBuf::from(home).join(".m2/repository");
            if m2.is_dir() {
                return Some(m2);
            }
        }
        None
    }

    fn indexable_extensions(&self) -> &'static [&'static str] {
        &["clj", "cljs", "cljc"]
    }
    fn package_sources(&self, _project_root: &Path) -> Vec<crate::PackageSource> {
        Vec::new()
    }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{has_extension, skip_dotfiles};
        if skip_dotfiles(name) {
            return true;
        }
        if is_dir && name == "target" {
            return true;
        }
        !is_dir && !has_extension(name, self.indexable_extensions())
    }

    fn discover_packages(&self, _source: &crate::PackageSource) -> Vec<(String, PathBuf)> {
        Vec::new()
    }

    fn package_module_name(&self, entry_name: &str) -> String {
        entry_name
            .strip_suffix(".clj")
            .or_else(|| entry_name.strip_suffix(".cljs"))
            .or_else(|| entry_name.strip_suffix(".cljc"))
            .unwrap_or(entry_name)
            .replace('_', "-")
    }

    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() {
            Some(path.to_path_buf())
        } else {
            None
        }
    }
}

impl Clojure {
    /// Extract the form name and symbol name from a list like (defn foo ...)
    fn extract_def_form(&self, node: &Node, content: &str) -> Option<(String, String)> {
        let mut cursor = node.walk();
        let mut form = None;
        let mut name = None;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "sym_lit" if form.is_none() => {
                    form = Some(content[child.byte_range()].to_string());
                }
                "sym_lit" if form.is_some() && name.is_none() => {
                    name = Some(content[child.byte_range()].to_string());
                    break;
                }
                _ => {}
            }
        }

        Some((form?, name?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate_unused_kinds_audit;

    #[test]
    fn unused_node_kinds_audit() {
        #[rustfmt::skip]
        let documented_unused: &[&str] = &[];
        validate_unused_kinds_audit(&Clojure, documented_unused)
            .expect("Clojure unused node kinds audit failed");
    }
}
