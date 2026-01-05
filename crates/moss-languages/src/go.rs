//! Go language support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use std::process::Command;
use tree_sitter::Node;

// ============================================================================
// Go module parsing (for local import resolution)
// ============================================================================

/// Information from a go.mod file
#[derive(Debug, Clone)]
struct GoModule {
    /// Module path (e.g., "github.com/user/project")
    path: String,
    /// Go version (e.g., "1.21")
    #[allow(dead_code)]
    go_version: Option<String>,
}

/// Parse a go.mod file to extract module information.
fn parse_go_mod(path: &Path) -> Option<GoModule> {
    let content = std::fs::read_to_string(path).ok()?;
    parse_go_mod_content(&content)
}

/// Parse go.mod content string.
fn parse_go_mod_content(content: &str) -> Option<GoModule> {
    let mut module_path = None;
    let mut go_version = None;

    for line in content.lines() {
        let line = line.trim();

        // module github.com/user/project
        if line.starts_with("module ") {
            module_path = Some(line.trim_start_matches("module ").trim().to_string());
        }

        // go 1.21
        if line.starts_with("go ") {
            go_version = Some(line.trim_start_matches("go ").trim().to_string());
        }
    }

    module_path.map(|path| GoModule { path, go_version })
}

/// Find go.mod by walking up from a directory.
fn find_go_mod(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };

    loop {
        let go_mod = current.join("go.mod");
        if go_mod.exists() {
            return Some(go_mod);
        }

        if !current.pop() {
            break;
        }
    }

    None
}

/// Resolve a Go import path to a local directory path.
///
/// Returns the computed path if the import is within the module, None for external imports.
/// Does not check if the path exists - caller should verify.
fn resolve_go_import(import_path: &str, module: &GoModule, project_root: &Path) -> Option<PathBuf> {
    // Check if import is within our module
    if !import_path.starts_with(&module.path) {
        return None; // External import
    }

    // Get the relative path after the module prefix
    let rel_path = import_path.strip_prefix(&module.path)?;
    let rel_path = rel_path.trim_start_matches('/');

    let target = if rel_path.is_empty() {
        project_root.to_path_buf()
    } else {
        project_root.join(rel_path)
    };

    Some(target)
}

// ============================================================================
// Go external package resolution
// ============================================================================

/// Get Go version.
pub fn get_go_version() -> Option<String> {
    let output = Command::new("go").args(["version"]).output().ok()?;

    if output.status.success() {
        let version_str = String::from_utf8_lossy(&output.stdout);
        // "go version go1.21.0 linux/amd64" -> "1.21"
        for part in version_str.split_whitespace() {
            if part.starts_with("go") && part.len() > 2 {
                let ver = part.trim_start_matches("go");
                // Take major.minor only
                let parts: Vec<&str> = ver.split('.').collect();
                if parts.len() >= 2 {
                    return Some(format!("{}.{}", parts[0], parts[1]));
                }
            }
        }
    }

    None
}

/// Find Go stdlib directory (GOROOT/src).
pub fn find_go_stdlib() -> Option<PathBuf> {
    // Try GOROOT env var
    if let Ok(goroot) = std::env::var("GOROOT") {
        let src = PathBuf::from(goroot).join("src");
        if src.is_dir() {
            return Some(src);
        }
    }

    // Try `go env GOROOT`
    if let Ok(output) = Command::new("go").args(["env", "GOROOT"]).output() {
        if output.status.success() {
            let goroot = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let src = PathBuf::from(goroot).join("src");
            if src.is_dir() {
                return Some(src);
            }
        }
    }

    // Common locations
    for path in &["/usr/local/go/src", "/usr/lib/go/src", "/opt/go/src"] {
        let src = PathBuf::from(path);
        if src.is_dir() {
            return Some(src);
        }
    }

    None
}

/// Check if a Go import is a stdlib import (no dots in first path segment).
fn is_go_stdlib_import(import_path: &str) -> bool {
    let first_segment = import_path.split('/').next().unwrap_or(import_path);
    !first_segment.contains('.')
}

/// Resolve a Go stdlib import to its source location.
fn resolve_go_stdlib_import(import_path: &str, stdlib_path: &Path) -> Option<ResolvedPackage> {
    if !is_go_stdlib_import(import_path) {
        return None;
    }

    let pkg_dir = stdlib_path.join(import_path);
    if pkg_dir.is_dir() {
        return Some(ResolvedPackage {
            path: pkg_dir,
            name: import_path.to_string(),
            is_namespace: false,
        });
    }

    None
}

