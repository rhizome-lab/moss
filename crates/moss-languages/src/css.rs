//! CSS language support (parse only, minimal skeleton).

use std::path::{Path, PathBuf};
use crate::{Export, Import, Language, Symbol, Visibility, VisibilityMechanism};
use crate::external_packages::ResolvedPackage;
use moss_core::tree_sitter::Node;

/// CSS language support.
pub struct Css;

impl Language for Css {
    fn name(&self) -> &'static str { "CSS" }
    fn extensions(&self) -> &'static [&'static str] { &["css", "scss"] }
    fn grammar_name(&self) -> &'static str { "css" }

    fn has_symbols(&self) -> bool { false }

    // CSS has no functions/containers/types in the traditional sense
    fn container_kinds(&self) -> &'static [&'static str] { &[] }
    fn function_kinds(&self) -> &'static [&'static str] { &[] }
    fn type_kinds(&self) -> &'static [&'static str] { &[] }
    fn import_kinds(&self) -> &'static [&'static str] { &[] }
    fn public_symbol_kinds(&self) -> &'static [&'static str] { &[] }
    fn visibility_mechanism(&self) -> VisibilityMechanism { VisibilityMechanism::NotApplicable }
    fn scope_creating_kinds(&self) -> &'static [&'static str] { &[] }
    fn control_flow_kinds(&self) -> &'static [&'static str] { &[] }
    fn complexity_nodes(&self) -> &'static [&'static str] { &[] }
    fn nesting_nodes(&self) -> &'static [&'static str] { &[] }

    fn extract_function(&self, _node: &Node, _content: &str, _in_container: bool) -> Option<Symbol> {
        None
    }

    fn extract_container(&self, _node: &Node, _content: &str) -> Option<Symbol> {
        None
    }

    fn extract_type(&self, _node: &Node, _content: &str) -> Option<Symbol> { None }
    fn extract_docstring(&self, _node: &Node, _content: &str) -> Option<String> { None }
    fn extract_imports(&self, _node: &Node, _content: &str) -> Vec<Import> { Vec::new() }
    fn extract_public_symbols(&self, _node: &Node, _content: &str) -> Vec<Export> { Vec::new() }

    fn is_public(&self, _node: &Node, _content: &str) -> bool { true }
    fn get_visibility(&self, _node: &Node, _content: &str) -> Visibility { Visibility::Public }

    fn embedded_content(&self, _node: &Node, _content: &str) -> Option<crate::EmbeddedBlock> { None }

    fn container_body<'a>(&self, _node: &'a Node<'a>) -> Option<Node<'a>> { None }
    fn body_has_docstring(&self, _body: &Node, _content: &str) -> bool { false }
    fn node_name<'a>(&self, _node: &Node, _content: &'a str) -> Option<&'a str> { None }

    fn file_path_to_module_name(&self, _: &Path) -> Option<String> { None }
    fn module_name_to_paths(&self, _: &str) -> Vec<String> { Vec::new() }

    fn lang_key(&self) -> &'static str { "" }
    fn resolve_local_import(&self, _: &str, _: &Path, _: &Path) -> Option<PathBuf> { None }
    fn resolve_external_import(&self, _: &str, _: &Path) -> Option<ResolvedPackage> { None }
    fn is_stdlib_import(&self, _: &str, _: &Path) -> bool { false }
    fn get_version(&self, _: &Path) -> Option<String> { None }
    fn find_package_cache(&self, _: &Path) -> Option<PathBuf> { None }
    fn indexable_extensions(&self) -> &'static [&'static str] { &[] }
    fn find_stdlib(&self, _: &Path) -> Option<PathBuf> { None }
    fn package_module_name(&self, name: &str) -> String { name.to_string() }
    fn package_sources(&self, _: &Path) -> Vec<crate::PackageSource> { Vec::new() }
    fn discover_packages(&self, _: &crate::PackageSource) -> Vec<(String, PathBuf)> { Vec::new() }
    fn find_package_entry(&self, _: &Path) -> Option<PathBuf> { None }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{skip_dotfiles, has_extension};
        if skip_dotfiles(name) { return true; }
        !is_dir && !has_extension(name, &["css", "scss"])
    }
}
