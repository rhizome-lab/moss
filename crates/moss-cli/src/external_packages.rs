//! External package resolution for Python and Go.
//!
//! Finds installed packages, stdlib, and resolves import paths to their source files.
//! Uses a global cache at ~/.cache/moss/ for indexed packages.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

// =============================================================================
// Python Path Cache (filesystem-based detection, no subprocess calls)
// =============================================================================

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
                            if ver.contains('.') && ver.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                                // Prefer higher versions
                                if best_version.as_ref().map_or(true, |(v, _)| ver > v.as_str()) {
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
                        let venv_site = venv.join("lib").join(format!("python{}", ver)).join("site-packages");
                        if venv_site.exists() {
                            Some(venv_site)
                        } else {
                            // Fall back to system site-packages
                            let sys_site = stdlib_path.join("site-packages");
                            if sys_site.exists() { Some(sys_site) } else { None }
                        }
                    } else {
                        let sys_site = stdlib_path.join("site-packages");
                        if sys_site.exists() { Some(sys_site) } else { None }
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

// =============================================================================
// Global Cache
// =============================================================================

/// Get the global moss cache directory (~/.cache/moss/).
pub fn get_global_cache_dir() -> Option<PathBuf> {
    // XDG_CACHE_HOME or ~/.cache
    let cache_base = if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        PathBuf::from(xdg)
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".cache")
    } else if let Ok(home) = std::env::var("USERPROFILE") {
        // Windows
        PathBuf::from(home).join(".cache")
    } else {
        return None;
    };

    let moss_cache = cache_base.join("moss");

    // Create if doesn't exist
    if !moss_cache.exists() {
        std::fs::create_dir_all(&moss_cache).ok()?;
    }

    Some(moss_cache)
}

/// Get the path to the unified global package index database.
/// e.g., ~/.cache/moss/packages.db
///
/// Schema:
/// - packages(id, language, name, path, min_major, min_minor, max_major, max_minor, indexed_at)
/// - symbols(id, package_id, name, kind, signature, line)
///
/// Version stored as (major, minor) integers for proper comparison.
/// max_major/max_minor NULL means "any version".
pub fn get_global_packages_db() -> Option<PathBuf> {
    let cache = get_global_cache_dir()?;
    Some(cache.join("packages.db"))
}

/// Get Python version from filesystem structure (no subprocess).
pub fn get_python_version(project_root: &Path) -> Option<String> {
    get_python_cache(project_root).version
}

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

/// Result of resolving an external package
#[derive(Debug, Clone)]
pub struct ResolvedPackage {
    /// Path to the package source
    pub path: PathBuf,
    /// Package name as imported
    pub name: String,
    /// Whether this is a namespace package (no __init__.py)
    pub is_namespace: bool,
}

// =============================================================================
// Python
// =============================================================================

/// Find Python stdlib directory from filesystem structure (no subprocess).
pub fn find_python_stdlib(project_root: &Path) -> Option<PathBuf> {
    get_python_cache(project_root).stdlib
}

/// Check if a module name is a Python stdlib module.
pub fn is_python_stdlib_module(module_name: &str, stdlib_path: &Path) -> bool {
    let top_level = module_name.split('.').next().unwrap_or(module_name);

    // Check for package
    let pkg_dir = stdlib_path.join(top_level);
    if pkg_dir.is_dir() {
        return true;
    }

    // Check for module
    let py_file = stdlib_path.join(format!("{}.py", top_level));
    if py_file.is_file() {
        return true;
    }

    false
}

