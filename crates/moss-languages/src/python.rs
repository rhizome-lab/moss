//! Python language support.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Language, Symbol, SymbolKind, Visibility, VisibilityMechanism};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tree_sitter::Node;

// ============================================================================
// Python path cache (filesystem-based detection, no subprocess calls)
// ============================================================================

static PYTHON_CACHE: Mutex<Option<PythonPathCache>> = Mutex::new(None);

/// Cached Python paths detected from filesystem structure.
#[derive(Clone)]
struct PythonPathCache {
    /// Canonical project root used as cache key
    root: PathBuf,
    /// Python version (e.g., "3.13")
    version: Option<String>,
    /// Stdlib path (e.g., /usr/.../lib/python3.13/)
    stdlib: Option<PathBuf>,
    /// Site-packages path
    site_packages: Option<PathBuf>,
}

impl PythonPathCache {
    fn new(root: &Path) -> Self {
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());

        // Try to find Python from venv or PATH
        let python_bin = if root.join(".venv/bin/python").exists() {
            Some(root.join(".venv/bin/python"))
        } else if root.join("venv/bin/python").exists() {
            Some(root.join("venv/bin/python"))
        } else {
            // Look in PATH
            std::env::var("PATH").ok().and_then(|path| {
                for dir in path.split(':') {
                    let python = PathBuf::from(dir).join("python3");
                    if python.exists() {
                        return Some(python);
                    }
                    let python = PathBuf::from(dir).join("python");
                    if python.exists() {
                        return Some(python);
                    }
                }
                None
            })
        };

        let Some(python_bin) = python_bin else {
            return Self {
                root,
                version: None,
                stdlib: None,
                site_packages: None,
            };
        };

        // Resolve symlinks to find the actual Python installation
        let python_real = std::fs::canonicalize(&python_bin).unwrap_or(python_bin.clone());

        // Python binary is typically at /prefix/bin/python3
        // Stdlib is at /prefix/lib/pythonX.Y/
        // Site-packages is at /prefix/lib/pythonX.Y/site-packages/ (system)
        // Or for venv: venv/lib/pythonX.Y/site-packages/

        let prefix = python_real.parent().and_then(|bin| bin.parent());

        // Look for lib/pythonX.Y directories to detect version
        let (version, stdlib, site_packages) = if let Some(prefix) = prefix {
            let lib = prefix.join("lib");
            if lib.exists() {
                // Find pythonX.Y directories
                let mut best_version: Option<(String, PathBuf)> = None;
                if let Ok(entries) = std::fs::read_dir(&lib) {
                    for entry in entries.flatten() {
                        let name = entry.file_name();
                        let name = name.to_string_lossy();
                        if name.starts_with("python") && entry.path().is_dir() {
                            let ver = name.trim_start_matches("python");
                            // Check it looks like a version (X.Y)
                            if ver.contains('.')
                                && ver.chars().next().is_some_and(|c| c.is_ascii_digit())
                            {
                                // Prefer higher versions
                                if best_version.as_ref().is_none_or(|(v, _)| ver > v.as_str()) {
                                    best_version = Some((ver.to_string(), entry.path()));
                                }
                            }
                        }
                    }
                }

                if let Some((ver, stdlib_path)) = best_version {
                    // For venv, site-packages is in the venv
                    let site = if root.join(".venv").exists() || root.join("venv").exists() {
                        let venv = if root.join(".venv").exists() {
                            root.join(".venv")
                        } else {
                            root.join("venv")
                        };
                        let venv_site = venv
                            .join("lib")
                            .join(format!("python{}", ver))
                            .join("site-packages");
                        if venv_site.exists() {
                            Some(venv_site)
                        } else {
                            // Fall back to system site-packages
                            let sys_site = stdlib_path.join("site-packages");
                            if sys_site.exists() {
                                Some(sys_site)
                            } else {
                                None
                            }
                        }
                    } else {
                        let sys_site = stdlib_path.join("site-packages");
                        if sys_site.exists() {
                            Some(sys_site)
                        } else {
                            None
                        }
                    };

                    (Some(ver), Some(stdlib_path), site)
                } else {
                    (None, None, None)
                }
            } else {
                (None, None, None)
            }
        } else {
            (None, None, None)
        };

