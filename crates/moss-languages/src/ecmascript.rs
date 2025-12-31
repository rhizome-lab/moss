//! Shared ECMAScript (JavaScript/TypeScript) support functions.
//!
//! This module contains common logic shared between JavaScript, TypeScript, and TSX.
//! Each language struct delegates to these functions for DRY implementation.

use crate::external_packages::ResolvedPackage;
use crate::{Export, Import, Symbol, SymbolKind, Visibility};
use std::path::{Path, PathBuf};
use std::process::Command;
use tree_sitter::Node;

// ============================================================================
// Node kind constants
// ============================================================================

pub const CONTAINER_KINDS: &[&str] = &["class_declaration", "class"];

pub const JS_FUNCTION_KINDS: &[&str] = &[
    "function_declaration",
    "method_definition",
    "generator_function_declaration",
];
pub const TS_FUNCTION_KINDS: &[&str] = &["function_declaration", "method_definition"];

pub const JS_TYPE_KINDS: &[&str] = &["class_declaration"];
pub const TS_TYPE_KINDS: &[&str] = &[
    "class_declaration",
    "interface_declaration",
    "type_alias_declaration",
    "enum_declaration",
];

pub const IMPORT_KINDS: &[&str] = &["import_statement"];
pub const PUBLIC_SYMBOL_KINDS: &[&str] = &["export_statement"];

pub const SCOPE_CREATING_KINDS: &[&str] = &[
    "for_statement",
    "for_in_statement",
    "while_statement",
    "do_statement",
    "try_statement",
    "catch_clause",
    "switch_statement",
    "arrow_function",
];

pub const CONTROL_FLOW_KINDS: &[&str] = &[
    "if_statement",
    "for_statement",
    "for_in_statement",
    "while_statement",
    "do_statement",
    "switch_statement",
    "try_statement",
    "return_statement",
    "break_statement",
    "continue_statement",
    "throw_statement",
];

pub const COMPLEXITY_NODES: &[&str] = &[
    "if_statement",
    "for_statement",
    "for_in_statement",
    "while_statement",
    "do_statement",
    "switch_case",
    "catch_clause",
    "ternary_expression",
    "binary_expression",
];

pub const NESTING_NODES: &[&str] = &[
    "if_statement",
    "for_statement",
    "for_in_statement",
    "while_statement",
    "do_statement",
    "switch_statement",
    "try_statement",
    "function_declaration",
    "method_definition",
    "class_declaration",
];

// ============================================================================
// Symbol extraction
// ============================================================================

/// Extract a function/method symbol from a node.
pub fn extract_function(node: &Node, content: &str, in_container: bool, name: &str) -> Symbol {
    let params = node
        .child_by_field_name("parameters")
        .map(|p| content[p.byte_range()].to_string())
        .unwrap_or_else(|| "()".to_string());

    let signature = if node.kind() == "method_definition" {
        format!("{}{}", name, params)
    } else {
        format!("function {}{}", name, params)
    };

    Symbol {
        name: name.to_string(),
        kind: if in_container {
            SymbolKind::Method
        } else {
            SymbolKind::Function
        },
        signature,
        docstring: None,
        attributes: Vec::new(),
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        visibility: Visibility::Public,
        children: Vec::new(),
        is_interface_impl: false,
    }
}

/// Extract a class container symbol from a node.
pub fn extract_container(node: &Node, name: &str) -> Symbol {
    Symbol {
        name: name.to_string(),
        kind: SymbolKind::Class,
        signature: format!("class {}", name),
        docstring: None,
        attributes: Vec::new(),
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        visibility: Visibility::Public,
        children: Vec::new(),
        is_interface_impl: false,
    }
}

/// Extract a TypeScript type symbol (interface, type alias, enum).
pub fn extract_type(node: &Node, name: &str) -> Option<Symbol> {
    let (kind, keyword) = match node.kind() {
        "interface_declaration" => (SymbolKind::Interface, "interface"),
        "type_alias_declaration" => (SymbolKind::Type, "type"),
        "enum_declaration" => (SymbolKind::Enum, "enum"),
        "class_declaration" => (SymbolKind::Class, "class"),
        _ => return None,
    };

    Some(Symbol {
        name: name.to_string(),
        kind,
        signature: format!("{} {}", keyword, name),
        docstring: None,
        attributes: Vec::new(),
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        visibility: Visibility::Public,
        children: Vec::new(),
        is_interface_impl: false,
    })
}

