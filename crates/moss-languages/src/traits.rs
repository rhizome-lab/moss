//! Core trait for language support.

use std::path::{Path, PathBuf};
use moss_core::tree_sitter::Node;
use crate::external_packages::ResolvedPackage;

/// Symbol kind classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Enum,
    Trait,
    Interface,
    Module,
    Type,
    Constant,
    Variable,
    Heading,
}

impl SymbolKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SymbolKind::Function => "function",
            SymbolKind::Method => "method",
            SymbolKind::Class => "class",
            SymbolKind::Struct => "struct",
            SymbolKind::Enum => "enum",
            SymbolKind::Trait => "trait",
            SymbolKind::Interface => "interface",
            SymbolKind::Module => "module",
            SymbolKind::Type => "type",
            SymbolKind::Constant => "constant",
            SymbolKind::Variable => "variable",
            SymbolKind::Heading => "heading",
        }
    }
}

/// Symbol visibility
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Visibility {
    #[default]
    Public,
    Private,
    Protected,
    Internal,
}

/// How a language determines symbol visibility
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisibilityMechanism {
    /// Explicit export keyword (JS/TS: `export function foo()`)
    ExplicitExport,
    /// Access modifier keywords (Java, Scala, C#: `public`, `private`, `protected`)
    AccessModifier,
    /// Naming convention (Go: uppercase = public, Python: underscore = private)
    NamingConvention,
    /// Header-based (C/C++: symbols in headers are public, source files are private)
    HeaderBased,
    /// Everything is public by default (Ruby modules, Lua)
    AllPublic,
    /// Not applicable (data formats like JSON, YAML, TOML)
    NotApplicable,
}

/// A code symbol extracted from source
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub signature: String,
    pub docstring: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub visibility: Visibility,
    pub children: Vec<Symbol>,
}

/// An import statement
#[derive(Debug, Clone)]
pub struct Import {
    pub module: String,
    pub names: Vec<String>,
    pub alias: Option<String>,
    pub is_wildcard: bool,
    pub is_relative: bool,
    pub line: usize,
}

/// An export declaration
#[derive(Debug, Clone)]
pub struct Export {
    pub name: String,
    pub kind: SymbolKind,
    pub line: usize,
}

// === Helper functions for should_skip_package_entry ===

/// Check if name is a dotfile/dotdir (starts with '.')
pub fn skip_dotfiles(name: &str) -> bool {
    name.starts_with('.')
}

/// Check if name has one of the given extensions
pub fn has_extension(name: &str, extensions: &[&str]) -> bool {
    extensions.iter().any(|ext| name.ends_with(&format!(".{}", ext)))
}

/// Unified language support trait.
///
/// Each language implements this trait to provide:
/// - Node kind classification
/// - Symbol extraction (functions, classes, types)
/// - Import/export parsing
/// - Complexity analysis nodes
/// - Visibility detection
/// - Edit support (container bodies, docstrings)
pub trait Language: Send + Sync {
    /// Display name for this language (e.g., "Python", "C++")
    fn name(&self) -> &'static str;