        Self {
            root,
            version,
            stdlib,
            site_packages,
        }
    }
}

/// Get cached Python paths for a project.
fn get_python_cache(project_root: &Path) -> PythonPathCache {
    let canonical = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());

    let mut cache_guard = PYTHON_CACHE.lock().unwrap();

    if let Some(ref cache) = *cache_guard {
        if cache.root == canonical {
            return cache.clone();
        }
    }

    let new_cache = PythonPathCache::new(project_root);
    *cache_guard = Some(new_cache.clone());
    new_cache
}

// ============================================================================
// Python stdlib and site-packages resolution
// ============================================================================

/// Get Python version from filesystem structure (no subprocess).
pub fn get_python_version(project_root: &Path) -> Option<String> {
    get_python_cache(project_root).version
}

/// Find Python stdlib directory from filesystem structure (no subprocess).
pub fn find_python_stdlib(project_root: &Path) -> Option<PathBuf> {
    get_python_cache(project_root).stdlib
}

/// Check if a module name is a Python stdlib module.
fn is_python_stdlib_module(module_name: &str, stdlib_path: &Path) -> bool {
    let top_level = module_name.split('.').next().unwrap_or(module_name);

    // Check for package
    let pkg_dir = stdlib_path.join(top_level);
    if pkg_dir.is_dir() {
        return true;
    }

    // Check for module
    let py_file = stdlib_path.join(format!("{}.py", top_level));
    py_file.is_file()
}

/// Resolve a Python stdlib import to its source location.
fn resolve_python_stdlib_import(import_name: &str, stdlib_path: &Path) -> Option<ResolvedPackage> {
    let parts: Vec<&str> = import_name.split('.').collect();
    let top_level = parts[0];

    // Check for package (directory)
    let pkg_dir = stdlib_path.join(top_level);
    if pkg_dir.is_dir() {
        if parts.len() == 1 {
            let init = pkg_dir.join("__init__.py");
            if init.is_file() {
                return Some(ResolvedPackage {
                    path: pkg_dir,
                    name: import_name.to_string(),
                    is_namespace: false,
                });
            }
            // Some stdlib packages don't have __init__.py in newer Python
            return Some(ResolvedPackage {
                path: pkg_dir,
                name: import_name.to_string(),
                is_namespace: true,
            });
        } else {
            // Submodule
            let mut path = pkg_dir.clone();
            for part in &parts[1..] {
                path = path.join(part);
            }

            if path.is_dir() {
                let init = path.join("__init__.py");
                return Some(ResolvedPackage {
                    path: path.clone(),
                    name: import_name.to_string(),
                    is_namespace: !init.is_file(),
                });
            }

            let py_file = path.with_extension("py");
            if py_file.is_file() {
                return Some(ResolvedPackage {
                    path: py_file,
                    name: import_name.to_string(),
                    is_namespace: false,
                });
            }

            return None;
        }
    }

    // Check for single-file module
    let py_file = stdlib_path.join(format!("{}.py", top_level));
    if py_file.is_file() {
        return Some(ResolvedPackage {
            path: py_file,
            name: import_name.to_string(),
            is_namespace: false,
        });
    }

    None
}

/// Find Python site-packages directory for a project.
///
/// Search order:
/// 1. .venv/lib/pythonX.Y/site-packages/ (uv, poetry, standard venv)
/// 2. Walk up looking for venv directories
pub fn find_python_site_packages(project_root: &Path) -> Option<PathBuf> {
    // Use cached result from filesystem detection
    if let Some(site) = get_python_cache(project_root).site_packages {
        return Some(site);
    }

    // Fall back to scanning parent directories for venvs
    let mut current = project_root.to_path_buf();
    while let Some(parent) = current.parent() {
        let venv_dir = parent.join(".venv");
        if venv_dir.is_dir() {
            if let Some(site_packages) = find_site_packages_in_venv(&venv_dir) {
                return Some(site_packages);
            }
        }
        current = parent.to_path_buf();
    }

    None
}

/// Find site-packages within a venv directory.
fn find_site_packages_in_venv(venv: &Path) -> Option<PathBuf> {
    // Unix: lib/pythonX.Y/site-packages
    let lib_dir = venv.join("lib");
    if lib_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&lib_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with("python") {
                    let site_packages = entry.path().join("site-packages");
                    if site_packages.is_dir() {
                        return Some(site_packages);
                    }
                }
            }
        }
    }

    // Windows: Lib/site-packages
    let lib_dir = venv.join("Lib").join("site-packages");
    if lib_dir.is_dir() {
        return Some(lib_dir);
    }

    None
}