/// Find Go module cache directory.
///
/// Uses GOMODCACHE env var, falls back to ~/go/pkg/mod
pub fn find_go_mod_cache() -> Option<PathBuf> {
    // Check GOMODCACHE env var
    if let Ok(cache) = std::env::var("GOMODCACHE") {
        let path = PathBuf::from(cache);
        if path.is_dir() {
            return Some(path);
        }
    }

    // Fall back to ~/go/pkg/mod using HOME env var
    if let Ok(home) = std::env::var("HOME") {
        let mod_cache = PathBuf::from(home).join("go").join("pkg").join("mod");
        if mod_cache.is_dir() {
            return Some(mod_cache);
        }
    }

    // Windows fallback
    if let Ok(home) = std::env::var("USERPROFILE") {
        let mod_cache = PathBuf::from(home).join("go").join("pkg").join("mod");
        if mod_cache.is_dir() {
            return Some(mod_cache);
        }
    }

    None
}

/// Resolve a Go import from mod cache to its source location.
///
/// Import paths like "github.com/user/repo/pkg" are mapped to
/// $GOMODCACHE/github.com/user/repo@version/pkg
fn resolve_go_mod_cache_import(import_path: &str, mod_cache: &Path) -> Option<ResolvedPackage> {
    // Skip standard library imports (no dots in first segment)
    let first_segment = import_path.split('/').next()?;
    if !first_segment.contains('.') {
        // This is stdlib (fmt, os, etc.) - not in mod cache
        return None;
    }

    // Find the module in cache
    // Import path: github.com/user/repo/internal/pkg
    // Cache path: github.com/user/repo@v1.2.3/internal/pkg

    // We need to find the right version directory
    // Start with the full path and try progressively shorter prefixes
    let parts: Vec<&str> = import_path.split('/').collect();

    for i in (2..=parts.len()).rev() {
        let module_prefix = parts[..i].join("/");
        let module_dir = mod_cache.join(&module_prefix);

        // The parent directory might contain version directories
        if let Some(parent) = module_dir.parent() {
            if parent.is_dir() {
                // Look for versioned directories matching this module
                let module_name = module_dir.file_name()?.to_string_lossy();
                if let Ok(entries) = std::fs::read_dir(parent) {
                    for entry in entries.flatten() {
                        let name = entry.file_name();
                        let name_str = name.to_string_lossy();
                        // Match module@version pattern
                        if name_str.starts_with(&format!("{}@", module_name)) {
                            let versioned_path = entry.path();
                            // Add remaining path components
                            let remainder = if i < parts.len() {
                                parts[i..].join("/")
                            } else {
                                String::new()
                            };
                            let full_path = if remainder.is_empty() {
                                versioned_path.clone()
                            } else {
                                versioned_path.join(&remainder)
                            };

                            if full_path.is_dir() {
                                return Some(ResolvedPackage {
                                    path: full_path,
                                    name: import_path.to_string(),
                                    is_namespace: false,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

// ============================================================================
// Go language support
// ============================================================================

/// Go language support.
pub struct Go;

impl Language for Go {
    fn name(&self) -> &'static str {
        "Go"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["go"]
    }
    fn grammar_name(&self) -> &'static str {
        "go"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &[] // Go types don't have children in the tree-sitter sense
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_declaration", "method_declaration"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["type_spec"] // The actual type is in type_spec, not type_declaration
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["import_declaration"]
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &[
            "function_declaration",
            "method_declaration",
            "type_spec",
            "const_spec",
            "var_spec",
        ]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::NamingConvention
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        &[
            "for_statement",
            "if_statement",
            "expression_switch_statement",
            "type_switch_statement",
            "select_statement",
            "block",
        ]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &[
            "if_statement",
            "for_statement",
            "expression_switch_statement",
            "type_switch_statement",
            "select_statement",
            "return_statement",
            "break_statement",
            "continue_statement",
            "goto_statement",
            "defer_statement",
        ]
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &[
            "if_statement",
            "for_statement",
            "expression_switch_statement",
            "type_switch_statement",
            "select_statement",
            "expression_case",
            "type_case",
            "communication_case",
            "binary_expression",
        ]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &[
            "if_statement",
            "for_statement",
            "expression_switch_statement",
            "type_switch_statement",
            "select_statement",
            "function_declaration",
            "method_declaration",
        ]
    }

    fn signature_suffix(&self) -> &'static str {
        " {}"
    }

    fn extract_function(&self, node: &Node, content: &str, in_container: bool) -> Option<Symbol> {
        let name = self.node_name(node, content)?;
        let params = node
            .child_by_field_name("parameters")
            .map(|p| content[p.byte_range()].to_string())
            .unwrap_or_else(|| "()".to_string());

        Some(Symbol {
            name: name.to_string(),
            kind: if in_container {
                SymbolKind::Method
            } else {
                SymbolKind::Function
            },
            signature: format!("func {}{}", name, params),
            docstring: None,
            attributes: Vec::new(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: if name
                .chars()
                .next()
                .map(|c| c.is_uppercase())
                .unwrap_or(false)
            {
                Visibility::Public
            } else {
                Visibility::Private
            },
            children: Vec::new(),
            is_interface_impl: false,
            implements: Vec::new(),
        })
    }

    fn extract_container(&self, _node: &Node, _content: &str) -> Option<Symbol> {
        None // Go types are extracted via extract_type
    }

    fn extract_type(&self, node: &Node, content: &str) -> Option<Symbol> {
        // Go type_spec: name field + type field (struct_type, interface_type, etc.)
        let name_node = node.child_by_field_name("name")?;
        let name = content[name_node.byte_range()].to_string();

        let type_node = node.child_by_field_name("type");
        let type_kind = type_node.map(|t| t.kind()).unwrap_or("");

        let kind = match type_kind {
            "struct_type" => SymbolKind::Struct,
            "interface_type" => SymbolKind::Interface,
            _ => SymbolKind::Type,
        };

        Some(Symbol {
            name: name.clone(),
            kind,
            signature: format!("type {}", name),
            docstring: None,
            attributes: Vec::new(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: if name
                .chars()
                .next()
                .map(|c| c.is_uppercase())
                .unwrap_or(false)
            {
                Visibility::Public
            } else {
                Visibility::Private
            },
            children: Vec::new(),
            is_interface_impl: false,
            implements: Vec::new(),
        })
    }

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        if node.kind() != "import_declaration" {
            return Vec::new();
        }

        let mut imports = Vec::new();
        let line = node.start_position().row + 1;

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "import_spec" => {
                    // import "path" or import alias "path"
                    if let Some(imp) = Self::parse_import_spec(&child, content, line) {
                        imports.push(imp);
                    }
                }
                "import_spec_list" => {
                    // Grouped imports
                    let mut list_cursor = child.walk();
                    for spec in child.children(&mut list_cursor) {
                        if spec.kind() == "import_spec" {
                            if let Some(imp) = Self::parse_import_spec(&spec, content, line) {
                                imports.push(imp);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        imports
    }

    fn format_import(&self, import: &Import, _names: Option<&[&str]>) -> String {
        // Go: import "pkg" or import alias "pkg"
        if let Some(ref alias) = import.alias {
            format!("import {} \"{}\"", alias, import.module)
        } else {
            format!("import \"{}\"", import.module)
        }
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        // Go exports are determined by uppercase first letter
        let name = match self.node_name(node, content) {
            Some(n) if n.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) => n,
            _ => return Vec::new(),
        };

        let line = node.start_position().row + 1;
        let kind = match node.kind() {
            "function_declaration" => SymbolKind::Function,
            "method_declaration" => SymbolKind::Method,
            "type_spec" => SymbolKind::Type,
            "const_spec" => SymbolKind::Constant,
            "var_spec" => SymbolKind::Variable,
            _ => return Vec::new(),
        };

        vec![Export {
            name: name.to_string(),
            kind,
            line,
        }]
    }

    fn is_public(&self, node: &Node, content: &str) -> bool {
        self.node_name(node, content)
            .and_then(|n| n.chars().next())
            .map(|c| c.is_uppercase())
            .unwrap_or(false)
    }

    fn get_visibility(&self, node: &Node, content: &str) -> Visibility {
        if self.is_public(node, content) {
            Visibility::Public
        } else {
            Visibility::Private
        }
    }

    fn is_test_symbol(&self, symbol: &crate::Symbol) -> bool {
        match symbol.kind {
            crate::SymbolKind::Function => {
                let name = symbol.name.as_str();
                name.starts_with("Test")
                    || name.starts_with("Benchmark")
                    || name.starts_with("Example")
            }
            _ => false,
        }
    }

    fn embedded_content(&self, _node: &Node, _content: &str) -> Option<crate::EmbeddedBlock> {
        None
    }

    fn extract_docstring(&self, _node: &Node, _content: &str) -> Option<String> {
        // Go doc comments could be extracted but need special handling
        None
    }

    fn extract_attributes(&self, _node: &Node, _content: &str) -> Vec<String> {
        Vec::new()
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
        if path.extension()?.to_str()? != "go" {
            return None;
        }
        // Go uses directories as packages, not individual files
        path.parent()?.to_str().map(|s| s.to_string())
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        // Go packages are directories, look for .go files within
        vec![format!("{}/*.go", module)]
    }

    // === Import Resolution ===

    fn lang_key(&self) -> &'static str {
        "go"
    }

    fn resolve_local_import(
        &self,
        import_path: &str,
        current_file: &Path,
        _project_root: &Path,
    ) -> Option<PathBuf> {
        // Find go.mod to understand module boundaries
        if let Some(go_mod_path) = find_go_mod(current_file)
            && let Some(module) = parse_go_mod(&go_mod_path)
        {
            // Try local resolution within the module
            let module_root = go_mod_path.parent()?;
            if let Some(local_path) = resolve_go_import(import_path, &module, module_root)
                && local_path.exists()
                && local_path.is_dir()
            {
                return Some(local_path);
            }
        }
        None
    }

    fn resolve_external_import(
        &self,
        import_name: &str,
        _project_root: &Path,
    ) -> Option<ResolvedPackage> {
        // Check stdlib first
        if is_go_stdlib_import(import_name)
            && let Some(stdlib) = find_go_stdlib()
            && let Some(pkg) = resolve_go_stdlib_import(import_name, &stdlib)
        {
            return Some(pkg);
        }

        // Then mod cache
        if let Some(mod_cache) = find_go_mod_cache() {
            return resolve_go_mod_cache_import(import_name, &mod_cache);
        }

        None
    }

    fn is_stdlib_import(&self, import_name: &str, _project_root: &Path) -> bool {
        is_go_stdlib_import(import_name)
    }

    fn get_version(&self, _project_root: &Path) -> Option<String> {
        get_go_version()
    }

    fn find_package_cache(&self, _project_root: &Path) -> Option<PathBuf> {
        find_go_mod_cache()
    }

    fn indexable_extensions(&self) -> &'static [&'static str] {
        &["go"]
    }

    fn find_stdlib(&self, _project_root: &Path) -> Option<PathBuf> {
        find_go_stdlib()
    }

    fn package_sources(&self, project_root: &Path) -> Vec<crate::PackageSource> {
        use crate::{PackageSource, PackageSourceKind};
        let mut sources = Vec::new();
        if let Some(stdlib) = self.find_stdlib(project_root) {
            sources.push(PackageSource {
                name: "stdlib",
                path: stdlib,
                kind: PackageSourceKind::Recursive,
                version_specific: true,
            });
        }
        if let Some(cache) = self.find_package_cache(project_root) {
            sources.push(PackageSource {
                name: "mod-cache",
                path: cache,
                kind: PackageSourceKind::Recursive,
                version_specific: false,
            });
        }
        sources
    }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        // Skip dotfiles
        if name.starts_with('.') {
            return true;
        }
        // Skip common non-source directories
        if is_dir && (name == "vendor" || name == "internal" || name == "testdata") {
            return true;
        }
        // Skip non-Go files
        if !is_dir && !name.ends_with(".go") {
            return true;
        }
        // Skip test files
        if name.ends_with("_test.go") {
            return true;
        }
        false
    }

    fn package_module_name(&self, entry_name: &str) -> String {
        entry_name
            .strip_suffix(".go")
            .unwrap_or(entry_name)
            .to_string()
    }

    fn discover_packages(&self, source: &crate::PackageSource) -> Vec<(String, PathBuf)> {
        self.discover_recursive_packages(&source.path, &source.path)
    }

    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() && path.extension().map(|e| e == "go").unwrap_or(false) {
            return Some(path.to_path_buf());
        }
        // For directories, Go packages don't have a single entry point
        // Return the directory itself if it contains .go files
        if path.is_dir() {
            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();
                    if name_str.ends_with(".go") && !name_str.ends_with("_test.go") {
                        return Some(path.to_path_buf());
                    }
                }
            }
        }
        None
    }
}

impl Go {
    fn parse_import_spec(node: &Node, content: &str, line: usize) -> Option<Import> {
        let mut path = String::new();
        let mut alias = None;

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "interpreted_string_literal" => {
                    let text = &content[child.byte_range()];
                    path = text.trim_matches('"').to_string();
                }
                "package_identifier" | "blank_identifier" | "dot" => {
                    alias = Some(content[child.byte_range()].to_string());
                }
                _ => {}
            }
        }

        if path.is_empty() {
            return None;
        }

        let is_wildcard = alias.as_deref() == Some(".");
        Some(Import {
            module: path,
            names: Vec::new(),
            alias,
            is_wildcard,
            is_relative: false, // Go doesn't have relative imports in the traditional sense
            line,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_go_mod() {
        let content = r#"
module github.com/user/project

go 1.21

require (
    github.com/pkg/errors v0.9.1
    golang.org/x/sync v0.3.0
)
"#;
        let module = parse_go_mod_content(content).unwrap();
        assert_eq!(module.path, "github.com/user/project");
        assert_eq!(module.go_version, Some("1.21".to_string()));
    }

    #[test]
    fn test_resolve_internal_import() {
        let module = GoModule {
            path: "github.com/user/project".to_string(),
            go_version: Some("1.21".to_string()),
        };

        // Internal import
        let result = resolve_go_import(
            "github.com/user/project/pkg/utils",
            &module,
            Path::new("/fake/root"),
        );
        assert_eq!(result, Some(PathBuf::from("/fake/root/pkg/utils")));

        // External import
        let result = resolve_go_import("github.com/other/lib", &module, Path::new("/fake/root"));
        assert!(result.is_none());
    }

    /// Documents node kinds that exist in the Go grammar but aren't used in trait methods.
    /// Run `cross_check_node_kinds` in registry.rs to see all potentially useful kinds.
    #[test]
    fn unused_node_kinds_audit() {
        use crate::validate_unused_kinds_audit;

        #[rustfmt::skip]
        let documented_unused: &[&str] = &[
            // STRUCTURAL
            "blank_identifier",        // _
            "field_declaration",       // struct field
            "field_declaration_list",  // struct body
            "field_identifier",        // field name
            "identifier",              // too common
            "package_clause",          // package foo
            "package_identifier",      // package name
            "parameter_declaration",   // func param
            "statement_list",          // block contents
            "variadic_parameter_declaration", // ...T

            // CLAUSE
            "default_case",            // default:
            "for_clause",              // for init; cond; post
            "import_spec",             // import spec
            "import_spec_list",        // import block
            "method_elem",             // interface method
            "range_clause",            // for range

            // EXPRESSION
            "call_expression",         // foo()
            "index_expression",        // arr[i]
            "parenthesized_expression",// (expr)
            "selector_expression",     // foo.bar
            "slice_expression",        // arr[1:3]
            "type_assertion_expression", // x.(T)
            "type_conversion_expression", // T(x)
            "type_instantiation_expression", // generic instantiation
            "unary_expression",        // -x, !x

            // TYPE
            "array_type",              // [N]T
            "channel_type",            // chan T
            "implicit_length_array_type", // [...]T
            "function_type",           // func(T) U
            "generic_type",            // T[U]
            "interface_type",          // interface{}
            "map_type",                // map[K]V
            "negated_type",            // ~T
            "parenthesized_type",      // (T)
            "pointer_type",            // *T
            "qualified_type",          // pkg.Type
            "slice_type",              // []T
            "struct_type",             // struct{}
            "type_arguments",          // [T, U]
            "type_constraint",         // T constraint
            "type_elem",               // type element
            "type_identifier",         // type name
            "type_parameter_declaration", // [T any]
            "type_parameter_list",     // type params

            // DECLARATION
            "assignment_statement",    // x = y
            "const_declaration",       // const x = 1
            "dec_statement",           // x--
            "expression_list",         // a, b, c
            "expression_statement",    // expr
            "inc_statement",           // x++
            "short_var_declaration",   // x := y
            "type_alias",              // type X = Y
            "type_declaration",        // type X struct{}
            "var_declaration",         // var x int

            // CONTROL FLOW DETAILS
            "empty_statement",         // ;
            "fallthrough_statement",   // fallthrough
            "go_statement",            // go foo()
            "labeled_statement",       // label:
            "receive_statement",       // <-ch
            "send_statement",          // ch <- x
        ];

        validate_unused_kinds_audit(&Go, documented_unused)
            .expect("Go unused node kinds audit failed");
    }
}