/// Resolve a Python stdlib import to its source location.
pub fn resolve_python_stdlib_import(import_name: &str, stdlib_path: &Path) -> Option<ResolvedPackage> {
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
pub fn resolve_python_import(import_name: &str, site_packages: &Path) -> Option<ResolvedPackage> {
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

// =============================================================================
// Go
// =============================================================================

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
pub fn is_go_stdlib_import(import_path: &str) -> bool {
    let first_segment = import_path.split('/').next().unwrap_or(import_path);
    !first_segment.contains('.')
}

/// Resolve a Go stdlib import to its source location.
pub fn resolve_go_stdlib_import(import_path: &str, stdlib_path: &Path) -> Option<ResolvedPackage> {
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

/// Resolve a Go import to its source location.
///
/// Import paths like "github.com/user/repo/pkg" are mapped to
/// $GOMODCACHE/github.com/user/repo@version/pkg
pub fn resolve_go_import(import_path: &str, mod_cache: &Path) -> Option<ResolvedPackage> {
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

// =============================================================================
// TypeScript / JavaScript
// =============================================================================

/// Find node_modules directory by walking up from a file.
pub fn find_node_modules(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };

    loop {
        let node_modules = current.join("node_modules");
        if node_modules.is_dir() {
            return Some(node_modules);
        }

        if !current.pop() {
            break;
        }
    }

    None
}

/// Get Node.js version (for index versioning).
pub fn get_node_version() -> Option<String> {
    let output = Command::new("node").args(["--version"]).output().ok()?;

    if output.status.success() {
        let version_str = String::from_utf8_lossy(&output.stdout);
        // "v20.10.0" -> "20.10"
        let ver = version_str.trim().trim_start_matches('v');
        let parts: Vec<&str> = ver.split('.').collect();
        if parts.len() >= 2 {
            return Some(format!("{}.{}", parts[0], parts[1]));
        }
    }

    None
}

/// Resolve a bare import (non-relative) to node_modules.
///
/// Handles:
/// - `lodash` -> `node_modules/lodash`
/// - `@scope/pkg` -> `node_modules/@scope/pkg`
/// - `lodash/fp` -> `node_modules/lodash/fp`
pub fn resolve_node_import(import_path: &str, node_modules: &Path) -> Option<ResolvedPackage> {
    // Parse package name (handle scoped packages)
    let (pkg_name, subpath) = parse_node_package_name(import_path);

    let pkg_dir = node_modules.join(&pkg_name);
    if !pkg_dir.is_dir() {
        return None;
    }

    // If there's a subpath, resolve it directly
    if let Some(subpath) = subpath {
        let target = pkg_dir.join(subpath);
        if let Some(resolved) = resolve_node_file_or_dir(&target) {
            return Some(ResolvedPackage {
                path: resolved,
                name: import_path.to_string(),
                is_namespace: false,
            });
        }
        return None;
    }

    // No subpath - use package.json to find entry point
    let pkg_json = pkg_dir.join("package.json");
    if pkg_json.is_file() {
        if let Some(entry) = get_package_entry_point(&pkg_dir, &pkg_json) {
            return Some(ResolvedPackage {
                path: entry,
                name: import_path.to_string(),
                is_namespace: false,
            });
        }
    }

    // Fall back to index.js
    if let Some(resolved) = resolve_node_file_or_dir(&pkg_dir) {
        return Some(ResolvedPackage {
            path: resolved,
            name: import_path.to_string(),
            is_namespace: false,
        });
    }

    None
}

/// Parse a package name, returning (package_name, optional_subpath).
/// `lodash` -> ("lodash", None)
/// `lodash/fp` -> ("lodash", Some("fp"))
/// `@scope/pkg` -> ("@scope/pkg", None)
/// `@scope/pkg/sub` -> ("@scope/pkg", Some("sub"))
fn parse_node_package_name(import_path: &str) -> (String, Option<&str>) {
    if import_path.starts_with('@') {
        // Scoped package: @scope/name or @scope/name/subpath
        let parts: Vec<&str> = import_path.splitn(3, '/').collect();
        if parts.len() >= 2 {
            let pkg_name = format!("{}/{}", parts[0], parts[1]);
            let subpath = if parts.len() > 2 { Some(parts[2]) } else { None };
            return (pkg_name, subpath);
        }
        (import_path.to_string(), None)
    } else {
        // Regular package: name or name/subpath
        if let Some(idx) = import_path.find('/') {
            let pkg_name = &import_path[..idx];
            let subpath = &import_path[idx + 1..];
            (pkg_name.to_string(), Some(subpath))
        } else {
            (import_path.to_string(), None)
        }
    }
}

/// Get the entry point from package.json.
/// Checks: "exports" (modern), "module" (ESM), "main" (CJS), falls back to index.js
fn get_package_entry_point(pkg_dir: &Path, pkg_json: &Path) -> Option<PathBuf> {
    let content = std::fs::read_to_string(pkg_json).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;

    // Try "exports" field (simplified - just handle string or { ".": ... })
    if let Some(exports) = json.get("exports") {
        if let Some(entry) = exports.as_str() {
            let path = pkg_dir.join(entry.trim_start_matches("./"));
            if path.is_file() {
                return Some(path);
            }
        } else if let Some(obj) = exports.as_object() {
            // Try "." entry
            if let Some(dot) = obj.get(".") {
                if let Some(entry) = extract_export_entry(dot) {
                    let path = pkg_dir.join(entry.trim_start_matches("./"));
                    if path.is_file() {
                        return Some(path);
                    }
                }
            }
        }
    }

    // Try "module" field (ESM entry point)
    if let Some(module) = json.get("module").and_then(|v| v.as_str()) {
        let path = pkg_dir.join(module.trim_start_matches("./"));
        if path.is_file() {
            return Some(path);
        }
    }

    // Try "main" field
    if let Some(main) = json.get("main").and_then(|v| v.as_str()) {
        let path = pkg_dir.join(main.trim_start_matches("./"));
        if let Some(resolved) = resolve_node_file_or_dir(&path) {
            return Some(resolved);
        }
    }

    None
}

/// Extract entry point from an exports value (handles string, { import, require, default }).
fn extract_export_entry(value: &serde_json::Value) -> Option<&str> {
    if let Some(s) = value.as_str() {
        return Some(s);
    }
    if let Some(obj) = value.as_object() {
        // Prefer: import > require > default
        for key in &["import", "require", "default"] {
            if let Some(entry) = obj.get(*key) {
                if let Some(s) = entry.as_str() {
                    return Some(s);
                }
                // Recursive for nested conditions
                if let Some(s) = extract_export_entry(entry) {
                    return Some(s);
                }
            }
        }
    }
    None
}

/// Resolve a path to a file, trying extensions and index files.
fn resolve_node_file_or_dir(target: &Path) -> Option<PathBuf> {
    let extensions = ["js", "mjs", "cjs", "ts", "tsx", "jsx"];

    // Exact file
    if target.is_file() {
        return Some(target.to_path_buf());
    }

    // Try with extensions
    for ext in &extensions {
        let with_ext = target.with_extension(ext);
        if with_ext.is_file() {
            return Some(with_ext);
        }
    }

    // Try as directory with index
    if target.is_dir() {
        for ext in &extensions {
            let index = target.join(format!("index.{}", ext));
            if index.is_file() {
                return Some(index);
            }
        }
    }

    None
}

// =============================================================================
// Rust
// =============================================================================

/// Get Rust version.
pub fn get_rust_version() -> Option<String> {
    let output = Command::new("rustc").args(["--version"]).output().ok()?;

    if output.status.success() {
        let version_str = String::from_utf8_lossy(&output.stdout);
        // "rustc 1.75.0 (82e1608df 2023-12-21)" -> "1.75"
        for part in version_str.split_whitespace() {
            if part.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                let parts: Vec<&str> = part.split('.').collect();
                if parts.len() >= 2 {
                    return Some(format!("{}.{}", parts[0], parts[1]));
                }
            }
        }
    }

    None
}

/// Find cargo registry source directory.
/// Structure: ~/.cargo/registry/src/
pub fn find_cargo_registry() -> Option<PathBuf> {
    // Check CARGO_HOME env var
    if let Ok(cargo_home) = std::env::var("CARGO_HOME") {
        let registry = PathBuf::from(cargo_home).join("registry").join("src");
        if registry.is_dir() {
            return Some(registry);
        }
    }

    // Fall back to ~/.cargo/registry/src
    if let Ok(home) = std::env::var("HOME") {
        let registry = PathBuf::from(home).join(".cargo").join("registry").join("src");
        if registry.is_dir() {
            return Some(registry);
        }
    }

    // Windows fallback
    if let Ok(home) = std::env::var("USERPROFILE") {
        let registry = PathBuf::from(home).join(".cargo").join("registry").join("src");
        if registry.is_dir() {
            return Some(registry);
        }
    }

    None
}

/// Resolve a Rust crate import to its source location.
pub fn resolve_rust_crate(crate_name: &str, registry: &Path) -> Option<ResolvedPackage> {
    // Registry structure: registry/src/index.crates.io-*/crate-version/
    if let Ok(indices) = std::fs::read_dir(registry) {
        for index_entry in indices.flatten() {
            let index_path = index_entry.path();
            if !index_path.is_dir() {
                continue;
            }

            if let Ok(crates) = std::fs::read_dir(&index_path) {
                for crate_entry in crates.flatten() {
                    let crate_dir = crate_entry.path();
                    let dir_name = crate_entry.file_name().to_string_lossy().to_string();

                    // Check if this is our crate (name-version pattern)
                    if dir_name.starts_with(&format!("{}-", crate_name)) {
                        let lib_rs = crate_dir.join("src").join("lib.rs");
                        if lib_rs.is_file() {
                            return Some(ResolvedPackage {
                                path: lib_rs,
                                name: crate_name.to_string(),
                                is_namespace: false,
                            });
                        }
                    }
                }
            }
        }
    }

    None
}

// =============================================================================
// C/C++
// =============================================================================

/// Get GCC version.
pub fn get_gcc_version() -> Option<String> {
    let output = Command::new("gcc").args(["--version"]).output().ok()?;

    if output.status.success() {
        let version_str = String::from_utf8_lossy(&output.stdout);
        // "gcc (GCC) 13.2.0" or "gcc (Ubuntu 11.4.0-1ubuntu1~22.04) 11.4.0"
        for line in version_str.lines() {
            // Look for version number pattern
            for part in line.split_whitespace() {
                if part.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                    let ver_parts: Vec<&str> = part.split('.').collect();
                    if ver_parts.len() >= 2 {
                        return Some(format!("{}.{}", ver_parts[0], ver_parts[1]));
                    }
                }
            }
        }
    }

    // Try clang as fallback
    let output = Command::new("clang").args(["--version"]).output().ok()?;
    if output.status.success() {
        let version_str = String::from_utf8_lossy(&output.stdout);
        for line in version_str.lines() {
            if line.contains("clang version") {
                for part in line.split_whitespace() {
                    if part.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                        let ver_parts: Vec<&str> = part.split('.').collect();
                        if ver_parts.len() >= 2 {
                            return Some(format!("{}.{}", ver_parts[0], ver_parts[1]));
                        }
                    }
                }
            }
        }
    }

    None
}

