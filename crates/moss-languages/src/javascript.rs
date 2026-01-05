//! JavaScript language support.

use crate::ecmascript;
use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// JavaScript language support.
pub struct JavaScript;

impl Language for JavaScript {
    fn name(&self) -> &'static str {
        "JavaScript"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["js", "mjs", "cjs", "jsx"]
    }
    fn grammar_name(&self) -> &'static str {
        "javascript"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        ecmascript::JS_CONTAINER_KINDS
    }
    fn function_kinds(&self) -> &'static [&'static str] {
        ecmascript::JS_FUNCTION_KINDS
    }
    fn type_kinds(&self) -> &'static [&'static str] {
        ecmascript::JS_TYPE_KINDS
    }
    fn import_kinds(&self) -> &'static [&'static str] {
        ecmascript::IMPORT_KINDS
    }
    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        ecmascript::PUBLIC_SYMBOL_KINDS
    }
    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::ExplicitExport
    }
    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        ecmascript::SCOPE_CREATING_KINDS
    }
    fn control_flow_kinds(&self) -> &'static [&'static str] {
        ecmascript::CONTROL_FLOW_KINDS
    }
    fn complexity_nodes(&self) -> &'static [&'static str] {
        ecmascript::COMPLEXITY_NODES
    }
    fn nesting_nodes(&self) -> &'static [&'static str] {
        ecmascript::NESTING_NODES
    }

    fn signature_suffix(&self) -> &'static str {
        " {}"
    }

    fn extract_function(&self, node: &Node, content: &str, in_container: bool) -> Option<Symbol> {
        let name = self.node_name(node, content)?;
        Some(ecmascript::extract_function(
            node,
            content,
            in_container,
            name,
        ))
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        let name = self.node_name(node, content)?;
        Some(ecmascript::extract_container(node, content, name))
    }

    fn extract_type(&self, node: &Node, content: &str) -> Option<Symbol> {
        // JS classes are the only type-like construct
        self.extract_container(node, content)
    }

    fn extract_docstring(&self, _node: &Node, _content: &str) -> Option<String> {
        // JS doesn't have standardized docstrings (JSDoc would require comment parsing)
        None
    }

    fn extract_attributes(&self, _node: &Node, _content: &str) -> Vec<String> {
        Vec::new()
    }

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        ecmascript::extract_imports(node, content)
    }

    fn format_import(&self, import: &Import, names: Option<&[&str]>) -> String {
        ecmascript::format_import(import, names)
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        ecmascript::extract_public_symbols(node, content)
    }

    fn is_public(&self, _node: &Node, _content: &str) -> bool {
        // JS uses export statements, not visibility modifiers on declarations
        true
    }

    fn get_visibility(&self, _node: &Node, _content: &str) -> Visibility {
        Visibility::Public
    }

    fn is_test_symbol(&self, symbol: &crate::Symbol) -> bool {
        {
            let name = symbol.name.as_str();
            match symbol.kind {
                crate::SymbolKind::Function | crate::SymbolKind::Method => {
                    name.starts_with("test_")
                        || name.starts_with("Test")
                        || name == "describe"
                        || name == "it"
                        || name == "test"
                }
                crate::SymbolKind::Module => {
                    name == "tests" || name == "test" || name == "__tests__"
                }
                _ => false,
            }
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
        let name_node = node.child_by_field_name("name")?;
        Some(&content[name_node.byte_range()])
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        if !["js", "mjs", "cjs", "jsx"].contains(&ext) {
            return None;
        }
        // For relative imports, just use the path without extension
        let stem = path.with_extension("");
        Some(stem.to_string_lossy().to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        vec![
            format!("{}.js", module),
            format!("{}.mjs", module),
            format!("{}/index.js", module),
        ]
    }

    fn is_stdlib_import(&self, _import_name: &str, _project_root: &Path) -> bool {
        // Node.js built-ins (could check against a list, but we don't have source for them)
        false
    }

    fn find_stdlib(&self, _project_root: &Path) -> Option<PathBuf> {
        // Node.js stdlib is compiled into the runtime
        None
    }

    // === Import Resolution ===

    fn lang_key(&self) -> &'static str {
        "js"
    }

    fn resolve_local_import(
        &self,
        module: &str,
        current_file: &Path,
        _project_root: &Path,
    ) -> Option<PathBuf> {
        ecmascript::resolve_local_import(module, current_file, ecmascript::JS_EXTENSIONS)
    }

    fn resolve_external_import(
        &self,
        import_name: &str,
        project_root: &Path,
    ) -> Option<ResolvedPackage> {
        ecmascript::resolve_external_import(import_name, project_root)
    }

    fn get_version(&self, _project_root: &Path) -> Option<String> {
        ecmascript::get_version()
    }

    fn find_package_cache(&self, project_root: &Path) -> Option<PathBuf> {
        ecmascript::find_package_cache(project_root)
    }

    fn indexable_extensions(&self) -> &'static [&'static str] {
        &["js", "mjs", "cjs"]
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
        // Also check for Deno cache
        if let Some(deno_cache) = ecmascript::find_deno_cache() {
            let npm_cache = deno_cache.join("npm").join("registry.npmjs.org");
            if npm_cache.is_dir() {
                sources.push(PackageSource {
                    name: "deno-npm",
                    path: npm_cache,
                    kind: PackageSourceKind::Deno,
                    version_specific: false,
                });
            }
        }
        sources
    }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        use crate::traits::{has_extension, skip_dotfiles};
        if skip_dotfiles(name) {
            return true;
        }
        // Skip common non-source dirs
        if is_dir && (name == "node_modules" || name == ".bin" || name == "test" || name == "tests")
        {
            return true;
        }
        !is_dir && !has_extension(name, self.indexable_extensions())
    }

    fn discover_packages(&self, source: &crate::PackageSource) -> Vec<(String, PathBuf)> {
        match source.kind {
            crate::PackageSourceKind::NpmScoped => self.discover_npm_scoped_packages(&source.path),
            crate::PackageSourceKind::Deno => discover_deno_packages(&source.path),
            _ => Vec::new(),
        }
    }

    fn package_module_name(&self, entry_name: &str) -> String {
        // Strip common JS extensions
        for ext in &[".js", ".mjs", ".cjs"] {
            if let Some(name) = entry_name.strip_suffix(ext) {
                return name.to_string();
            }
        }
        entry_name.to_string()
    }

    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() {
            return Some(path.to_path_buf());
        }
        ecmascript::find_package_entry(path)
    }
}

