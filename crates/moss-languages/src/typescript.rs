//! TypeScript language support.

use std::path::{Path, PathBuf};
use crate::{Export, Import, Language, Symbol, VisibilityMechanism, Visibility};
use crate::ecmascript;
use crate::external_packages::ResolvedPackage;
use moss_core::tree_sitter::Node;

/// TypeScript language support.
pub struct TypeScript;

/// TSX language support (TypeScript + JSX).
pub struct Tsx;

impl Language for TypeScript {
    fn name(&self) -> &'static str { "TypeScript" }
    fn extensions(&self) -> &'static [&'static str] { &["ts", "mts", "cts"] }
    fn grammar_name(&self) -> &'static str { "typescript" }

    fn has_symbols(&self) -> bool { true }

    fn container_kinds(&self) -> &'static [&'static str] { ecmascript::CONTAINER_KINDS }
    fn function_kinds(&self) -> &'static [&'static str] { ecmascript::TS_FUNCTION_KINDS }
    fn type_kinds(&self) -> &'static [&'static str] { ecmascript::TS_TYPE_KINDS }
    fn import_kinds(&self) -> &'static [&'static str] { ecmascript::IMPORT_KINDS }
    fn public_symbol_kinds(&self) -> &'static [&'static str] { ecmascript::PUBLIC_SYMBOL_KINDS }
    fn visibility_mechanism(&self) -> VisibilityMechanism { VisibilityMechanism::ExplicitExport }
    fn scope_creating_kinds(&self) -> &'static [&'static str] { ecmascript::SCOPE_CREATING_KINDS }
    fn control_flow_kinds(&self) -> &'static [&'static str] { ecmascript::CONTROL_FLOW_KINDS }
    fn complexity_nodes(&self) -> &'static [&'static str] { ecmascript::COMPLEXITY_NODES }
    fn nesting_nodes(&self) -> &'static [&'static str] { ecmascript::NESTING_NODES }

    fn extract_function(&self, node: &Node, content: &str, in_container: bool) -> Option<Symbol> {
        let name = self.node_name(node, content)?;
        Some(ecmascript::extract_function(node, content, in_container, name))
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        let name = self.node_name(node, content)?;
        Some(ecmascript::extract_container(node, name))
    }

    fn extract_type(&self, node: &Node, content: &str) -> Option<Symbol> {
        let name = self.node_name(node, content)?;
        ecmascript::extract_type(node, name)
    }

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        ecmascript::extract_imports(node, content)
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        ecmascript::extract_public_symbols(node, content)
    }

    fn extract_docstring(&self, _node: &Node, _content: &str) -> Option<String> {
        None
    }

    fn is_public(&self, _node: &Node, _content: &str) -> bool {
        true
    }

    fn get_visibility(&self, _node: &Node, _content: &str) -> Visibility {
        Visibility::Public
    }

    fn embedded_content(&self, _node: &Node, _content: &str) -> Option<crate::EmbeddedBlock> { None }

    fn container_body<'a>(&self, node: &'a Node<'a>) -> Option<Node<'a>> {
        node.child_by_field_name("body")
    }

    fn body_has_docstring(&self, _body: &Node, _content: &str) -> bool {
        false
    }

    fn node_name<'a>(&self, node: &Node, content: &'a str) -> Option<&'a str> {
        let name_node = node.child_by_field_name("name")?;
        Some(&content[name_node.byte_range()])
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        if !["ts", "mts", "cts", "tsx"].contains(&ext) {
            return None;
        }
        let stem = path.with_extension("");
        Some(stem.to_string_lossy().to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![
            format!("{}.ts", module),
            format!("{}.tsx", module),
            format!("{}/index.ts", module),
        ]
    }

    fn is_stdlib_import(&self, _import_name: &str, _project_root: &Path) -> bool {
        false
    }

    fn find_stdlib(&self, _project_root: &Path) -> Option<PathBuf> {
        None
    }

    // === Import Resolution ===

    fn lang_key(&self) -> &'static str { "js" } // Uses same cache as JS

    fn resolve_local_import(
        &self,
        module: &str,
        current_file: &Path,
        _project_root: &Path,
    ) -> Option<PathBuf> {
        ecmascript::resolve_local_import(module, current_file, ecmascript::TS_EXTENSIONS)
    }

    fn resolve_external_import(&self, import_name: &str, project_root: &Path) -> Option<ResolvedPackage> {
        ecmascript::resolve_external_import(import_name, project_root)
    }

    fn get_version(&self, _project_root: &Path) -> Option<String> {
        ecmascript::get_version()
    }

    fn find_package_cache(&self, project_root: &Path) -> Option<PathBuf> {
        ecmascript::find_package_cache(project_root)
    }

    fn indexable_extensions(&self) -> &'static [&'static str] {
        &["ts", "mts", "cts", "js", "mjs", "cjs"]
    }

    fn package_sources(&self, project_root: &Path) -> Vec<crate::PackageSource> {
        use crate::{PackageSource, PackageSourceKind};
        let mut sources = Vec::new();
        if let Some(cache) = self.find_package_cache(project_root) {
            sources.push(PackageSource {
                name: "node_modules",
                path: cache,
                kind: PackageSourceKind::NpmScoped,
                version_specific: false,
            });
        }
        sources
    }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{skip_dotfiles, has_extension};
        if skip_dotfiles(name) { return true; }
        if is_dir && (name == "node_modules" || name == ".bin" || name == "test" || name == "tests") {
            return true;
        }
        !is_dir && !has_extension(name, &["ts", "mts", "cts", "js", "mjs", "cjs", "d.ts"])
    }

    fn package_module_name(&self, entry_name: &str) -> String {
        for ext in &[".ts", ".mts", ".cts", ".d.ts", ".js", ".mjs", ".cjs"] {
            if let Some(name) = entry_name.strip_suffix(ext) {
                return name.to_string();
            }
        }
        entry_name.to_string()
    }

    fn discover_packages(&self, source: &crate::PackageSource) -> Vec<(String, PathBuf)> {
        self.discover_npm_scoped_packages(&source.path)
    }

    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() {
            return Some(path.to_path_buf());
        }
        ecmascript::find_package_entry(path)
    }
}