/// Find C/C++ system include directories.
pub fn find_cpp_include_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // Standard system include paths
    let system_paths = [
        "/usr/include",
        "/usr/local/include",
        "/usr/include/c++",
        "/usr/include/x86_64-linux-gnu",
        "/usr/include/aarch64-linux-gnu",
    ];

    for path in system_paths {
        let p = PathBuf::from(path);
        if p.is_dir() {
            paths.push(p);
        }
    }

    // Try to get GCC include paths
    if let Ok(output) = Command::new("gcc").args(["-E", "-Wp,-v", "-xc", "/dev/null"]).output() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut in_search_list = false;

        for line in stderr.lines() {
            if line.contains("#include <...> search starts here:") {
                in_search_list = true;
                continue;
            }
            if line.contains("End of search list.") {
                break;
            }
            if in_search_list {
                let path = PathBuf::from(line.trim());
                if path.is_dir() && !paths.contains(&path) {
                    paths.push(path);
                }
            }
        }
    }

    // Try clang as well
    if let Ok(output) = Command::new("clang").args(["-E", "-Wp,-v", "-xc", "/dev/null"]).output() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut in_search_list = false;

        for line in stderr.lines() {
            if line.contains("#include <...> search starts here:") {
                in_search_list = true;
                continue;
            }
            if line.contains("End of search list.") {
                break;
            }
            if in_search_list {
                let path = PathBuf::from(line.trim());
                if path.is_dir() && !paths.contains(&path) {
                    paths.push(path);
                }
            }
        }
    }

    // macOS specific paths
    #[cfg(target_os = "macos")]
    {
        // Xcode command line tools
        let xcode_paths = [
            "/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk/usr/include",
            "/Library/Developer/CommandLineTools/usr/include",
            "/Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk/usr/include",
        ];
        for path in xcode_paths {
            let p = PathBuf::from(path);
            if p.is_dir() && !paths.contains(&p) {
                paths.push(p);
            }
        }

        // Homebrew
        let homebrew_paths = [
            "/opt/homebrew/include",
            "/usr/local/include",
        ];
        for path in homebrew_paths {
            let p = PathBuf::from(path);
            if p.is_dir() && !paths.contains(&p) {
                paths.push(p);
            }
        }
    }

    paths
}

/// Resolve a C/C++ include to a header file.
/// Handles: <stdio.h>, <vector>, "myheader.h"
pub fn resolve_cpp_include(include: &str, include_paths: &[PathBuf]) -> Option<ResolvedPackage> {
    // Strip angle brackets or quotes
    let header = include
        .trim_start_matches('<')
        .trim_end_matches('>')
        .trim_start_matches('"')
        .trim_end_matches('"');

    // Search through include paths
    for base_path in include_paths {
        let full_path = base_path.join(header);
        if full_path.is_file() {
            return Some(ResolvedPackage {
                path: full_path,
                name: header.to_string(),
                is_namespace: false,
            });
        }

        // For C++ standard library, might be without extension
        if !header.contains('.') {
            // Try with common extensions
            for ext in &["", ".h", ".hpp", ".hxx"] {
                let with_ext = if ext.is_empty() {
                    base_path.join(header)
                } else {
                    base_path.join(format!("{}{}", header, ext))
                };
                if with_ext.is_file() {
                    return Some(ResolvedPackage {
                        path: with_ext,
                        name: header.to_string(),
                        is_namespace: false,
                    });
                }
            }
        }
    }

    None
}