// ============================================================================
// Import/Export extraction
// ============================================================================

/// Extract imports from an import_statement node.
pub fn extract_imports(node: &Node, content: &str) -> Vec<Import> {
    if node.kind() != "import_statement" {
        return Vec::new();
    }

    let line = node.start_position().row + 1;
    let mut module = String::new();
    let mut names = Vec::new();

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "string" | "string_fragment" => {
                let text = &content[child.byte_range()];
                module = text.trim_matches(|c| c == '"' || c == '\'').to_string();
            }
            "import_clause" => {
                collect_import_names(&child, content, &mut names);
            }
            _ => {}
        }
    }

    if module.is_empty() {
        return Vec::new();
    }

    vec![Import {
        module: module.clone(),
        names,
        alias: None,
        is_wildcard: false,
        is_relative: module.starts_with('.'),
        line,
    }]
}

/// Format an import as JavaScript/TypeScript source code.
pub fn format_import(import: &Import, names: Option<&[&str]>) -> String {
    let names_to_use: Vec<&str> = names
        .map(|n| n.to_vec())
        .unwrap_or_else(|| import.names.iter().map(|s| s.as_str()).collect());

    if import.is_wildcard {
        format!("import * from '{}';", import.module)
    } else if names_to_use.is_empty() {
        format!("import '{}';", import.module)
    } else if names_to_use.len() == 1 {
        format!("import {{ {} }} from '{}';", names_to_use[0], import.module)
    } else {
        format!(
            "import {{ {} }} from '{}';",
            names_to_use.join(", "),
            import.module
        )
    }
}

fn collect_import_names(import_clause: &Node, content: &str, names: &mut Vec<String>) {
    let mut cursor = import_clause.walk();
    for child in import_clause.children(&mut cursor) {
        match child.kind() {
            "identifier" => {
                // Default import: import foo from './module'
                names.push(content[child.byte_range()].to_string());
            }
            "named_imports" => {
                // { foo, bar }
                let mut inner_cursor = child.walk();
                for inner in child.children(&mut inner_cursor) {
                    if inner.kind() == "import_specifier" {
                        if let Some(name_node) = inner.child_by_field_name("name") {
                            names.push(content[name_node.byte_range()].to_string());
                        }
                    }
                }
            }
            "namespace_import" => {
                // import * as foo
                if let Some(name_node) = child.child_by_field_name("name") {
                    names.push(format!("* as {}", &content[name_node.byte_range()]));
                }
            }
            _ => {}
        }
    }
}

