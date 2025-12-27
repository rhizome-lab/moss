//! bun.lock (text) and bun.lockb (binary) parser
//!
//! Binary format reference from Bun (MIT License):
//! Copyright (c) 2022 Oven-sh
//! https://github.com/oven-sh/bun/blob/main/src/install/lockfile.zig

use crate::{DependencyTree, PackageError, TreeNode};
use json_strip_comments::strip;
use std::path::Path;

/// Get installed version from bun.lock or bun.lockb
pub fn installed_version(package: &str, project_root: &Path) -> Option<String> {
    // Try text format first (bun.lock)
    if let Some(v) = installed_version_text(package, project_root) {
        return Some(v);
    }
    // Fall back to binary format via bun CLI
    installed_version_binary(package, project_root)
}

fn installed_version_text(package: &str, project_root: &Path) -> Option<String> {
    let lockfile = find_text_lockfile(project_root)?;
    let mut content = std::fs::read_to_string(&lockfile).ok()?;
    strip(&mut content).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;

    // packages section: "pkg": ["pkg@version", registry, {deps}, hash]
    if let Some(packages) = parsed.get("packages").and_then(|p| p.as_object()) {
        if let Some(pkg_info) = packages.get(package) {
            if let Some(arr) = pkg_info.as_array() {
                if let Some(first) = arr.first().and_then(|v| v.as_str()) {
                    // Parse "pkg@version" or "@scope/pkg@version"
                    if let Some(version) = extract_version_from_spec(first) {
                        return Some(version);
                    }
                }
            }
        }
    }

    // Also check workspaces for direct deps
    if let Some(workspaces) = parsed.get("workspaces").and_then(|w| w.as_object()) {
        for (_ws_path, ws_info) in workspaces {
            for dep_type in ["dependencies", "devDependencies", "optionalDependencies"] {
                if let Some(deps) = ws_info.get(dep_type).and_then(|d| d.as_object()) {
                    if deps.contains_key(package) {
                        // Found in manifest, look up in packages
                        if let Some(packages) = parsed.get("packages").and_then(|p| p.as_object()) {
                            if let Some(pkg_info) = packages.get(package) {
                                if let Some(arr) = pkg_info.as_array() {
                                    if let Some(first) = arr.first().and_then(|v| v.as_str()) {
                                        if let Some(version) = extract_version_from_spec(first) {
                                            return Some(version);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

fn installed_version_binary(package: &str, project_root: &Path) -> Option<String> {
    let lockfile = find_binary_lockfile(project_root)?;
    let data = std::fs::read(&lockfile).ok()?;
    let parsed = BunLockbParser::parse(&data)?;
    parsed.find_package_version(package)
}

/// Native parser for bun.lockb binary format
struct BunLockbParser<'a> {
    _data: &'a [u8],
    _format_version: u32,
    packages: Vec<BunPackage>,
}

struct BunPackage {
    name: String,
    version: String,
}

impl<'a> BunLockbParser<'a> {
    const HEADER: &'static [u8] = b"#!/usr/bin/env bun\nbun-lockfile-format-v0\n";

    fn parse(data: &'a [u8]) -> Option<Self> {
        // Validate header
        if data.len() < Self::HEADER.len() + 20 {
            return None;
        }
        if !data.starts_with(Self::HEADER) {
            return None;
        }

        let offset = Self::HEADER.len();

        // Format version (u32 LE)
        let format_version = u32::from_le_bytes(data[offset..offset + 4].try_into().ok()?);

        // Skip meta hash (16 bytes) and continue to package data
        // The binary format is complex; extract packages by scanning for patterns
        let packages = Self::extract_packages(data, offset + 20)?;

        Some(Self {
            _data: data,
            _format_version: format_version,
            packages,
        })
    }

    fn extract_packages(data: &[u8], start: usize) -> Option<Vec<BunPackage>> {
        // Scan binary for package name strings
        // This is heuristic - full format parsing would require porting Bun's Zig code

        let mut names = Vec::new();

        // Scan for package names (usually in first half of file)
        let scan_end = data.len() / 2;
        let mut i = start;
        while i < scan_end {
            if let Some((name, skip)) = Self::try_extract_package_name(&data[i..]) {
                names.push(name);
                i += skip;
            } else {
                i += 1;
            }
        }

        // Deduplicate and sort
        names.sort();
        names.dedup();

        // Create packages (versions are not reliably extractable from binary)
        let packages: Vec<BunPackage> = names
            .into_iter()
            .map(|name| BunPackage {
                name,
                version: "?".to_string(),
            })
            .collect();

        if packages.is_empty() {
            None
        } else {
            Some(packages)
        }
    }

    fn try_extract_package_name(data: &[u8]) -> Option<(String, usize)> {
        if data.len() < 4 {
            return None;
        }

        // Look for null-terminated strings (common in binary formats)
        // Package names should end with \0 or be followed by non-name bytes

        // Find ASCII string run - stop at null or non-name char
        let mut end = 0;
        while end < data.len() && end < 50 {
            let b = data[end];
            if b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_' {
                end += 1;
            } else if b == b'@' && end == 0 {
                // Scoped package start
                end += 1;
            } else if b == b'/' && end > 0 && data[0] == b'@' {
                // Scoped package separator
                end += 1;
            } else {
                break;
            }
        }

        if end < 4 || end > 50 {
            return None;
        }

        // Verify string is properly terminated (null byte or non-printable)
        if end < data.len() && data[end] != 0 && data[end].is_ascii_alphanumeric() {
            // String continues with alphanumeric - might be concatenated
            return None;
        }

        let name = std::str::from_utf8(&data[..end]).ok()?.to_string();

        if !Self::looks_like_package_name(&name) {
            return None;
        }

        // Skip past null terminator if present
        let skip = if end < data.len() && data[end] == 0 {
            end + 1
        } else {
            end
        };

        Some((name, skip.max(4)))
    }

    fn looks_like_package_name(s: &str) -> bool {
        // npm package names have strict rules:
        // - Must be lowercase (or start with @)
        // - Can contain letters, digits, hyphens, underscores
        // - Cannot start with . or _
        // - Scoped packages start with @
        // - Minimum meaningful length is 2 chars

        if s.len() < 2 || s.len() > 50 {
            return false;
        }

        // Skip if looks like a path or URL
        if s.contains("://") || s.starts_with('/') || s.contains('\\') {
            return false;
        }

        // Skip common non-package strings
        if s.starts_with("src.") || s.contains("sizeof") || s.contains("alignof") {
            return false;
        }

        // Skip strings with trailing @ (incomplete scoped package refs)
        if s.ends_with('@') || s.ends_with('/') {
            return false;
        }

        // Skip short strings unless they're common package names
        if s.len() <= 3 {
            return matches!(s, "vue" | "ms" | "lru" | "es5" | "es6");
        }

        // npm packages are almost always all lowercase
        // Skip anything with uppercase letters (very few exceptions)
        if s.chars().any(|c| c.is_ascii_uppercase()) {
            return false;
        }

        // Package names should have reasonable vowel/consonant patterns
        let vowels = s.chars().filter(|&c| "aeiou".contains(c)).count();
        let consonants = s
            .chars()
            .filter(|&c| c.is_ascii_lowercase() && !"aeiou".contains(c))
            .count();

        // Too few vowels suggests random string
        if s.len() > 4 && vowels == 0 {
            return false;
        }

        // Ratio check for longer names
        if s.len() > 6 && !s.contains('-') && !s.contains('/') {
            if vowels < 2 || consonants < 2 {
                return false;
            }
        }

        // Skip strings that look like concatenated names (multiple capital-like patterns)
        // e.g., "fseventsrollup" should be "fsevents" + "rollup"
        // Heuristic: common package names are usually < 15 chars unless hyphenated
        if s.len() > 12 && !s.contains('-') && !s.contains('/') {
            return false;
        }

        // Must have at least 3 lowercase letters to be a real package name
        let lowercase_count = s.chars().filter(|c| c.is_ascii_lowercase()).count();
        if lowercase_count < 3 {
            return false;
        }

        // Valid npm package name pattern
        let first = s.chars().next().unwrap();
        if first == '@' {
            // Scoped package: @scope/name
            if !s.contains('/') {
                return false;
            }
        } else if !first.is_ascii_lowercase() {
            return false;
        }

        true
    }

    fn find_package_version(&self, package: &str) -> Option<String> {
        self.packages
            .iter()
            .find(|p| p.name == package)
            .map(|p| p.version.clone())
    }

    fn to_tree(&self, project_root: &Path) -> DependencyTree {
        let (name, version) = get_project_info_from_package_json(project_root);
        let root_deps: Vec<TreeNode> = self
            .packages
            .iter()
            .map(|p| TreeNode {
                name: p.name.clone(),
                version: p.version.clone(),
                dependencies: Vec::new(),
            })
            .collect();

        DependencyTree {
            roots: vec![TreeNode {
                name,
                version,
                dependencies: root_deps,
            }],
        }
    }
}

/// Build dependency tree from bun.lock or bun.lockb
pub fn dependency_tree(project_root: &Path) -> Option<Result<DependencyTree, PackageError>> {
    // Try text format first
    if let Some(lockfile) = find_text_lockfile(project_root) {
        let mut content = std::fs::read_to_string(&lockfile).ok()?;
        strip(&mut content).ok()?;
        let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;
        return Some(build_tree_text(&parsed, project_root));
    }

    // Try binary format via CLI
    let lockfile = find_binary_lockfile(project_root)?;
    if lockfile.exists() {
        return Some(build_tree_binary(project_root));
    }

    None
}

fn find_text_lockfile(project_root: &Path) -> Option<std::path::PathBuf> {
    let mut current = project_root.to_path_buf();
    loop {
        let lockfile = current.join("bun.lock");
        if lockfile.exists() {
            return Some(lockfile);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

fn find_binary_lockfile(project_root: &Path) -> Option<std::path::PathBuf> {
    let mut current = project_root.to_path_buf();
    loop {
        let lockfile = current.join("bun.lockb");
        if lockfile.exists() {
            return Some(lockfile);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

fn extract_version_from_spec(spec: &str) -> Option<String> {
    // Handle "@scope/pkg@version" or "pkg@version"
    if spec.starts_with('@') {
        // Scoped package: find second @
        let first_slash = spec.find('/')?;
        let version_at = spec[first_slash..].find('@').map(|i| i + first_slash)?;
        Some(spec[version_at + 1..].to_string())
    } else {
        let at_pos = spec.find('@')?;
        Some(spec[at_pos + 1..].to_string())
    }
}

fn build_tree_text(
    parsed: &serde_json::Value,
    project_root: &Path,
) -> Result<DependencyTree, PackageError> {
    // Get project info from package.json or root workspace
    let (name, version) = get_project_info(parsed, project_root);

    let mut root_deps = Vec::new();

    // Get direct dependencies from root workspace
    if let Some(workspaces) = parsed.get("workspaces").and_then(|w| w.as_object()) {
        if let Some(root_ws) = workspaces.get("") {
            for dep_type in ["dependencies", "devDependencies"] {
                if let Some(deps) = root_ws.get(dep_type).and_then(|d| d.as_object()) {
                    for (dep_name, _version_req) in deps {
                        // Look up resolved version in packages
                        let version = if let Some(packages) =
                            parsed.get("packages").and_then(|p| p.as_object())
                        {
                            packages
                                .get(dep_name)
                                .and_then(|p| p.as_array())
                                .and_then(|arr| arr.first())
                                .and_then(|v| v.as_str())
                                .and_then(extract_version_from_spec)
                                .unwrap_or_else(|| "?".to_string())
                        } else {
                            "?".to_string()
                        };

                        root_deps.push(TreeNode {
                            name: dep_name.clone(),
                            version,
                            dependencies: Vec::new(),
                        });
                    }
                }
            }
        }

        // Also add workspace packages
        for (ws_path, ws_info) in workspaces {
            if ws_path.is_empty() {
                continue;
            }
            if let Some(ws_name) = ws_info.get("name").and_then(|n| n.as_str()) {
                let ws_version = ws_info
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("0.0.0");
                root_deps.push(TreeNode {
                    name: ws_name.to_string(),
                    version: ws_version.to_string(),
                    dependencies: Vec::new(),
                });
            }
        }
    }

    let root = TreeNode {
        name,
        version,
        dependencies: root_deps,
    };

    Ok(DependencyTree { roots: vec![root] })
}

fn build_tree_binary(project_root: &Path) -> Result<DependencyTree, PackageError> {
    let lockfile = find_binary_lockfile(project_root)
        .ok_or_else(|| PackageError::ParseError("bun.lockb not found".to_string()))?;

    let data = std::fs::read(&lockfile)
        .map_err(|e| PackageError::ParseError(format!("failed to read bun.lockb: {}", e)))?;

    let parsed = BunLockbParser::parse(&data)
        .ok_or_else(|| PackageError::ParseError("invalid bun.lockb format".to_string()))?;

    Ok(parsed.to_tree(project_root))
}

fn get_project_info(parsed: &serde_json::Value, project_root: &Path) -> (String, String) {
    // Try root workspace first
    if let Some(workspaces) = parsed.get("workspaces").and_then(|w| w.as_object()) {
        if let Some(root_ws) = workspaces.get("") {
            let name = root_ws
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("root");
            let version = root_ws
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("0.0.0");
            return (name.to_string(), version.to_string());
        }
    }

    // Fall back to package.json
    get_project_info_from_package_json(project_root)
}

fn get_project_info_from_package_json(project_root: &Path) -> (String, String) {
    let pkg_json = project_root.join("package.json");
    if let Ok(content) = std::fs::read_to_string(&pkg_json) {
        if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content) {
            let name = pkg
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("root")
                .to_string();
            let version = pkg
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("0.0.0")
                .to_string();
            return (name, version);
        }
    }
    ("root".to_string(), "0.0.0".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_version_simple() {
        assert_eq!(
            extract_version_from_spec("react@18.2.0"),
            Some("18.2.0".to_string())
        );
    }

    #[test]
    fn test_extract_version_scoped() {
        assert_eq!(
            extract_version_from_spec("@types/node@20.0.0"),
            Some("20.0.0".to_string())
        );
    }
}
