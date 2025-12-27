//! pnpm-lock.yaml parser

use crate::{DependencyTree, PackageError, TreeNode};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Get installed version from pnpm-lock.yaml
pub fn installed_version(package: &str, project_root: &Path) -> Option<String> {
    let lockfile = find_lockfile(project_root)?;
    let content = std::fs::read_to_string(&lockfile).ok()?;
    let parsed: serde_yaml::Value = serde_yaml::from_str(&content).ok()?;

    // Check packages section for the package
    // Format: packages["package@version"] or packages["/package@version"]
    if let Some(packages) = parsed.get("packages").and_then(|p| p.as_mapping()) {
        for (key, _value) in packages {
            if let Some(key_str) = key.as_str() {
                // Keys are like "@scope/pkg@1.0.0" or "pkg@1.0.0"
                let key_trimmed = key_str.trim_start_matches('/');
                if let Some((name, version)) = parse_package_key(key_trimmed) {
                    if name == package {
                        return Some(version);
                    }
                }
            }
        }
    }

    // Also check importers for direct dependencies
    if let Some(importers) = parsed.get("importers").and_then(|i| i.as_mapping()) {
        for (_importer_path, importer) in importers {
            for dep_type in ["dependencies", "devDependencies", "optionalDependencies"] {
                if let Some(deps) = importer.get(dep_type).and_then(|d| d.as_mapping()) {
                    if let Some(dep) = deps.get(package) {
                        if let Some(version_info) = dep.get("version").and_then(|v| v.as_str()) {
                            // Version might have peer dep suffix like "1.0.0(peer@2.0.0)"
                            let version = version_info.split('(').next().unwrap_or(version_info);
                            return Some(version.to_string());
                        }
                    }
                }
            }
        }
    }

    None
}

/// Build dependency tree from pnpm-lock.yaml
pub fn dependency_tree(project_root: &Path) -> Option<Result<DependencyTree, PackageError>> {
    let lockfile = find_lockfile(project_root)?;
    let content = std::fs::read_to_string(&lockfile).ok()?;
    let parsed: serde_yaml::Value = serde_yaml::from_str(&content).ok()?;
    Some(build_tree(&parsed, project_root))
}