/// Extract exports from an export_statement node.
pub fn extract_public_symbols(node: &Node, content: &str) -> Vec<Export> {
    if node.kind() != "export_statement" {
        return Vec::new();
    }

    let line = node.start_position().row + 1;
    let mut exports = Vec::new();

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_declaration" | "generator_function_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    exports.push(Export {
                        name: content[name_node.byte_range()].to_string(),
                        kind: SymbolKind::Function,
                        line,
                    });
                }
            }
            "class_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    exports.push(Export {
                        name: content[name_node.byte_range()].to_string(),
                        kind: SymbolKind::Class,
                        line,
                    });
                }
            }
            "lexical_declaration" => {
                // export const foo = ...
                let mut decl_cursor = child.walk();
                for decl_child in child.children(&mut decl_cursor) {
                    if decl_child.kind() == "variable_declarator" {
                        if let Some(name_node) = decl_child.child_by_field_name("name") {
                            exports.push(Export {
                                name: content[name_node.byte_range()].to_string(),
                                kind: SymbolKind::Variable,
                                line,
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }

    exports
}

// ============================================================================
// Import resolution
// ============================================================================

/// Resolve a relative import to a local file path.
pub fn resolve_local_import(
    module: &str,
    current_file: &Path,
    extensions: &[&str],
) -> Option<PathBuf> {
    // Only handle relative imports
    if !module.starts_with('.') {
        return None;
    }

    let current_dir = current_file.parent()?;

    // Normalize the path
    let target = if module.starts_with("./") {
        current_dir.join(&module[2..])
    } else if module.starts_with("../") {
        current_dir.join(module)
    } else {
        return None;
    };

    // First try exact path (might already have extension)
    if target.exists() && target.is_file() {
        return Some(target);
    }

    // Try adding extensions
    for ext in extensions {
        let with_ext = target.with_extension(ext);
        if with_ext.exists() && with_ext.is_file() {
            return Some(with_ext);
        }
    }

    // Try index files in directory
    if target.is_dir() {
        for ext in extensions {
            let index = target.join(format!("index.{}", ext));
            if index.exists() && index.is_file() {
                return Some(index);
            }
        }
    }

    None
}

// ============================================================================
// Node.js external package resolution
// ============================================================================

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
fn resolve_node_import(import_path: &str, node_modules: &Path) -> Option<ResolvedPackage> {
    // Parse package name (handle scoped packages)
    let parsed = parse_node_package_name(import_path);

    let pkg_dir = node_modules.join(&parsed.name);
    if !pkg_dir.is_dir() {
        return None;
    }

    // If there's a subpath, resolve it directly
    if let Some(subpath) = parsed.subpath {
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

/// Parsed node package reference
struct ParsedPackage<'a> {
    name: String,
    subpath: Option<&'a str>,
}

/// Parse a package name into name and optional subpath
fn parse_node_package_name(import_path: &str) -> ParsedPackage<'_> {
    if import_path.starts_with('@') {
        // Scoped package: @scope/name or @scope/name/subpath
        let parts: Vec<&str> = import_path.splitn(3, '/').collect();
        if parts.len() >= 2 {
            let name = format!("{}/{}", parts[0], parts[1]);
            let subpath = if parts.len() > 2 {
                Some(parts[2])
            } else {
                None
            };
            return ParsedPackage { name, subpath };
        }
        ParsedPackage {
            name: import_path.to_string(),
            subpath: None,
        }
    } else {
        // Regular package: name or name/subpath
        if let Some(idx) = import_path.find('/') {
            let name = import_path[..idx].to_string();
            let subpath = Some(&import_path[idx + 1..]);
            ParsedPackage { name, subpath }
        } else {
            ParsedPackage {
                name: import_path.to_string(),
                subpath: None,
            }
        }
    }
}

/// Get the entry point from package.json.
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

/// Extract entry point from an exports value.
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

/// Resolve an external (node_modules) import.
pub fn resolve_external_import(import_name: &str, project_root: &Path) -> Option<ResolvedPackage> {
    if import_name.starts_with('.') || import_name.starts_with('/') {
        return None;
    }

    let node_modules = find_node_modules(project_root)?;
    resolve_node_import(import_name, &node_modules)
}

/// Get the Node.js version.
pub fn get_version() -> Option<String> {
    get_node_version()
}

/// Find the node_modules directory.
pub fn find_package_cache(project_root: &Path) -> Option<PathBuf> {
    find_node_modules(project_root)
}

// Extension preferences for each language variant
pub const JS_EXTENSIONS: &[&str] = &["js", "jsx", "mjs", "cjs"];
pub const TS_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "mts", "mjs"];

// ============================================================================
// Deno external package resolution
// ============================================================================

/// Get Deno version.
pub fn get_deno_version() -> Option<String> {
    let output = Command::new("deno").args(["--version"]).output().ok()?;

    if output.status.success() {
        let version_str = String::from_utf8_lossy(&output.stdout);
        for line in version_str.lines() {
            if line.starts_with("deno ") {
                let version_part = line.strip_prefix("deno ")?;
                let parts: Vec<&str> = version_part.split('.').collect();
                if parts.len() >= 2 {
                    let major = parts[0].trim();
                    let minor = parts[1]
                        .chars()
                        .take_while(|c| c.is_ascii_digit())
                        .collect::<String>();
                    return Some(format!("{}.{}", major, minor));
                }
            }
        }
    }

    None
}

/// Find Deno cache directory.
pub fn find_deno_cache() -> Option<PathBuf> {
    if let Ok(deno_dir) = std::env::var("DENO_DIR") {
        let cache = PathBuf::from(deno_dir);
        if cache.is_dir() {
            return Some(cache);
        }
    }

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
pub fn resolve_deno_import(import_url: &str, cache: &Path) -> Option<ResolvedPackage> {
    if let Some(npm_spec) = import_url.strip_prefix("npm:") {
        return resolve_deno_npm_import(npm_spec, cache);
    }

    if import_url.starts_with("https://") || import_url.starts_with("http://") {
        return resolve_deno_url_import(import_url, cache);
    }

    None
}

fn resolve_deno_npm_import(npm_spec: &str, cache: &Path) -> Option<ResolvedPackage> {
    let npm_cache = cache.join("npm").join("registry.npmjs.org");
    if !npm_cache.is_dir() {
        return None;
    }

    let (pkg_name, version_spec) = if npm_spec.starts_with('@') {
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
    } else if let Some(idx) = npm_spec.rfind('@') {
        (npm_spec[..idx].to_string(), Some(&npm_spec[idx + 1..]))
    } else {
        (npm_spec.to_string(), None)
    };

    let pkg_path = if pkg_name.starts_with('@') {
        let parts: Vec<&str> = pkg_name.splitn(2, '/').collect();
        npm_cache.join(parts[0]).join(parts[1])
    } else {
        npm_cache.join(&pkg_name)
    };

    if !pkg_path.is_dir() {
        return None;
    }

    let version_dir = find_best_version_dir(&pkg_path, version_spec)?;
    let entry = find_package_entry(&version_dir)?;

    Some(ResolvedPackage {
        path: entry,
        name: pkg_name,
        is_namespace: false,
    })
}

fn resolve_deno_url_import(url: &str, cache: &Path) -> Option<ResolvedPackage> {
    let deps_dir = cache.join("deps");
    let url_parsed = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let scheme = if url.starts_with("https://") {
        "https"
    } else {
        "http"
    };

    let scheme_dir = deps_dir.join(scheme);
    if !scheme_dir.is_dir() {
        return None;
    }

    let (host, path) = url_parsed.split_once('/')?;
    let host_dir = scheme_dir.join(host);
    if !host_dir.is_dir() {
        return None;
    }

    if let Ok(entries) = std::fs::read_dir(&host_dir) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            if name.ends_with(".metadata.json") {
                continue;
            }

            let meta_path = host_dir.join(format!("{}.metadata.json", name));
            if meta_path.is_file() {
                if let Ok(meta_content) = std::fs::read_to_string(&meta_path) {
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

fn find_best_version_dir(pkg_path: &Path, version_spec: Option<&str>) -> Option<PathBuf> {
    let entries: Vec<_> = std::fs::read_dir(pkg_path).ok()?.flatten().collect();

    if let Some(spec) = version_spec {
        let exact = pkg_path.join(spec);
        if exact.is_dir() {
            return Some(exact);
        }

        for entry in &entries {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(spec) && entry.path().is_dir() {
                return Some(entry.path());
            }
        }
    }

    let mut versions: Vec<_> = entries.into_iter().filter(|e| e.path().is_dir()).collect();
    versions.sort_by(|a, b| {
        let a_name = a.file_name().to_string_lossy().to_string();
        let b_name = b.file_name().to_string_lossy().to_string();
        deno_version_cmp(&a_name, &b_name)
    });
    versions.last().map(|e| e.path())
}

fn deno_version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
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

/// Find entry point for a JavaScript/TypeScript package.
/// Checks package.json for module/main fields, falls back to index.{js,mjs,cjs,ts}.
pub fn find_package_entry(dir: &Path) -> Option<PathBuf> {
    let pkg_json = dir.join("package.json");
    if pkg_json.is_file() {
        if let Ok(content) = std::fs::read_to_string(&pkg_json) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                for field in &["module", "main"] {
                    if let Some(entry) = json.get(field).and_then(|v| v.as_str()) {
                        let path = dir.join(entry.trim_start_matches("./"));
                        if path.is_file() {
                            return Some(path);
                        }
                        let with_ext = path.with_extension("js");
                        if with_ext.is_file() {
                            return Some(with_ext);
                        }
                    }
                }
            }
        }
    }

    for ext in &["js", "mjs", "cjs", "ts"] {
        let index = dir.join(format!("index.{}", ext));
        if index.is_file() {
            return Some(index);
        }
    }

    None
}