    /// File extensions this language handles (e.g., ["py", "pyi", "pyw"])
    fn extensions(&self) -> &'static [&'static str];

    /// Grammar name for arborium (e.g., "python", "rust")
    fn grammar_name(&self) -> &'static str;

    /// Whether this language has code symbols (functions, classes, etc.)
    /// Default: true if function_kinds or container_kinds is non-empty
    fn has_symbols(&self) -> bool {
        !self.function_kinds().is_empty() || !self.container_kinds().is_empty()
    }

    // === Node Classification ===

    /// Container nodes that can hold methods (class, impl, module)
    fn container_kinds(&self) -> &'static [&'static str];

    /// Function/method definition nodes
    fn function_kinds(&self) -> &'static [&'static str];

    /// Type definition nodes (struct, enum, interface, type alias)
    fn type_kinds(&self) -> &'static [&'static str];

    /// Import statement nodes
    fn import_kinds(&self) -> &'static [&'static str];

    /// AST node kinds that may contain publicly visible symbols.
    /// For JS/TS: export_statement nodes.
    /// For Go/Java/Python: function/class/type declaration nodes.
    /// The extract_public_symbols() method filters by actual visibility.
    fn public_symbol_kinds(&self) -> &'static [&'static str];

    /// How this language determines symbol visibility
    fn visibility_mechanism(&self) -> VisibilityMechanism;

    // === Symbol Extraction ===

    /// Extract symbol from a function/method node
    fn extract_function(&self, node: &Node, content: &str, in_container: bool) -> Option<Symbol>;

    /// Extract symbol from a container node (class, impl, module)
    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol>;

    /// Extract symbol from a type definition node
    fn extract_type(&self, node: &Node, content: &str) -> Option<Symbol> {
        // Default: types are often containers too
        self.extract_container(node, content)
    }

    /// Extract docstring/doc comment for a node
    fn extract_docstring(&self, node: &Node, content: &str) -> Option<String> {
        let _ = (node, content);
        None
    }

    // === Import/Export ===

    /// Extract imports from an import node (may return multiple)
    fn extract_imports(&self, node: &Node, content: &str) -> Vec<Import> {
        let _ = (node, content);
        Vec::new()
    }

    /// Extract public symbols from a node.
    /// The node is one of the kinds from public_symbol_kinds().
    /// For JS/TS: extracts exported names from export statements.
    /// For Go/Java/Python: checks visibility and returns public symbols.
    fn extract_public_symbols(&self, node: &Node, content: &str) -> Vec<Export> {
        let _ = (node, content);
        Vec::new()
    }

    // === Scope Analysis ===

    /// Nodes that create new variable scopes (for scope analysis)
    /// Includes: loops, blocks, comprehensions, lambdas, with statements
    /// Note: Functions and containers (from function_kinds/container_kinds) also create scopes
    fn scope_creating_kinds(&self) -> &'static [&'static str];

    // === Control Flow ===

    /// Nodes that affect control flow (for CFG analysis)
    /// Includes: if, for, while, return, break, continue, try, match
    fn control_flow_kinds(&self) -> &'static [&'static str];

    // === Complexity ===

    /// Nodes that increase cyclomatic complexity
    fn complexity_nodes(&self) -> &'static [&'static str];

    /// Nodes that indicate nesting depth
    fn nesting_nodes(&self) -> &'static [&'static str];

    // === Visibility ===

    /// Check if a node is public/exported
    fn is_public(&self, node: &Node, content: &str) -> bool {
        let _ = (node, content);
        true
    }

    /// Get visibility of a node
    fn get_visibility(&self, node: &Node, content: &str) -> Visibility {
        if self.is_public(node, content) {
            Visibility::Public
        } else {
            Visibility::Private
        }
    }

    // === Edit Support ===

    /// Find the body node of a container (for prepend/append)
    fn container_body<'a>(&self, node: &'a Node<'a>) -> Option<Node<'a>> {
        node.child_by_field_name("body")
    }

    /// Detect if first child of body is a docstring
    fn body_has_docstring(&self, body: &Node, content: &str) -> bool {
        let _ = (body, content);
        false
    }

    // === Helpers ===

    /// Get the name of a node (typically via "name" field)
    fn node_name<'a>(&self, node: &Node, content: &'a str) -> Option<&'a str> {
        let name_node = node.child_by_field_name("name")?;
        Some(&content[name_node.byte_range()])
    }

    /// Convert a file path to a module name for this language.
    /// Used to find "importers" - files that import a given file.
    /// Returns None for languages without module systems or where not applicable.
    fn file_path_to_module_name(&self, _path: &Path) -> Option<String> {
        None
    }

    // === Import Resolution ===

    /// Language key for package index cache (e.g., "python", "go", "js").
    fn lang_key(&self) -> &'static str {
        ""
    }

    /// Resolve a local import within the project.
    ///
    /// Handles project-relative imports (e.g., `from . import foo`, `crate::`,
    /// `./module`, relative includes).
    fn resolve_local_import(
        &self,
        import_name: &str,
        current_file: &Path,
        project_root: &Path,
    ) -> Option<PathBuf> {
        let _ = (import_name, current_file, project_root);
        None
    }

    /// Resolve an external import to its source location.
    ///
    /// Returns the path to stdlib or installed packages.
    fn resolve_external_import(
        &self,
        import_name: &str,
        project_root: &Path,
    ) -> Option<ResolvedPackage> {
        let _ = (import_name, project_root);
        None
    }

    /// Check if an import is from the standard library.
    fn is_stdlib_import(&self, import_name: &str, project_root: &Path) -> bool {
        let _ = (import_name, project_root);
        false
    }

    /// Get the language/runtime version (for package index versioning).
    fn get_version(&self, project_root: &Path) -> Option<String> {
        let _ = project_root;
        None
    }

    /// Find package cache/installation directory.
    fn find_package_cache(&self, project_root: &Path) -> Option<PathBuf> {
        let _ = project_root;
        None
    }

    /// File extensions to index when caching a package.
    fn indexable_extensions(&self) -> &'static [&'static str] {
        &[]
    }

    // === Package Indexing ===

    /// Find standard library directory (if applicable).
    /// Returns None for languages without a separate stdlib to index.
    fn find_stdlib(&self, _project_root: &Path) -> Option<PathBuf> {
        None
    }

    /// Should this entry be skipped when indexing packages?
    /// Called for each file/directory in package directories.
    /// Use helper functions `skip_dotfiles()` and `has_indexable_extension()` for common checks.
    fn should_skip_package_entry(&self, name: &str, is_dir: bool) -> bool;

    /// Get the module/package name from a directory entry name.
    /// Default: strip common extensions.
    fn package_module_name(&self, entry_name: &str) -> String {
        // Strip common extensions
        for ext in self.indexable_extensions() {
            let with_dot = format!(".{}", ext);
            if entry_name.ends_with(&with_dot) {
                return entry_name.trim_end_matches(&with_dot).to_string();
            }
        }
        entry_name.to_string()
    }

    /// Return package sources to index for this language.
    /// Each source describes a directory containing packages.
    /// Default: returns stdlib and package cache if available.
    fn package_sources(&self, project_root: &Path) -> Vec<PackageSource> {
        let mut sources = Vec::new();
        if let Some(stdlib) = self.find_stdlib(project_root) {
            sources.push(PackageSource {
                name: "stdlib",
                path: stdlib,
                kind: PackageSourceKind::Flat,
                version_specific: true,
            });
        }
        if let Some(cache) = self.find_package_cache(project_root) {
            sources.push(PackageSource {
                name: "packages",
                path: cache,
                kind: PackageSourceKind::Flat,
                version_specific: false,
            });
        }
        sources
    }

    /// Discover packages in a source directory.
    /// Returns (package_name, path) pairs for all packages found.
    /// Default implementation handles Flat, Recursive, and NpmScoped kinds.
    /// Languages with special source kinds (Maven, Gradle, Cargo, Deno) should override.
    fn discover_packages(&self, source: &PackageSource) -> Vec<(String, PathBuf)> {
        match source.kind {
            PackageSourceKind::Flat => self.discover_flat_packages(&source.path),
            PackageSourceKind::Recursive => self.discover_recursive_packages(&source.path, &source.path),
            PackageSourceKind::NpmScoped => self.discover_npm_scoped_packages(&source.path),
            // Languages using these kinds must override discover_packages
            PackageSourceKind::Maven
            | PackageSourceKind::Gradle
            | PackageSourceKind::Cargo
            | PackageSourceKind::Deno => Vec::new(),
        }
    }

    /// Discover packages in a flat directory (each entry is a package).
    fn discover_flat_packages(&self, source_path: &Path) -> Vec<(String, PathBuf)> {
        let entries = match std::fs::read_dir(source_path) {
            Ok(e) => e,
            Err(_) => return Vec::new(),
        };

        let mut packages = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            if self.should_skip_package_entry(&name, path.is_dir()) {
                continue;
            }

            let module_name = self.package_module_name(&name);
            packages.push((module_name, path));
        }
        packages
    }

    /// Discover packages recursively (each file with matching extension is a package).
    fn discover_recursive_packages(&self, base_path: &Path, current_path: &Path) -> Vec<(String, PathBuf)> {
        let entries = match std::fs::read_dir(current_path) {
            Ok(e) => e,
            Err(_) => return Vec::new(),
        };

        let mut packages = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = path.is_dir();

            if self.should_skip_package_entry(&name, is_dir) {
                continue;
            }

            if is_dir {
                packages.extend(self.discover_recursive_packages(base_path, &path));
            } else {
                // Get relative path from base as module name
                let rel_path = path.strip_prefix(base_path)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| name);
                packages.push((rel_path, path));
            }
        }
        packages
    }

    /// Find the entry point file for a package path.
    /// If path is a file, returns it directly.
    /// If path is a directory, looks for language-specific entry points.
    fn find_package_entry(&self, path: &Path) -> Option<PathBuf> {
        if path.is_file() {
            return Some(path.to_path_buf());
        }
        // Default: no entry point for directories
        // Languages should override to specify their entry points
        None
    }

    /// Discover packages in npm-scoped directory (handles @scope/package).
    fn discover_npm_scoped_packages(&self, source_path: &Path) -> Vec<(String, PathBuf)> {
        let entries = match std::fs::read_dir(source_path) {
            Ok(e) => e,
            Err(_) => return Vec::new(),
        };

        let mut packages = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            if self.should_skip_package_entry(&name, path.is_dir()) {
                continue;
            }

            if name.starts_with('@') && path.is_dir() {
                // Scoped package - iterate contents
                if let Ok(scoped_entries) = std::fs::read_dir(&path) {
                    for scoped_entry in scoped_entries.flatten() {
                        let scoped_path = scoped_entry.path();
                        let scoped_name = scoped_entry.file_name().to_string_lossy().to_string();
                        if self.should_skip_package_entry(&scoped_name, scoped_path.is_dir()) {
                            continue;
                        }
                        let full_name = format!("{}/{}", name, scoped_name);
                        packages.push((full_name, scoped_path));
                    }
                }
            } else {
                let module_name = self.package_module_name(&name);
                packages.push((module_name, path));
            }
        }
        packages
    }
}

/// A source of packages to index.
#[derive(Debug, Clone)]
pub struct PackageSource {
    /// Display name (e.g., "stdlib", "site-packages", "node_modules")
    pub name: &'static str,
    /// Path to the source directory
    pub path: PathBuf,
    /// How to traverse this source
    pub kind: PackageSourceKind,
    /// Whether packages here are version-specific (affects max_version in index)
    pub version_specific: bool,
}

/// How to traverse a package source directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageSourceKind {
    /// Flat directory of packages (Python site-packages, node_modules)
    /// Each top-level entry is a package.
    Flat,
    /// Recursive directory (Go stdlib, C++ includes)
    /// Packages are identified by having indexable files.
    Recursive,
    /// NPM-style scoped packages (@scope/package)
    NpmScoped,
    /// Maven repository structure (group/artifact/version)
    Maven,
    /// Gradle cache structure (group/artifact/version/hash)
    Gradle,
    /// Cargo registry structure (index/crate-version)
    Cargo,
    /// Deno cache structure (needs special handling for npm vs URL deps)
    Deno,
}