fn find_lockfile(project_root: &Path) -> Option<std::path::PathBuf> {
    let mut current = project_root.to_path_buf();
    loop {
        let lockfile = current.join("pnpm-lock.yaml");
        if lockfile.exists() {
            return Some(lockfile);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

/// Parse package key like "@scope/pkg@1.0.0" into (name, version)
fn parse_package_key(key: &str) -> Option<(String, String)> {
    // Handle scoped packages: @scope/pkg@version
    if key.starts_with('@') {
        // Find the second @ which separates name from version
        let first_slash = key.find('/')?;
        let version_at = key[first_slash..].find('@').map(|i| i + first_slash)?;
        let name = &key[..version_at];
        let version = &key[version_at + 1..];
        Some((name.to_string(), version.to_string()))
    } else {
        // Non-scoped: pkg@version
        let at_pos = key.find('@')?;
        let name = &key[..at_pos];
        let version = &key[at_pos + 1..];
        Some((name.to_string(), version.to_string()))
    }
}

/// Extract package name from a snapshot key like "mermaid@11.12.2" or
/// "@braintree/sanitize-url@7.1.1" or "vitepress@1.6.4(@algolia/...)".
fn parse_snapshot_key(key: &str) -> Option<(String, String)> {
    // Strip peer dep suffix: "pkg@1.0.0(peer@2.0.0)" -> "pkg@1.0.0"
    let key = key.split('(').next().unwrap_or(key);
    parse_package_key(key)
}

/// Build a map of package@version -> list of dependency package@version strings
fn build_deps_map(parsed: &serde_yaml::Value) -> HashMap<String, Vec<String>> {
    let mut deps_map = HashMap::new();

    if let Some(snapshots) = parsed.get("snapshots").and_then(|s| s.as_mapping()) {
        for (key, value) in snapshots {
            if let Some(key_str) = key.as_str() {
                if let Some((name, version)) = parse_snapshot_key(key_str) {
                    let pkg_key = format!("{}@{}", name, version);
                    let mut deps = Vec::new();

                    // Get dependencies
                    if let Some(dep_map) = value.get("dependencies").and_then(|d| d.as_mapping()) {
                        for (dep_name, dep_version) in dep_map {
                            if let (Some(name), Some(ver)) =
                                (dep_name.as_str(), dep_version.as_str())
                            {
                                // Version might have peer suffix
                                let ver = ver.split('(').next().unwrap_or(ver);
                                deps.push(format!("{}@{}", name, ver));
                            }
                        }
                    }

                    // Also include optionalDependencies
                    if let Some(dep_map) = value
                        .get("optionalDependencies")
                        .and_then(|d| d.as_mapping())
                    {
                        for (dep_name, dep_version) in dep_map {
                            if let (Some(name), Some(ver)) =
                                (dep_name.as_str(), dep_version.as_str())
                            {
                                let ver = ver.split('(').next().unwrap_or(ver);
                                deps.push(format!("{}@{}", name, ver));
                            }
                        }
                    }

                    deps_map.insert(pkg_key, deps);
                }
            }
        }
    }

    deps_map
}

/// Recursively build a TreeNode, tracking visited packages to avoid cycles
fn build_node(
    name: &str,
    version: &str,
    deps_map: &HashMap<String, Vec<String>>,
    visited: &mut HashSet<String>,
    max_depth: usize,
) -> TreeNode {
    let pkg_key = format!("{}@{}", name, version);

    // Avoid infinite recursion on cycles
    if visited.contains(&pkg_key) || max_depth == 0 {
        return TreeNode {
            name: name.to_string(),
            version: version.to_string(),
            dependencies: Vec::new(),
        };
    }

    visited.insert(pkg_key.clone());

    let mut children = Vec::new();
    if let Some(deps) = deps_map.get(&pkg_key) {
        for dep_key in deps {
            if let Some((dep_name, dep_version)) = parse_package_key(dep_key) {
                children.push(build_node(
                    &dep_name,
                    &dep_version,
                    deps_map,
                    visited,
                    max_depth - 1,
                ));
            }
        }
    }

    visited.remove(&pkg_key);

    TreeNode {
        name: name.to_string(),
        version: version.to_string(),
        dependencies: children,
    }
}

fn build_tree(
    parsed: &serde_yaml::Value,
    project_root: &Path,
) -> Result<DependencyTree, PackageError> {
    // Get project name from package.json
    let pkg_json = project_root.join("package.json");
    let (name, version) = if let Ok(content) = std::fs::read_to_string(&pkg_json) {
        if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content) {
            (
                pkg.get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("root")
                    .to_string(),
                pkg.get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("0.0.0")
                    .to_string(),
            )
        } else {
            ("root".to_string(), "0.0.0".to_string())
        }
    } else {
        ("root".to_string(), "0.0.0".to_string())
    };

    // Build the dependency map from snapshots
    let deps_map = build_deps_map(parsed);

    let mut root_deps = Vec::new();
    let mut visited = HashSet::new();
    const MAX_DEPTH: usize = 50; // Prevent runaway recursion

    // Get direct dependencies from importers section
    if let Some(importers) = parsed.get("importers").and_then(|i| i.as_mapping()) {
        // Root importer is "."
        if let Some(root_importer) = importers
            .get(".")
            .or_else(|| importers.get(&serde_yaml::Value::String(".".to_string())))
        {
            for dep_type in ["dependencies", "devDependencies"] {
                if let Some(deps) = root_importer.get(dep_type).and_then(|d| d.as_mapping()) {
                    for (dep_name, dep_info) in deps {
                        if let (Some(dep_name_str), Some(version_info)) = (
                            dep_name.as_str(),
                            dep_info.get("version").and_then(|v| v.as_str()),
                        ) {
                            // Version might have peer dep suffix
                            let dep_version =
                                version_info.split('(').next().unwrap_or(version_info);
                            root_deps.push(build_node(
                                dep_name_str,
                                dep_version,
                                &deps_map,
                                &mut visited,
                                MAX_DEPTH,
                            ));
                        }
                    }
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_package_key_simple() {
        let (name, version) = parse_package_key("react@18.2.0").unwrap();
        assert_eq!(name, "react");
        assert_eq!(version, "18.2.0");
    }

    #[test]
    fn test_parse_package_key_scoped() {
        let (name, version) = parse_package_key("@types/node@20.0.0").unwrap();
        assert_eq!(name, "@types/node");
        assert_eq!(version, "20.0.0");
    }

    #[test]
    fn test_parse_snapshot_key_with_peer_suffix() {
        // pnpm adds peer dep info in parentheses
        let (name, version) =
            parse_snapshot_key("vitepress@1.6.4(@algolia/client-search@5.0.0)").unwrap();
        assert_eq!(name, "vitepress");
        assert_eq!(version, "1.6.4");

        let (name, version) = parse_snapshot_key("@vue/compiler-core@3.5.0(vue@3.5.0)").unwrap();
        assert_eq!(name, "@vue/compiler-core");
        assert_eq!(version, "3.5.0");
    }

    #[test]
    fn test_build_deps_map() {
        let yaml = r#"
lockfileVersion: '9.0'

snapshots:
  react@18.2.0:
    dependencies:
      loose-envify: 1.4.0

  loose-envify@1.4.0:
    dependencies:
      js-tokens: 4.0.0

  js-tokens@4.0.0: {}

  typescript@5.4.5: {}

  "@types/node@20.11.0":
    dependencies:
      undici-types: 5.26.5

  undici-types@5.26.5: {}
"#;

        let parsed: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let deps_map = build_deps_map(&parsed);

        // Check react has loose-envify as dependency
        assert!(deps_map.contains_key("react@18.2.0"));
        let react_deps = deps_map.get("react@18.2.0").unwrap();
        assert!(react_deps.contains(&"loose-envify@1.4.0".to_string()));

        // Check loose-envify has js-tokens
        let loose_deps = deps_map.get("loose-envify@1.4.0").unwrap();
        assert!(loose_deps.contains(&"js-tokens@4.0.0".to_string()));

        // Check packages with no deps
        let ts_deps = deps_map.get("typescript@5.4.5").unwrap();
        assert!(ts_deps.is_empty());

        // Check scoped package
        let types_node_deps = deps_map.get("@types/node@20.11.0").unwrap();
        assert!(types_node_deps.contains(&"undici-types@5.26.5".to_string()));
    }

    #[test]
    fn test_build_deps_map_with_optional() {
        let yaml = r#"
snapshots:
  esbuild@0.21.0:
    optionalDependencies:
      "@esbuild/linux-x64": 0.21.0
      "@esbuild/darwin-arm64": 0.21.0
"#;

        let parsed: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let deps_map = build_deps_map(&parsed);

        let esbuild_deps = deps_map.get("esbuild@0.21.0").unwrap();
        assert_eq!(esbuild_deps.len(), 2);
        assert!(esbuild_deps.contains(&"@esbuild/linux-x64@0.21.0".to_string()));
        assert!(esbuild_deps.contains(&"@esbuild/darwin-arm64@0.21.0".to_string()));
    }
}