// =============================================================================
// Java
// =============================================================================

/// Get Java version.
pub fn get_java_version() -> Option<String> {
    let output = Command::new("java").args(["--version"]).output().ok()?;

    if output.status.success() {
        let version_str = String::from_utf8_lossy(&output.stdout);
        // "openjdk 17.0.1 2021-10-19" or "java 21.0.1 2023-10-17 LTS"
        for line in version_str.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let version = parts[1];
                let ver_parts: Vec<&str> = version.split('.').collect();
                if ver_parts.len() >= 2 {
                    return Some(format!("{}.{}", ver_parts[0], ver_parts[1]));
                } else if ver_parts.len() == 1 {
                    return Some(format!("{}.0", ver_parts[0]));
                }
            }
        }
    }

    None
}

/// Find Maven local repository.
/// Default: ~/.m2/repository/
pub fn find_maven_repository() -> Option<PathBuf> {
    // Check M2_HOME or MAVEN_HOME env var
    if let Ok(m2_home) = std::env::var("M2_HOME").or_else(|_| std::env::var("MAVEN_HOME")) {
        let repo = PathBuf::from(m2_home).join("repository");
        if repo.is_dir() {
            return Some(repo);
        }
    }

    // Default ~/.m2/repository
    if let Ok(home) = std::env::var("HOME") {
        let repo = PathBuf::from(home).join(".m2").join("repository");
        if repo.is_dir() {
            return Some(repo);
        }
    }

    // Windows fallback
    if let Ok(home) = std::env::var("USERPROFILE") {
        let repo = PathBuf::from(home).join(".m2").join("repository");
        if repo.is_dir() {
            return Some(repo);
        }
    }

    None
}

/// Find Gradle cache directory.
/// Default: ~/.gradle/caches/modules-2/files-2.1/
pub fn find_gradle_cache() -> Option<PathBuf> {
    // Check GRADLE_USER_HOME env var
    if let Ok(gradle_home) = std::env::var("GRADLE_USER_HOME") {
        let cache = PathBuf::from(gradle_home).join("caches").join("modules-2").join("files-2.1");
        if cache.is_dir() {
            return Some(cache);
        }
    }

    // Default ~/.gradle/caches/modules-2/files-2.1
    if let Ok(home) = std::env::var("HOME") {
        let cache = PathBuf::from(home).join(".gradle").join("caches").join("modules-2").join("files-2.1");
        if cache.is_dir() {
            return Some(cache);
        }
    }

    // Windows fallback
    if let Ok(home) = std::env::var("USERPROFILE") {
        let cache = PathBuf::from(home).join(".gradle").join("caches").join("modules-2").join("files-2.1");
        if cache.is_dir() {
            return Some(cache);
        }
    }

    None
}

/// Resolve a Java import to a source file in Maven/Gradle cache.
/// Note: This resolves to sources JAR if available, otherwise returns the JAR path.
pub fn resolve_java_import(import: &str, maven_repo: Option<&Path>, gradle_cache: Option<&Path>) -> Option<ResolvedPackage> {
    // Java imports are like: com.google.gson.Gson
    // Package structure: com/google/gson/
    // Artifact: com.google.gson:gson:2.10.1

    // For now, try to find a matching package directory
    // Full resolution would require parsing POMs and looking up dependencies

    let package_path = import.replace('.', "/");

    // Try Maven first
    if let Some(maven) = maven_repo {
        if let Some(result) = find_java_package_in_maven(maven, &package_path, import) {
            return Some(result);
        }
    }

    // Try Gradle
    if let Some(gradle) = gradle_cache {
        if let Some(result) = find_java_package_in_gradle(gradle, &package_path, import) {
            return Some(result);
        }
    }

    None
}

/// Find a Java package in Maven repository.
fn find_java_package_in_maven(maven_repo: &Path, package_path: &str, import: &str) -> Option<ResolvedPackage> {
    // Maven structure: group/artifact/version/artifact-version.jar
    // e.g., com/google/gson/gson/2.10.1/gson-2.10.1.jar
    //       com/google/gson/gson/2.10.1/gson-2.10.1-sources.jar

    // Try to find a matching directory structure
    let target_dir = maven_repo.join(package_path);
    if target_dir.is_dir() {
        // This might be the artifact directory, look for version subdirs
        return find_maven_artifact(&target_dir, import);
    }

    // Try parent paths (package path might include class name)
    let parts: Vec<&str> = package_path.split('/').collect();
    for i in (2..parts.len()).rev() {
        let dir_path = parts[..i].join("/");
        let artifact = parts[i - 1];
        let search_dir = maven_repo.join(&dir_path);

        if search_dir.is_dir() {
            if let Some(result) = find_maven_artifact(&search_dir, import) {
                return Some(result);
            }

            // Also try the artifact name directly under group
            let artifact_dir = search_dir.join(artifact);
            if artifact_dir.is_dir() {
                if let Some(result) = find_maven_artifact(&artifact_dir, import) {
                    return Some(result);
                }
            }
        }
    }

    None
}