// TSX shares the same implementation as TypeScript, just with a different grammar
impl Language for Tsx {
    fn name(&self) -> &'static str { "TSX" }
    fn extensions(&self) -> &'static [&'static str] { &["tsx"] }
    fn grammar_name(&self) -> &'static str { "tsx" }

    fn has_symbols(&self) -> bool { true }

    fn container_kinds(&self) -> &'static [&'static str] { ecmascript::CONTAINER_KINDS }
    fn function_kinds(&self) -> &'static [&'static str] { ecmascript::TS_FUNCTION_KINDS }
    fn type_kinds(&self) -> &'static [&'static str] { ecmascript::TS_TYPE_KINDS }
    fn import_kinds(&self) -> &'static [&'static str] { ecmascript::IMPORT_KINDS }
    fn public_symbol_kinds(&self) -> &'static [&'static str] { ecmascript::PUBLIC_SYMBOL_KINDS }
    fn visibility_mechanism(&self) -> VisibilityMechanism { VisibilityMechanism::ExplicitExport }
    fn scope_creating_kinds(&self) -> &'static [&'static str] { ecmascript::SCOPE_CREATING_KINDS }
    fn control_flow_kinds(&self) -> &'static [&'static str] { ecmascript::CONTROL_FLOW_KINDS }
    fn complexity_nodes(&self) -> &'static [&'static str] { ecmascript::COMPLEXITY_NODES }
    fn nesting_nodes(&self) -> &'static [&'static str] { ecmascript::NESTING_NODES }

    fn extract_function(&self, node: &Node, content: &str, in_container: bool) -> Option<Symbol> {
        let name = self.node_name(node, content)?;
        Some(ecmascript::extract_function(node, content, in_container, name))
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        let name = self.node_name(node, content)?;
        Some(ecmascript::extract_container(node, name))
    }

    fn extract_type(&self, node: &Node, content: &str) -> Option<Symbol> {
        let name = self.node_name(node, content)?;
        ecmascript::extract_type(node, name)
    }

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        ecmascript::extract_imports(node, content)
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        ecmascript::extract_public_symbols(node, content)
    }

    fn extract_docstring(&self, _node: &Node, _content: &str) -> Option<String> {
        None
    }

    fn is_public(&self, _node: &Node, _content: &str) -> bool {
        true
    }

    fn get_visibility(&self, _node: &Node, _content: &str) -> Visibility {
        Visibility::Public
    }

    fn embedded_content(&self, _node: &Node, _content: &str) -> Option<crate::EmbeddedBlock> { None }

    fn container_body<'a>(&self, node: &'a Node<'a>) -> Option<Node<'a>> {
        node.child_by_field_name("body")
    }

    fn body_has_docstring(&self, _body: &Node, _content: &str) -> bool {
        false
    }

    fn node_name<'a>(&self, node: &Node, content: &'a str) -> Option<&'a str> {
        let name_node = node.child_by_field_name("name")?;
        Some(&content[name_node.byte_range()])
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        if ext != "tsx" {
            return None;
        }
        let stem = path.with_extension("");
        Some(stem.to_string_lossy().to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![
            format!("{}.tsx", module),
            format!("{}/index.tsx", module),
        ]
    }

    fn is_stdlib_import(&self, _import_name: &str, _project_root: &Path) -> bool {
        false
    }

    fn find_stdlib(&self, _project_root: &Path) -> Option<PathBuf> {
        None
    }

    // === Import Resolution ===

    fn lang_key(&self) -> &'static str { "js" }

    fn resolve_local_import(
        &self,
        module: &str,
        current_file: &Path,
        _project_root: &Path,
    ) -> Option<PathBuf> {
        ecmascript::resolve_local_import(module, current_file, ecmascript::TS_EXTENSIONS)
    }

    fn resolve_external_import(&self, import_name: &str, project_root: &Path) -> Option<ResolvedPackage> {
        ecmascript::resolve_external_import(import_name, project_root)
    }

    fn get_version(&self, _project_root: &Path) -> Option<String> {
        ecmascript::get_version()
    }

    fn find_package_cache(&self, project_root: &Path) -> Option<PathBuf> {
        ecmascript::find_package_cache(project_root)
    }

    fn indexable_extensions(&self) -> &'static [&'static str] {
        &["tsx", "ts", "js"]
    }

    fn package_sources(&self, project_root: &Path) -> Vec<crate::PackageSource> {
        use crate::{PackageSource, PackageSourceKind};
        let mut sources = Vec::new();
        if let Some(cache) = self.find_package_cache(project_root) {
            sources.push(PackageSource {
                name: "node_modules",
                path: cache,
                kind: PackageSourceKind::NpmScoped,
                version_specific: false,
            });
        }
        sources
    }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{skip_dotfiles, has_extension};
        if skip_dotfiles(name) { return true; }
        if is_dir && (name == "node_modules" || name == ".bin" || name == "test" || name == "tests") {
            return true;
        }
        !is_dir && !has_extension(name, &["tsx", "ts", "js", "d.ts"])
    }

    fn package_module_name(&self, entry_name: &str) -> String {
        for ext in &[".tsx", ".ts", ".d.ts", ".js"] {
            if let Some(name) = entry_name.strip_suffix(ext) {
                return name.to_string();
            }
        }
        entry_name.to_string()
    }

    fn discover_packages(&self, source: &crate::PackageSource) -> Vec<(String, PathBuf)> {
        self.discover_npm_scoped_packages(&source.path)
    }

    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() {
            return Some(path.to_path_buf());
        }
        ecmascript::find_package_entry(path)
    }
}