/// Resolve a Python import to its source location.
///
/// Handles:
/// - Package imports (requests -> requests/__init__.py)
/// - Module imports (six -> six.py)
/// - Submodule imports (requests.api -> requests/api.py)
/// - Namespace packages (no __init__.py)
fn resolve_python_import(import_name: &str, site_packages: &Path) -> Option<ResolvedPackage> {
    // Split on dots for submodule resolution
    let parts: Vec<&str> = import_name.split('.').collect();
    let top_level = parts[0];

    // Check for package (directory)
    let pkg_dir = site_packages.join(top_level);
    if pkg_dir.is_dir() {
        if parts.len() == 1 {
            // Just the package - look for __init__.py
            let init = pkg_dir.join("__init__.py");
            if init.is_file() {
                return Some(ResolvedPackage {
                    path: pkg_dir,
                    name: import_name.to_string(),
                    is_namespace: false,
                });
            }
            // Namespace package (no __init__.py)
            return Some(ResolvedPackage {
                path: pkg_dir,
                name: import_name.to_string(),
                is_namespace: true,
            });
        } else {
            // Submodule - build path
            let mut path = pkg_dir.clone();
            for part in &parts[1..] {
                path = path.join(part);
            }

            // Try as package first
            if path.is_dir() {
                let init = path.join("__init__.py");
                return Some(ResolvedPackage {
                    path: path.clone(),
                    name: import_name.to_string(),
                    is_namespace: !init.is_file(),
                });
            }

            // Try as module
            let py_file = path.with_extension("py");
            if py_file.is_file() {
                return Some(ResolvedPackage {
                    path: py_file,
                    name: import_name.to_string(),
                    is_namespace: false,
                });
            }

            return None;
        }
    }

    // Check for single-file module
    let py_file = site_packages.join(format!("{}.py", top_level));
    if py_file.is_file() {
        return Some(ResolvedPackage {
            path: py_file,
            name: import_name.to_string(),
            is_namespace: false,
        });
    }

    None
}

// ============================================================================
// Python language support
// ============================================================================

/// Python language support.
pub struct Python;