/// Find an artifact in a Maven artifact directory.
fn find_maven_artifact(artifact_dir: &Path, import: &str) -> Option<ResolvedPackage> {
    // Look for version subdirectories
    let versions: Vec<_> = std::fs::read_dir(artifact_dir).ok()?
        .flatten()
        .filter(|e| e.path().is_dir())
        .collect();

    if versions.is_empty() {
        return None;
    }

    // Sort by version and take the latest
    let mut versions: Vec<_> = versions.into_iter().collect();
    versions.sort_by(|a, b| {
        let a_name = a.file_name().to_string_lossy().to_string();
        let b_name = b.file_name().to_string_lossy().to_string();
        version_cmp(&a_name, &b_name)
    });

    let version_dir = versions.last()?.path();
    let artifact_name = artifact_dir.file_name()?.to_string_lossy().to_string();
    let version = version_dir.file_name()?.to_string_lossy().to_string();

    // Prefer sources JAR
    let sources_jar = version_dir.join(format!("{}-{}-sources.jar", artifact_name, version));
    if sources_jar.is_file() {
        return Some(ResolvedPackage {
            path: sources_jar,
            name: import.to_string(),
            is_namespace: false,
        });
    }

    // Fall back to regular JAR
    let jar = version_dir.join(format!("{}-{}.jar", artifact_name, version));
    if jar.is_file() {
        return Some(ResolvedPackage {
            path: jar,
            name: import.to_string(),
            is_namespace: false,
        });
    }

    None
}

/// Find a Java package in Gradle cache.
fn find_java_package_in_gradle(gradle_cache: &Path, package_path: &str, import: &str) -> Option<ResolvedPackage> {
    // Gradle structure: group/artifact/version/hash/artifact-version.jar
    // e.g., com.google.gson/gson/2.10.1/abc123/gson-2.10.1.jar

    // Convert package path to group (dots for gradle)
    let parts: Vec<&str> = package_path.split('/').collect();

    for i in (2..parts.len()).rev() {
        // Try group.artifact format
        let group = parts[..i - 1].join(".");
        let artifact = parts[i - 1];
        let group_dir = gradle_cache.join(&group);

        if group_dir.is_dir() {
            let artifact_dir = group_dir.join(artifact);
            if artifact_dir.is_dir() {
                if let Some(result) = find_gradle_artifact(&artifact_dir, import) {
                    return Some(result);
                }
            }
        }
    }

    None
}

/// Find an artifact in a Gradle artifact directory.
fn find_gradle_artifact(artifact_dir: &Path, import: &str) -> Option<ResolvedPackage> {
    // Look for version subdirectories
    let versions: Vec<_> = std::fs::read_dir(artifact_dir).ok()?
        .flatten()
        .filter(|e| e.path().is_dir())
        .collect();

    if versions.is_empty() {
        return None;
    }

    // Sort by version and take the latest
    let mut versions: Vec<_> = versions.into_iter().collect();
    versions.sort_by(|a, b| {
        let a_name = a.file_name().to_string_lossy().to_string();
        let b_name = b.file_name().to_string_lossy().to_string();
        version_cmp(&a_name, &b_name)
    });

    let version_dir = versions.last()?.path();

    // Gradle has hash subdirectories
    let hash_dirs: Vec<_> = std::fs::read_dir(&version_dir).ok()?
        .flatten()
        .filter(|e| e.path().is_dir())
        .collect();

    for hash_dir in hash_dirs {
        let hash_path = hash_dir.path();

        // Look for sources JAR first
        if let Ok(entries) = std::fs::read_dir(&hash_path) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with("-sources.jar") {
                    return Some(ResolvedPackage {
                        path: entry.path(),
                        name: import.to_string(),
                        is_namespace: false,
                    });
                }
            }
        }

        // Fall back to regular JAR
        if let Ok(entries) = std::fs::read_dir(&hash_path) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with(".jar") && !name.ends_with("-sources.jar") && !name.ends_with("-javadoc.jar") {
                    return Some(ResolvedPackage {
                        path: entry.path(),
                        name: import.to_string(),
                        is_namespace: false,
                    });
                }
            }
        }
    }

    None
}

// =============================================================================
// Deno
// =============================================================================

/// Get Deno version.
pub fn get_deno_version() -> Option<String> {
    let output = Command::new("deno").args(["--version"]).output().ok()?;

    if output.status.success() {
        let version_str = String::from_utf8_lossy(&output.stdout);
        // "deno 1.40.0 (release, ...)" -> "1.40"
        for line in version_str.lines() {
            if line.starts_with("deno ") {
                let version_part = line.strip_prefix("deno ")?;
                let parts: Vec<&str> = version_part.split('.').collect();
                if parts.len() >= 2 {
                    // First part might have extra chars, get just the number
                    let major = parts[0].trim();
                    let minor = parts[1].chars().take_while(|c| c.is_ascii_digit()).collect::<String>();
                    return Some(format!("{}.{}", major, minor));
                }
            }
        }
    }

    None
}