/// Discover packages in Deno npm cache (package/version/ structure with scoped packages).
fn discover_deno_packages(source_path: &Path) -> Vec<(String, PathBuf)> {
    let entries = match std::fs::read_dir(source_path) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut packages = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if !path.is_dir() {
            continue;
        }

        // Handle scoped packages (@scope/name)
        if name.starts_with('@') {
            if let Ok(scoped) = std::fs::read_dir(&path) {
                for scoped_entry in scoped.flatten() {
                    let scoped_path = scoped_entry.path();
                    let scoped_name =
                        format!("{}/{}", name, scoped_entry.file_name().to_string_lossy());
                    if let Some((pkg_name, pkg_path)) =
                        find_deno_version_dir(&scoped_path, &scoped_name)
                    {
                        packages.push((pkg_name, pkg_path));
                    }
                }
            }
        } else if let Some((pkg_name, pkg_path)) = find_deno_version_dir(&path, &name) {
            packages.push((pkg_name, pkg_path));
        }
    }

    packages
}

/// Find the latest version directory in a Deno package directory.
fn find_deno_version_dir(pkg_path: &Path, pkg_name: &str) -> Option<(String, PathBuf)> {
    let versions: Vec<_> = std::fs::read_dir(pkg_path)
        .ok()?
        .flatten()
        .filter(|e| e.path().is_dir())
        .collect();

    if versions.is_empty() {
        return None;
    }

    // Use the last version (sorted lexically, usually latest)
    let version_dir = versions.last()?.path();
    Some((pkg_name.to_string(), version_dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate_unused_kinds_audit;

    /// Documents node kinds that exist in the JavaScript grammar but aren't used in trait methods.
    /// Run `cross_check_node_kinds` in registry.rs to see all potentially useful kinds.
    #[test]
    fn unused_node_kinds_audit() {
        #[rustfmt::skip]
        let documented_unused: &[&str] = &[
            // STRUCTURAL
            "class_body",              // class body block
            "class_heritage",          // extends clause
            "class_static_block",      // static { }
            "formal_parameters",       // function params
            "field_definition",        // class field
            "identifier",              // too common
            "private_property_identifier", // #field
            "property_identifier",     // obj.prop
            "shorthand_property_identifier", // { x } shorthand
            "shorthand_property_identifier_pattern", // destructuring shorthand
            "statement_block",         // { }
            "statement_identifier",    // label name
            "switch_body",             // switch cases

            // CLAUSE
            "else_clause",             // else branch
            "finally_clause",          // finally block

            // EXPRESSION
            "assignment_expression",   // x = y
            "augmented_assignment_expression", // x += y
            "await_expression",        // await foo
            "call_expression",         // foo()
            "function_expression",     // function() {}
            "member_expression",       // foo.bar
            "new_expression",          // new Foo()
            "parenthesized_expression",// (expr)
            "sequence_expression",     // a, b
            "subscript_expression",    // arr[i]
            "unary_expression",        // -x, !x
            "update_expression",       // x++
            "yield_expression",        // yield x

            // IMPORT/EXPORT DETAILS
            "export_clause",           // export { a, b }
            "export_specifier",        // export { a as b }
            "import",                  // import keyword
            "import_attribute",        // import attributes
            "import_clause",           // import clause
            "import_specifier",        // import { a }
            "named_imports",           // { a, b }
            "namespace_export",        // export * as ns
            "namespace_import",        // import * as ns

            // DECLARATION
            "debugger_statement",      // debugger;
            "empty_statement",         // ;
            "expression_statement",    // expr;
            "generator_function",      // function* foo
            "labeled_statement",       // label: stmt
            "lexical_declaration",     // let/const
            "using_declaration",       // using x = ...
            "variable_declaration",    // var x
            "with_statement",          // with (obj) - deprecated

            // JSX
            "jsx_expression",          // {expr} in JSX
        ];

        validate_unused_kinds_audit(&JavaScript, documented_unused)
            .expect("JavaScript unused node kinds audit failed");
    }
}