impl Language for Python {
    fn name(&self) -> &'static str {
        "Python"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["py", "pyi", "pyw"]
    }
    fn grammar_name(&self) -> &'static str {
        "python"
    }

    fn has_symbols(&self) -> bool {
        true
    }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["class_definition"]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_definition"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["class_definition"]
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["import_statement", "import_from_statement"]
    }

    fn public_symbol_kinds(&self) -> &'static [&'static str] {
        &["function_definition", "class_definition"]
    }

    fn visibility_mechanism(&self) -> VisibilityMechanism {
        VisibilityMechanism::NamingConvention
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &[
            "if_statement",
            "for_statement",
            "while_statement",
            "try_statement",
            "except_clause",
            "with_statement",
            "match_statement",
            "case_clause",
            "and",
            "or",
            "conditional_expression",
            "list_comprehension",
            "dictionary_comprehension",
            "set_comprehension",
            "generator_expression",
        ]
    }

    fn nesting_nodes(&self) -> &'static [&'static str] {
        &[
            "if_statement",
            "for_statement",
            "while_statement",
            "try_statement",
            "with_statement",
            "match_statement",
            "function_definition",
            "class_definition",
        ]
    }

    fn signature_suffix(&self) -> &'static str {
        ""
    }

    fn scope_creating_kinds(&self) -> &'static [&'static str] {
        // Additional scope-creating nodes beyond functions and containers
        &[
            "for_statement",
            "with_statement",
            "list_comprehension",
            "set_comprehension",
            "dictionary_comprehension",
            "generator_expression",
            "lambda",
        ]
    }

    fn control_flow_kinds(&self) -> &'static [&'static str] {
        &[
            "if_statement",
            "for_statement",
            "while_statement",
            "try_statement",
            "with_statement",
            "match_statement",
            "return_statement",
            "break_statement",
            "continue_statement",
            "raise_statement",
            "assert_statement",
        ]
    }

    fn extract_function(&self, node: &Node, content: &str, in_container: bool) -> Option<Symbol> {
        let name = self.node_name(node, content)?;

        // Skip private methods unless they're dunder methods
        // (visibility filtering can be done by caller)

        // Check for async keyword as first child token
        let is_async = node
            .child(0)
            .map(|c| &content[c.byte_range()] == "async")
            .unwrap_or(false);
        let prefix = if is_async { "async def" } else { "def" };

        let params = node
            .child_by_field_name("parameters")
            .map(|p| &content[p.byte_range()])
            .unwrap_or("()");

        let return_type = node
            .child_by_field_name("return_type")
            .map(|r| format!(" -> {}", &content[r.byte_range()]))
            .unwrap_or_default();

        let signature = format!("{} {}{}{}", prefix, name, params, return_type);
        let visibility = self.get_visibility(node, content);

        Some(Symbol {
            name: name.to_string(),
            kind: if in_container {
                SymbolKind::Method
            } else {
                SymbolKind::Function
            },
            signature,
            docstring: self.extract_docstring(node, content),
            attributes: Vec::new(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility,
            children: Vec::new(),
            is_interface_impl: false,
            implements: Vec::new(),
        })
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        let name = self.node_name(node, content)?;

        let bases = node
            .child_by_field_name("superclasses")
            .map(|b| &content[b.byte_range()])
            .unwrap_or("");

        let signature = if bases.is_empty() {
            format!("class {}", name)
        } else {
            format!("class {}{}", name, bases)
        };

        Some(Symbol {
            name: name.to_string(),
            kind: SymbolKind::Class,
            signature,
            docstring: self.extract_docstring(node, content),
            attributes: Vec::new(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: self.get_visibility(node, content),
            children: Vec::new(), // Caller fills this in
            is_interface_impl: false,
            implements: Vec::new(),
        })
    }

    fn extract_type(&self, node: &Node, content: &str) -> Option<Symbol> {
        // Python classes are both containers and types
        self.extract_container(node, content)
    }

    fn extract_docstring(&self, node: &Node, content: &str) -> Option<String> {
        let body = node.child_by_field_name("body")?;
        let first = body.child(0)?;

        // Handle both grammar versions:
        // - Old: expression_statement > string
        // - New (arborium): string directly, with string_content child
        let string_node = match first.kind() {
            "string" => Some(first),
            "expression_statement" => first.child(0).filter(|n| n.kind() == "string"),
            _ => None,
        }?;

        // Try string_content child (arborium style)
        let mut cursor = string_node.walk();
        for child in string_node.children(&mut cursor) {
            if child.kind() == "string_content" {
                let doc = content[child.byte_range()].trim();
                if !doc.is_empty() {
                    return Some(doc.to_string());
                }
            }
        }

        // Fallback: extract from full string text (old style)
        let text = &content[string_node.byte_range()];
        let doc = text
            .trim_start_matches("\"\"\"")
            .trim_start_matches("'''")
            .trim_start_matches('"')
            .trim_start_matches('\'')
            .trim_end_matches("\"\"\"")
            .trim_end_matches("'''")
            .trim_end_matches('"')
            .trim_end_matches('\'')
            .trim();

        if !doc.is_empty() {
            Some(doc.to_string())
        } else {
            None
        }
    }

    fn extract_attributes(&self, _node: &Node, _content: &str) -> Vec<String> {
        Vec::new()
    }

    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        let line = node.start_position().row + 1;

        match node.kind() {
            "import_statement" => {
                // import foo, import foo as bar
                let mut imports = Vec::new();
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "dotted_name" {
                        let module = content[child.byte_range()].to_string();
                        imports.push(Import {
                            module,
                            names: Vec::new(),
                            alias: None,
                            is_wildcard: false,
                            is_relative: false,
                            line,
                        });
                    } else if child.kind() == "aliased_import" {
                        if let Some(name) = child.child_by_field_name("name") {
                            let module = content[name.byte_range()].to_string();
                            let alias = child
                                .child_by_field_name("alias")
                                .map(|a| content[a.byte_range()].to_string());
                            imports.push(Import {
                                module,
                                names: Vec::new(),
                                alias,
                                is_wildcard: false,
                                is_relative: false,
                                line,
                            });
                        }
                    }
                }
                imports
            }
            "import_from_statement" => {
                // from foo import bar, baz
                let module = node
                    .child_by_field_name("module_name")
                    .map(|m| content[m.byte_range()].to_string())
                    .unwrap_or_default();

                // Check for relative import (from . or from .. or from .foo)
                let text = &content[node.byte_range()];
                let is_relative = text.starts_with("from .");

                let mut names = Vec::new();
                let mut is_wildcard = false;
                let module_end = node
                    .child_by_field_name("module_name")
                    .map(|m| m.end_byte())
                    .unwrap_or(0);

                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    match child.kind() {
                        "dotted_name" | "identifier" => {
                            // Skip the module name itself
                            if child.start_byte() > module_end {
                                names.push(content[child.byte_range()].to_string());
                            }
                        }
                        "aliased_import" => {
                            if let Some(name) = child.child_by_field_name("name") {
                                names.push(content[name.byte_range()].to_string());
                            }
                        }
                        "wildcard_import" => {
                            is_wildcard = true;
                        }
                        _ => {}
                    }
                }

                vec![Import {
                    module,
                    names,
                    alias: None,
                    is_wildcard,
                    is_relative,
                    line,
                }]
            }
            _ => Vec::new(),
        }
    }

    fn format_import(&self, import: &Import, names: Option<&[&str]>) -> String {
        let names_to_use: Vec<&str> = names
            .map(|n| n.to_vec())
            .unwrap_or_else(|| import.names.iter().map(|s| s.as_str()).collect());

        if import.is_wildcard {
            format!("from {} import *", import.module)
        } else if names_to_use.is_empty() {
            if let Some(ref alias) = import.alias {
                format!("import {} as {}", import.module, alias)
            } else {
                format!("import {}", import.module)
            }
        } else {
            format!("from {} import {}", import.module, names_to_use.join(", "))
        }
    }

    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        let line = node.start_position().row + 1;

        match node.kind() {
            "function_definition" => {
                if let Some(name) = self.node_name(node, content) {
                    if !name.starts_with('_') {
                        return vec![Export {
                            name: name.to_string(),
                            kind: SymbolKind::Function,
                            line,
                        }];
                    }
                }
                Vec::new()
            }
            "class_definition" => {
                if let Some(name) = self.node_name(node, content) {
                    if !name.starts_with('_') {
                        return vec![Export {
                            name: name.to_string(),
                            kind: SymbolKind::Class,
                            line,
                        }];
                    }
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn is_public(&self, node: &Node, content: &str) -> bool {
        if let Some(name) = self.node_name(node, content) {
            // Public if doesn't start with _ or is dunder method
            !name.starts_with('_') || name.starts_with("__")
        } else {
            true
        }
    }

    fn get_visibility(&self, node: &Node, content: &str) -> Visibility {
        if let Some(name) = self.node_name(node, content) {
            if name.starts_with("__") && name.ends_with("__") {
                Visibility::Public // dunder methods
            } else if name.starts_with("__") {
                Visibility::Private // name mangled
            } else if name.starts_with('_') {
                Visibility::Protected // convention private
            } else {
                Visibility::Public
            }
        } else {
            Visibility::Public
        }
    }

    fn is_test_symbol(&self, symbol: &crate::Symbol) -> bool {
        let name = symbol.name.as_str();
        match symbol.kind {
            crate::SymbolKind::Function | crate::SymbolKind::Method => name.starts_with("test_"),
            crate::SymbolKind::Class => name.starts_with("Test") && name.len() > 4,
            crate::SymbolKind::Module => name == "tests" || name == "test" || name == "__tests__",
            _ => false,
        }
    }

    fn embedded_content(&self, _node: &Node, _content: &str) -> Option<crate::EmbeddedBlock> {
        None
    }

    fn body_has_docstring(&self, body: &Node, content: &str) -> bool {
        let _ = content;
        body.child(0)
            .map(|c| {
                c.kind() == "string"
                    || (c.kind() == "expression_statement"
                        && c.child(0).map(|n| n.kind() == "string").unwrap_or(false))
            })
            .unwrap_or(false)
    }

    fn container_body<'a>(&self, node: &'a Node<'a>) -> Option<Node<'a>> {
        node.child_by_field_name("body")
    }

    fn node_name<'a>(&self, node: &Node, content: &'a str) -> Option<&'a str> {
        let name_node = node.child_by_field_name("name")?;
        Some(&content[name_node.byte_range()])
    }

    // === Import Resolution ===

    fn lang_key(&self) -> &'static str {
        "python"
    }

    fn resolve_local_import(
        &self,
        import_name: &str,
        current_file: &Path,
        project_root: &Path,
    ) -> Option<PathBuf> {
        // Handle relative imports (starting with .)
        if import_name.starts_with('.') {
            let current_dir = current_file.parent()?;
            let dots = import_name.chars().take_while(|c| *c == '.').count();
            let module_part = &import_name[dots..];

            // Go up (dots-1) directories from current file's directory
            let mut base = current_dir.to_path_buf();
            for _ in 1..dots {
                base = base.parent()?.to_path_buf();
            }

            // Convert module.path to module/path.py
            let module_path = if module_part.is_empty() {
                base.join("__init__.py")
            } else {
                let path_part = module_part.replace('.', "/");
                // Try module/submodule.py first, then module/submodule/__init__.py
                let direct = base.join(format!("{}.py", path_part));
                if direct.exists() {
                    return Some(direct);
                }
                base.join(path_part).join("__init__.py")
            };

            if module_path.exists() {
                return Some(module_path);
            }
        }

        // Handle absolute imports - try to find in src/ or as top-level package
        let module_path = import_name.replace('.', "/");

        // Try src/<module>.py
        let src_path = project_root.join("src").join(format!("{}.py", module_path));
        if src_path.exists() {
            return Some(src_path);
        }

        // Try src/<module>/__init__.py
        let src_pkg_path = project_root
            .join("src")
            .join(&module_path)
            .join("__init__.py");
        if src_pkg_path.exists() {
            return Some(src_pkg_path);
        }

        // Try <module>.py directly
        let direct_path = project_root.join(format!("{}.py", module_path));
        if direct_path.exists() {
            return Some(direct_path);
        }

        // Try <module>/__init__.py
        let pkg_path = project_root.join(&module_path).join("__init__.py");
        if pkg_path.exists() {
            return Some(pkg_path);
        }

        None
    }

    fn resolve_external_import(
        &self,
        import_name: &str,
        project_root: &Path,
    ) -> Option<ResolvedPackage> {
        // Check stdlib first
        if let Some(stdlib) = find_python_stdlib(project_root)
            && let Some(pkg) = resolve_python_stdlib_import(import_name, &stdlib)
        {
            return Some(pkg);
        }

        // Then site-packages
        if let Some(site_packages) = find_python_site_packages(project_root) {
            return resolve_python_import(import_name, &site_packages);
        }

        None
    }

    fn is_stdlib_import(&self, import_name: &str, project_root: &Path) -> bool {
        if let Some(stdlib) = find_python_stdlib(project_root) {
            is_python_stdlib_module(import_name, &stdlib)
        } else {
            false
        }
    }

    fn get_version(&self, project_root: &Path) -> Option<String> {
        get_python_version(project_root)
    }

    fn find_package_cache(&self, project_root: &Path) -> Option<PathBuf> {
        find_python_site_packages(project_root)
    }

    fn indexable_extensions(&self) -> &'static [&'static str] {
        &["py"]
    }

    fn find_stdlib(&self, project_root: &Path) -> Option<PathBuf> {
        find_python_stdlib(project_root)
    }

    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool {
        // Skip private modules
        if name.starts_with('_') {
            return true;
        }
        // Skip __pycache__, dist-info, egg-info
        if name == "__pycache__" || name.ends_with(".dist-info") || name.ends_with(".egg-info") {
            return true;
        }
        // Skip non-Python files
        if !is_dir && !name.ends_with(".py") {
            return true;
        }
        false
    }

    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() {
            return Some(path.to_path_buf());
        }
        // Python packages use __init__.py as entry point
        let init_py = path.join("__init__.py");
        if init_py.is_file() {
            return Some(init_py);
        }
        None
    }

    fn package_module_name(&self, entry_name: &str) -> String {
        // Strip .py extension
        entry_name
            .strip_suffix(".py")
            .unwrap_or(entry_name)
            .to_string()
    }

    fn package_sources(&self, project_root: &Path) -> Vec<crate::PackageSource> {
        let mut sources = Vec::new();
        if let Some(stdlib) = self.find_stdlib(project_root) {
            sources.push(crate::PackageSource {
                name: "stdlib",
                path: stdlib,
                kind: crate::PackageSourceKind::Flat,
                version_specific: true,
            });
        }
        if let Some(cache) = self.find_package_cache(project_root) {
            sources.push(crate::PackageSource {
                name: "site-packages",
                path: cache,
                kind: crate::PackageSourceKind::Flat,
                version_specific: false,
            });
        }
        sources
    }

    fn discover_packages(&self, source: &crate::PackageSource) -> Vec<(String, PathBuf)> {
        self.discover_flat_packages(&source.path)
    }

    fn file_path_to_module_name(&self, path: &Path) -> Option<String> {
        // Only Python files
        if path.extension()?.to_str()? != "py" {
            return None;
        }

        // Remove extension
        let stem = path.with_extension("");
        let stem_str = stem.to_str()?;

        // Strip common source directory prefixes
        let module_path = stem_str
            .strip_prefix("src/")
            .or_else(|| stem_str.strip_prefix("lib/"))
            .unwrap_or(stem_str);

        // Handle __init__.py - use parent directory as module
        let module_path = if module_path.ends_with("/__init__") {
            module_path.strip_suffix("/__init__")?
        } else {
            module_path
        };

        // Convert path separators to dots
        Some(module_path.replace('/', "."))
    }

    fn module_name_to_paths(&self, module: &str) -> Vec<String> {
        // Convert dots to path separators
        let rel_path = module.replace('.', "/");

        // Try common source directories and both .py and __init__.py
        let mut candidates = Vec::with_capacity(4);
        for prefix in &["src/", ""] {
            candidates.push(format!("{}{}.py", prefix, rel_path));
            candidates.push(format!("{}{}/__init__.py", prefix, rel_path));
        }
        candidates
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GrammarLoader;
    use tree_sitter::Parser;

    struct ParseResult {
        tree: tree_sitter::Tree,
        #[allow(dead_code)]
        loader: GrammarLoader,
    }

    fn parse_python(content: &str) -> ParseResult {
        let loader = GrammarLoader::new();
        let language = loader.get("python").unwrap();
        let mut parser = Parser::new();
        parser.set_language(&language).unwrap();
        ParseResult {
            tree: parser.parse(content, None).unwrap(),
            loader,
        }
    }

    #[test]
    fn test_python_function_kinds() {
        let support = Python;
        assert!(support.function_kinds().contains(&"function_definition"));
        // async functions are function_definition with "async" keyword as first child
    }

    #[test]
    fn test_python_extract_function() {
        let support = Python;
        let content = r#"def foo(x: int) -> str:
    """Convert to string."""
    return str(x)
"#;
        let result = parse_python(content);
        let root = result.tree.root_node();

        // Find function node
        let mut cursor = root.walk();
        let func = root
            .children(&mut cursor)
            .find(|n| n.kind() == "function_definition")
            .unwrap();

        let sym = support.extract_function(&func, content, false).unwrap();
        assert_eq!(sym.name, "foo");
        assert_eq!(sym.kind, SymbolKind::Function);
        assert!(sym.signature.contains("def foo(x: int) -> str"));
        assert_eq!(sym.docstring, Some("Convert to string.".to_string()));
    }

    #[test]
    fn test_python_extract_class() {
        let support = Python;
        let content = r#"class Foo(Bar):
    """A foo class."""
    pass
"#;
        let result = parse_python(content);
        let root = result.tree.root_node();

        let mut cursor = root.walk();
        let class = root
            .children(&mut cursor)
            .find(|n| n.kind() == "class_definition")
            .unwrap();

        let sym = support.extract_container(&class, content).unwrap();
        assert_eq!(sym.name, "Foo");
        assert_eq!(sym.kind, SymbolKind::Class);
        assert!(sym.signature.contains("class Foo(Bar)"));
        assert_eq!(sym.docstring, Some("A foo class.".to_string()));
    }

    #[test]
    fn test_python_visibility() {
        let support = Python;
        let content = r#"def public(): pass
def _protected(): pass
def __private(): pass
def __dunder__(): pass
"#;
        let result = parse_python(content);
        let root = result.tree.root_node();

        let mut cursor = root.walk();
        let funcs: Vec<_> = root
            .children(&mut cursor)
            .filter(|n| n.kind() == "function_definition")
            .collect();

        assert_eq!(
            support.get_visibility(&funcs[0], content),
            Visibility::Public
        );
        assert_eq!(
            support.get_visibility(&funcs[1], content),
            Visibility::Protected
        );
        assert_eq!(
            support.get_visibility(&funcs[2], content),
            Visibility::Private
        );
        assert_eq!(
            support.get_visibility(&funcs[3], content),
            Visibility::Public
        ); // dunder
    }

    /// Documents node kinds that exist in the Python grammar but aren't used in trait methods.
    /// Each exclusion has a reason. Review periodically as features expand.
    ///
    /// Run `cross_check_node_kinds` in registry.rs to see all potentially useful kinds.
    #[test]
    fn unused_node_kinds_audit() {
        use crate::validate_unused_kinds_audit;

        // Categories:
        // - STRUCTURAL: Internal/wrapper nodes, not semantically meaningful on their own
        // - CLAUSE: Sub-parts of statements, handled via parent (e.g., else_clause in if_statement)
        // - EXPRESSION: Expressions don't create control flow/scope, we track statements
        // - TYPE: Type annotation nodes, not relevant for current analysis
        // - LEGACY: Python 2 compatibility, not worth supporting
        // - OPERATOR: Operators within expressions, too granular
        // - TODO: Potentially useful, to be added when needed

        #[rustfmt::skip]
        let documented_unused: &[&str] = &[
            // STRUCTURAL
            "aliased_import",          // used internally by extract_imports
            "block",                   // generic block wrapper (duplicate in grammar)
            "expression_list",         // comma-separated expressions
            "identifier",              // too common, used everywhere
            "import_prefix",           // dots in relative imports
            "lambda_parameters",       // internal to lambda
            "module",                  // root node of file
            "parenthesized_expression",// grouping only
            "relative_import",         // handled in extract_imports
            "tuple_expression",        // comma-separated values
            "wildcard_import",         // handled in extract_imports

            // CLAUSE (sub-parts of statements)
            "case_pattern",            // internal to case_clause
            "class_pattern",           // pattern in match/case
            "elif_clause",             // part of if_statement
            "else_clause",             // part of if/for/while/try
            "finally_clause",          // part of try_statement
            "for_in_clause",           // internal to comprehensions
            "if_clause",               // internal to comprehensions
            "with_clause",             // internal to with_statement
            "with_item",               // internal to with_statement

            // EXPRESSION (don't affect control flow structure)
            "await",                   // await keyword, not a statement
            "format_expression",       // f-string interpolation
            "format_specifier",        // f-string format spec
            "named_expression",        // walrus operator :=
            "yield",                   // yield keyword form

            // TYPE (type annotations)
            "constrained_type",        // type constraints
            "generic_type",            // parameterized types
            "member_type",             // attribute access in types
            "splat_type",              // *args/**kwargs types
            "type",                    // generic type node
            "type_alias_statement",    // could track as symbol
            "type_conversion",         // !r/!s/!a in f-strings
            "type_parameter",          // generic type params
            "typed_default_parameter", // param with type and default
            "typed_parameter",         // param with type annotation
            "union_type",              // X | Y union syntax

            // OPERATOR
            "binary_operator",         // +, -, *, /, etc.
            "boolean_operator",        // and/or - handled in complexity_nodes as keywords
            "comparison_operator",     // ==, <, >, etc.
            "not_operator",            // not keyword
            "unary_operator",          // -, +, ~

            // LEGACY (Python 2)
            "exec_statement",          // Python 2 exec
            "print_statement",         // Python 2 print

            // TODO: Potentially useful
            "decorated_definition",    // wrapper for @decorator
            "delete_statement",        // del statement
            "future_import_statement", // from __future__
            "global_statement",        // scope modifier
            "nonlocal_statement",      // scope modifier
            "pass_statement",          // no-op, detect empty bodies
        ];

        validate_unused_kinds_audit(&Python, documented_unused)
            .expect("Python unused node kinds audit failed");
    }
}