/// Find Deno cache directory.
/// Structure: $DENO_DIR or ~/.cache/deno (Linux) / ~/Library/Caches/deno (macOS)
pub fn find_deno_cache() -> Option<PathBuf> {
    // Check DENO_DIR env var first
    if let Ok(deno_dir) = std::env::var("DENO_DIR") {
        let cache = PathBuf::from(deno_dir);
        if cache.is_dir() {
            return Some(cache);
        }
    }

    // Platform-specific defaults
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            let cache = PathBuf::from(home).join("Library/Caches/deno");
            if cache.is_dir() {
                return Some(cache);
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        // XDG_CACHE_HOME or ~/.cache
        if let Ok(xdg_cache) = std::env::var("XDG_CACHE_HOME") {
            let cache = PathBuf::from(xdg_cache).join("deno");
            if cache.is_dir() {
                return Some(cache);
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            let cache = PathBuf::from(home).join(".cache/deno");
            if cache.is_dir() {
                return Some(cache);
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            let cache = PathBuf::from(local_app_data).join("deno");
            if cache.is_dir() {
                return Some(cache);
            }
        }
    }

    // Fallback: try common paths
    if let Ok(home) = std::env::var("HOME") {
        for path in &[".cache/deno", "Library/Caches/deno"] {
            let cache = PathBuf::from(&home).join(path);
            if cache.is_dir() {
                return Some(cache);
            }
        }
    }

    None
}

/// Resolve a Deno URL import to its cached location.
/// Handles: https://deno.land/std@version/path, https://esm.sh/package, npm:package
pub fn resolve_deno_import(import_url: &str, cache: &Path) -> Option<ResolvedPackage> {
    // Handle npm: imports
    if let Some(npm_spec) = import_url.strip_prefix("npm:") {
        return resolve_deno_npm_import(npm_spec, cache);
    }

    // Handle https:// imports
    if import_url.starts_with("https://") || import_url.starts_with("http://") {
        return resolve_deno_url_import(import_url, cache);
    }

    None
}

/// Resolve a Deno npm: import.
fn resolve_deno_npm_import(npm_spec: &str, cache: &Path) -> Option<ResolvedPackage> {
    // npm:express@4 or npm:@types/node@20
    // Deno stores npm packages in cache/npm/registry.npmjs.org/package/version/

    let npm_cache = cache.join("npm").join("registry.npmjs.org");
    if !npm_cache.is_dir() {
        return None;
    }

    // Parse package name and version
    let (pkg_name, version_spec) = if npm_spec.starts_with('@') {
        // Scoped package: @scope/name@version
        let parts: Vec<&str> = npm_spec.splitn(3, '/').collect();
        if parts.len() < 2 {
            return None;
        }
        let scope = parts[0];
        let name_ver = parts[1];
        let (name, ver) = if let Some(idx) = name_ver.rfind('@') {
            (&name_ver[..idx], Some(&name_ver[idx + 1..]))
        } else {
            (name_ver, None)
        };
        (format!("{}/{}", scope, name), ver)
    } else {
        // Regular package: name@version
        if let Some(idx) = npm_spec.rfind('@') {
            (npm_spec[..idx].to_string(), Some(&npm_spec[idx + 1..]))
        } else {
            (npm_spec.to_string(), None)
        }
    };

    // Find the package in cache
    let pkg_path = if pkg_name.starts_with('@') {
        // Scoped: npm/registry.npmjs.org/@scope/name/version
        let parts: Vec<&str> = pkg_name.splitn(2, '/').collect();
        npm_cache.join(parts[0]).join(parts[1])
    } else {
        npm_cache.join(&pkg_name)
    };

    if !pkg_path.is_dir() {
        return None;
    }

    // Find matching version directory
    let version_dir = find_best_version_dir(&pkg_path, version_spec)?;

    // Look for entry point
    let entry = find_node_entry_in_dir(&version_dir)?;

    Some(ResolvedPackage {
        path: entry,
        name: pkg_name,
        is_namespace: false,
    })
}

/// Resolve a Deno https:// URL import.
fn resolve_deno_url_import(url: &str, cache: &Path) -> Option<ResolvedPackage> {
    // https://deno.land/std@0.200.0/path/mod.ts
    // -> cache/deps/https/deno.land/<hash>
    // Deno uses content hashing, so we need to find by metadata

    let deps_dir = cache.join("deps");

    // Parse URL
    let url_parsed = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let scheme = if url.starts_with("https://") { "https" } else { "http" };

    let scheme_dir = deps_dir.join(scheme);
    if !scheme_dir.is_dir() {
        return None;
    }

    // Get host and path
    let (host, path) = url_parsed.split_once('/')?;
    let host_dir = scheme_dir.join(host);
    if !host_dir.is_dir() {
        return None;
    }

    // Deno uses hashed filenames, but stores metadata files
    // Look for a file whose .metadata.json contains our URL
    if let Ok(entries) = std::fs::read_dir(&host_dir) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            // Skip metadata files, look for actual cached files
            if name.ends_with(".metadata.json") {
                continue;
            }

            // Check if there's a corresponding metadata file
            let meta_path = host_dir.join(format!("{}.metadata.json", name));
            if meta_path.is_file() {
                if let Ok(meta_content) = std::fs::read_to_string(&meta_path) {
                    // Metadata contains {"url": "...", ...}
                    if meta_content.contains(url) {
                        return Some(ResolvedPackage {
                            path: entry_path,
                            name: format!("{}/{}", host, path),
                            is_namespace: false,
                        });
                    }
                }
            }
        }
    }

    None
}

/// Find the best matching version directory.
fn find_best_version_dir(pkg_path: &Path, version_spec: Option<&str>) -> Option<PathBuf> {
    let entries: Vec<_> = std::fs::read_dir(pkg_path).ok()?.flatten().collect();

    if let Some(spec) = version_spec {
        // Try exact match first
        let exact = pkg_path.join(spec);
        if exact.is_dir() {
            return Some(exact);
        }

        // Try prefix match (e.g., "4" matches "4.18.2")
        for entry in &entries {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(spec) && entry.path().is_dir() {
                return Some(entry.path());
            }
        }
    }

    // Return latest (last in sorted order)
    let mut versions: Vec<_> = entries
        .into_iter()
        .filter(|e| e.path().is_dir())
        .collect();
    versions.sort_by(|a, b| {
        let a_name = a.file_name().to_string_lossy().to_string();
        let b_name = b.file_name().to_string_lossy().to_string();
        version_cmp(&a_name, &b_name)
    });
    versions.last().map(|e| e.path())
}

/// Compare version strings (simple semver-like comparison).
pub fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let a_parts: Vec<u32> = a.split('.').filter_map(|p| p.parse().ok()).collect();
    let b_parts: Vec<u32> = b.split('.').filter_map(|p| p.parse().ok()).collect();

    for (ap, bp) in a_parts.iter().zip(b_parts.iter()) {
        match ap.cmp(bp) {
            std::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }
    a_parts.len().cmp(&b_parts.len())
}

/// Find entry point in a node-style package directory.
fn find_node_entry_in_dir(dir: &Path) -> Option<PathBuf> {
    // Try package.json
    let pkg_json = dir.join("package.json");
    if pkg_json.is_file() {
        if let Ok(content) = std::fs::read_to_string(&pkg_json) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                // Try module, main
                for field in &["module", "main"] {
                    if let Some(entry) = json.get(field).and_then(|v| v.as_str()) {
                        let path = dir.join(entry.trim_start_matches("./"));
                        if path.is_file() {
                            return Some(path);
                        }
                        // Try with .js extension
                        let with_ext = path.with_extension("js");
                        if with_ext.is_file() {
                            return Some(with_ext);
                        }
                    }
                }
            }
        }
    }

    // Fallback to index files
    for ext in &["js", "mjs", "cjs", "ts"] {
        let index = dir.join(format!("index.{}", ext));
        if index.is_file() {
            return Some(index);
        }
    }

    None
}

// =============================================================================
// Global Package Index Database
// =============================================================================

use rusqlite::{Connection, params};

/// Parsed version as (major, minor).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
}

impl Version {
    /// Parse "3.11" into Version { major: 3, minor: 11 }.
    pub fn parse(s: &str) -> Option<Version> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() >= 2 {
            Some(Version {
                major: parts[0].parse().ok()?,
                minor: parts[1].parse().ok()?,
            })
        } else {
            None
        }
    }

    /// Check if this version is within a range [min, max].
    /// If max is None, only checks >= min.
    pub fn in_range(&self, min: Version, max: Option<Version>) -> bool {
        if *self < min {
            return false;
        }
        if let Some(max) = max {
            if *self > max {
                return false;
            }
        }
        true
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.major.cmp(&other.major) {
            std::cmp::Ordering::Equal => self.minor.cmp(&other.minor),
            ord => ord,
        }
    }
}

/// A package record in the index.
#[derive(Debug, Clone)]
pub struct PackageRecord {
    pub id: i64,
    pub language: String,
    pub name: String,
    pub path: String,
    pub min_major: u32,
    pub min_minor: u32,
    pub max_major: Option<u32>,
    pub max_minor: Option<u32>,
}

impl PackageRecord {
    pub fn min_version(&self) -> Version {
        Version { major: self.min_major, minor: self.min_minor }
    }

    pub fn max_version(&self) -> Option<Version> {
        match (self.max_major, self.max_minor) {
            (Some(major), Some(minor)) => Some(Version { major, minor }),
            _ => None,
        }
    }
}

/// A symbol record in the index.
#[derive(Debug, Clone)]
pub struct SymbolRecord {
    pub id: i64,
    pub package_id: i64,
    pub name: String,
    pub kind: String,
    pub signature: String,
    pub line: u32,
}

/// Global package index backed by SQLite.
pub struct PackageIndex {
    conn: Connection,
}

impl PackageIndex {
    /// Open or create the global package index.
    pub fn open() -> Result<Self, rusqlite::Error> {
        let db_path = get_global_packages_db()
            .ok_or_else(|| rusqlite::Error::InvalidPath("Cannot determine cache directory".into()))?;

        let conn = Connection::open(&db_path)?;
        let index = PackageIndex { conn };
        index.init_schema()?;
        Ok(index)
    }

    /// Open an in-memory database (for testing).
    pub fn open_in_memory() -> Result<Self, rusqlite::Error> {
        let conn = Connection::open_in_memory()?;
        let index = PackageIndex { conn };
        index.init_schema()?;
        Ok(index)
    }

    /// Initialize database schema.
    fn init_schema(&self) -> Result<(), rusqlite::Error> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS packages (
                id INTEGER PRIMARY KEY,
                language TEXT NOT NULL,
                name TEXT NOT NULL,
                path TEXT NOT NULL,
                min_major INTEGER NOT NULL,
                min_minor INTEGER NOT NULL,
                max_major INTEGER,
                max_minor INTEGER,
                indexed_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_packages_lang_name ON packages(language, name);

            CREATE TABLE IF NOT EXISTS symbols (
                id INTEGER PRIMARY KEY,
                package_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                signature TEXT NOT NULL,
                line INTEGER NOT NULL,
                FOREIGN KEY (package_id) REFERENCES packages(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_symbols_package ON symbols(package_id);
            CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name);
            "
        )?;
        Ok(())
    }

    /// Insert a package and return its ID.
    pub fn insert_package(
        &self,
        language: &str,
        name: &str,
        path: &str,
        min_version: Version,
        max_version: Option<Version>,
    ) -> Result<i64, rusqlite::Error> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        self.conn.execute(
            "INSERT INTO packages (language, name, path, min_major, min_minor, max_major, max_minor, indexed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                language,
                name,
                path,
                min_version.major,
                min_version.minor,
                max_version.map(|v| v.major),
                max_version.map(|v| v.minor),
                now,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Insert a symbol for a package.
    pub fn insert_symbol(
        &self,
        package_id: i64,
        name: &str,
        kind: &str,
        signature: &str,
        line: u32,
    ) -> Result<i64, rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO symbols (package_id, name, kind, signature, line)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![package_id, name, kind, signature, line],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Find a package by language and name, optionally filtering by version.
    pub fn find_package(
        &self,
        language: &str,
        name: &str,
        version: Option<Version>,
    ) -> Result<Option<PackageRecord>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id, language, name, path, min_major, min_minor, max_major, max_minor
             FROM packages WHERE language = ?1 AND name = ?2"
        )?;

        let packages: Vec<PackageRecord> = stmt.query_map(params![language, name], |row| {
            Ok(PackageRecord {
                id: row.get(0)?,
                language: row.get(1)?,
                name: row.get(2)?,
                path: row.get(3)?,
                min_major: row.get(4)?,
                min_minor: row.get(5)?,
                max_major: row.get(6)?,
                max_minor: row.get(7)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;

        // Filter by version in Rust (simpler than complex SQL)
        if let Some(ver) = version {
            for pkg in packages {
                if ver.in_range(pkg.min_version(), pkg.max_version()) {
                    return Ok(Some(pkg));
                }
            }
            Ok(None)
        } else {
            Ok(packages.into_iter().next())
        }
    }

    /// Get all symbols for a package.
    pub fn get_symbols(&self, package_id: i64) -> Result<Vec<SymbolRecord>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id, package_id, name, kind, signature, line
             FROM symbols WHERE package_id = ?1 ORDER BY line"
        )?;

        let symbols = stmt.query_map(params![package_id], |row| {
            Ok(SymbolRecord {
                id: row.get(0)?,
                package_id: row.get(1)?,
                name: row.get(2)?,
                kind: row.get(3)?,
                signature: row.get(4)?,
                line: row.get(5)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;

        Ok(symbols)
    }

    /// Find a symbol by name across all packages for a language.
    pub fn find_symbol(
        &self,
        language: &str,
        symbol_name: &str,
        version: Option<Version>,
    ) -> Result<Vec<(PackageRecord, SymbolRecord)>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT p.id, p.language, p.name, p.path, p.min_major, p.min_minor, p.max_major, p.max_minor,
                    s.id, s.package_id, s.name, s.kind, s.signature, s.line
             FROM symbols s
             JOIN packages p ON s.package_id = p.id
             WHERE p.language = ?1 AND s.name = ?2"
        )?;

        let results: Vec<(PackageRecord, SymbolRecord)> = stmt.query_map(params![language, symbol_name], |row| {
            Ok((
                PackageRecord {
                    id: row.get(0)?,
                    language: row.get(1)?,
                    name: row.get(2)?,
                    path: row.get(3)?,
                    min_major: row.get(4)?,
                    min_minor: row.get(5)?,
                    max_major: row.get(6)?,
                    max_minor: row.get(7)?,
                },
                SymbolRecord {
                    id: row.get(8)?,
                    package_id: row.get(9)?,
                    name: row.get(10)?,
                    kind: row.get(11)?,
                    signature: row.get(12)?,
                    line: row.get(13)?,
                },
            ))
        })?.collect::<Result<Vec<_>, _>>()?;

        // Filter by version
        if let Some(ver) = version {
            Ok(results.into_iter()
                .filter(|(pkg, _)| ver.in_range(pkg.min_version(), pkg.max_version()))
                .collect())
        } else {
            Ok(results)
        }
    }

    /// Check if a package is already indexed.
    pub fn is_indexed(&self, language: &str, name: &str) -> Result<bool, rusqlite::Error> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM packages WHERE language = ?1 AND name = ?2",
            params![language, name],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Delete a package and its symbols.
    pub fn delete_package(&self, package_id: i64) -> Result<(), rusqlite::Error> {
        self.conn.execute("DELETE FROM symbols WHERE package_id = ?1", params![package_id])?;
        self.conn.execute("DELETE FROM packages WHERE id = ?1", params![package_id])?;
        Ok(())
    }

    /// Clear all packages and symbols from the index.
    pub fn clear(&self) -> Result<(), rusqlite::Error> {
        self.conn.execute("DELETE FROM symbols", [])?;
        self.conn.execute("DELETE FROM packages", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_parsing() {
        assert_eq!(Version::parse("3.11"), Some(Version { major: 3, minor: 11 }));
        assert_eq!(Version::parse("1.21"), Some(Version { major: 1, minor: 21 }));
        assert_eq!(Version::parse("invalid"), None);
    }

    #[test]
    fn test_version_comparison() {
        let v39 = Version { major: 3, minor: 9 };
        let v310 = Version { major: 3, minor: 10 };
        let v311 = Version { major: 3, minor: 11 };

        assert!(v39 < v310);
        assert!(v310 < v311);
        assert!(v311 > v39);
    }

    #[test]
    fn test_version_in_range() {
        let v310 = Version { major: 3, minor: 10 };
        let min = Version { major: 3, minor: 9 };
        let max = Version { major: 3, minor: 12 };

        assert!(v310.in_range(min, Some(max)));
        assert!(v310.in_range(min, None));
        assert!(!Version { major: 3, minor: 8 }.in_range(min, Some(max)));
        assert!(!Version { major: 3, minor: 13 }.in_range(min, Some(max)));
    }

    #[test]
    fn test_package_index() {
        let index = PackageIndex::open_in_memory().unwrap();

        // Insert a package
        let pkg_id = index.insert_package(
            "python",
            "requests",
            "/path/to/requests",
            Version { major: 3, minor: 8 },
            Some(Version { major: 3, minor: 12 }),
        ).unwrap();

        // Insert symbols
        index.insert_symbol(pkg_id, "get", "function", "def get(url, **kwargs) -> Response", 42).unwrap();
        index.insert_symbol(pkg_id, "post", "function", "def post(url, **kwargs) -> Response", 100).unwrap();

        // Find package
        let found = index.find_package("python", "requests", Some(Version { major: 3, minor: 10 })).unwrap();
        assert!(found.is_some());
        let pkg = found.unwrap();
        assert_eq!(pkg.name, "requests");

        // Find with wrong version
        let found = index.find_package("python", "requests", Some(Version { major: 2, minor: 7 })).unwrap();
        assert!(found.is_none());

        // Get symbols
        let symbols = index.get_symbols(pkg_id).unwrap();
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "get");

        // Find symbol by name
        let results = index.find_symbol("python", "get", None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0.name, "requests");
        assert_eq!(results[0].1.name, "get");
    }

    #[test]
    fn test_find_site_packages() {
        // Test with current project (has .venv)
        let root = std::env::current_dir().unwrap();
        let site_packages = find_python_site_packages(&root);
        // This test assumes we're running from moss project root with .venv
        if root.join(".venv").exists() {
            assert!(site_packages.is_some());
            let sp = site_packages.unwrap();
            assert!(sp.to_string_lossy().contains("site-packages"));
        }
    }

    #[test]
    fn test_resolve_python_import() {
        let root = std::env::current_dir().unwrap();
        if let Some(site_packages) = find_python_site_packages(&root) {
            // Try to resolve a common package
            if let Some(pkg) = resolve_python_import("pathlib", &site_packages) {
                // pathlib might be stdlib, skip
                let _ = pkg;
            }

            // Try requests if installed
            if let Some(pkg) = resolve_python_import("ruff", &site_packages) {
                assert!(pkg.path.exists());
                assert_eq!(pkg.name, "ruff");
            }
        }
    }
}
