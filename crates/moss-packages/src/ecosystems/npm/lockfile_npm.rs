//! package-lock.json parser (npm)

use crate::{DependencyTree, PackageError, TreeNode};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Get installed version from package-lock.json
pub fn installed_version(package: &str, project_root: &Path) -> Option<String> {
    let lockfile = find_lockfile(project_root)?;
    let content = std::fs::read_to_string(&lockfile).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;

    // v2/v3 format: packages["node_modules/pkg"]
    if let Some(pkgs) = parsed.get("packages").and_then(|p| p.as_object()) {
        let key = format!("node_modules/{}", package);
        if let Some(pkg) = pkgs.get(&key) {
            if let Some(v) = pkg.get("version").and_then(|v| v.as_str()) {
                return Some(v.to_string());
            }
        }
    }

    // v1 format: dependencies["pkg"]
    if let Some(deps) = parsed.get("dependencies").and_then(|d| d.as_object()) {
        if let Some(pkg) = deps.get(package) {
            if let Some(v) = pkg.get("version").and_then(|v| v.as_str()) {
                return Some(v.to_string());
            }
        }
    }

    None
}

/// Build dependency tree from package-lock.json
pub fn dependency_tree(project_root: &Path) -> Option<Result<DependencyTree, PackageError>> {
    let lockfile = find_lockfile(project_root)?;
    let content = std::fs::read_to_string(&lockfile).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;
    Some(build_tree(&parsed))
}

fn find_lockfile(project_root: &Path) -> Option<std::path::PathBuf> {
    let mut current = project_root.to_path_buf();
    loop {
        let lockfile = current.join("package-lock.json");
        if lockfile.exists() {
            return Some(lockfile);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

/// Build dependency tree from parsed package-lock.json
/// Exposed for testing
fn build_tree(parsed: &serde_json::Value) -> Result<DependencyTree, PackageError> {
    let name = parsed
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("root");
    let version = parsed
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("0.0.0");

    // v2/v3 format: packages["node_modules/..."]
    let packages = parsed.get("packages").and_then(|p| p.as_object());

    if let Some(packages) = packages {
        // Build adjacency map from node_modules structure
        let mut deps_map: HashMap<String, Vec<(String, String)>> = HashMap::new();

        for (path, info) in packages {
            if path.is_empty() {
                continue; // Skip root
            }

            // Strip leading "node_modules/" prefix, then split on "/node_modules/"
            // "node_modules/foo" -> "foo" (parent: root)
            // "node_modules/foo/node_modules/bar" -> "bar" (parent: "foo")
            let path_stripped = path.strip_prefix("node_modules/").unwrap_or(path);
            let parts: Vec<&str> = path_stripped.split("/node_modules/").collect();
            let pkg_name = parts.last().unwrap_or(&"");
            let pkg_version = info.get("version").and_then(|v| v.as_str()).unwrap_or("");

            // Parent is everything before the last /node_modules/
            let parent = if parts.len() > 1 {
                parts[..parts.len() - 1].join("/node_modules/")
            } else {
                String::new() // root
            };

            deps_map
                .entry(parent)
                .or_default()
                .push((pkg_name.to_string(), pkg_version.to_string()));
        }

        fn build_node(
            name: &str,
            version: &str,
            parent_path: &str,
            deps_map: &HashMap<String, Vec<(String, String)>>,
            visited: &mut HashSet<String>,
        ) -> TreeNode {
            let children = if visited.contains(name) {
                Vec::new()
            } else {
                visited.insert(name.to_string());
                let child_path = if parent_path.is_empty() {
                    name.to_string()
                } else {
                    format!("{}/node_modules/{}", parent_path, name)
                };
                deps_map
                    .get(&child_path)
                    .map(|deps| {
                        deps.iter()
                            .map(|(n, v)| build_node(n, v, &child_path, deps_map, visited))
                            .collect()
                    })
                    .unwrap_or_default()
            };

            TreeNode {
                name: name.to_string(),
                version: version.to_string(),
                dependencies: children,
            }
        }

        // Build root children
        let mut visited = HashSet::new();
        let root_deps = deps_map
            .get("")
            .map(|deps| {
                deps.iter()
                    .map(|(n, v)| build_node(n, v, "", &deps_map, &mut visited))
                    .collect()
            })
            .unwrap_or_default();

        let root = TreeNode {
            name: name.to_string(),
            version: version.to_string(),
            dependencies: root_deps,
        };

        Ok(DependencyTree { roots: vec![root] })
    } else {
        // No packages section, return minimal tree
        Ok(DependencyTree {
            roots: vec![TreeNode {
                name: name.to_string(),
                version: version.to_string(),
                dependencies: Vec::new(),
            }],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_tree_v2_format() {
        // package-lock.json v2/v3 format with packages section
        let json = r#"{
            "name": "my-app",
            "version": "1.0.0",
            "lockfileVersion": 3,
            "packages": {
                "": {
                    "name": "my-app",
                    "version": "1.0.0"
                },
                "node_modules/react": {
                    "version": "18.2.0",
                    "resolved": "https://registry.npmjs.org/react/-/react-18.2.0.tgz"
                },
                "node_modules/react/node_modules/loose-envify": {
                    "version": "1.4.0"
                },
                "node_modules/typescript": {
                    "version": "5.0.0",
                    "dev": true
                }
            }
        }"#;

        let parsed: serde_json::Value = serde_json::from_str(json).unwrap();
        let tree = build_tree(&parsed).unwrap();

        assert_eq!(tree.roots.len(), 1);
        let root = &tree.roots[0];
        assert_eq!(root.name, "my-app");
        assert_eq!(root.version, "1.0.0");
        assert_eq!(root.dependencies.len(), 2); // react and typescript

        // Find react and check its transitive dep
        let react = root
            .dependencies
            .iter()
            .find(|d| d.name == "react")
            .unwrap();
        assert_eq!(react.version, "18.2.0");
        assert_eq!(react.dependencies.len(), 1);
        assert_eq!(react.dependencies[0].name, "loose-envify");
        assert_eq!(react.dependencies[0].version, "1.4.0");
    }

    #[test]
    fn test_build_tree_scoped_packages() {
        let json = r#"{
            "name": "app",
            "version": "0.1.0",
            "packages": {
                "": {},
                "node_modules/@types/node": {
                    "version": "20.11.0"
                },
                "node_modules/@babel/core": {
                    "version": "7.24.0"
                },
                "node_modules/@babel/core/node_modules/@babel/parser": {
                    "version": "7.24.1"
                }
            }
        }"#;

        let parsed: serde_json::Value = serde_json::from_str(json).unwrap();
        let tree = build_tree(&parsed).unwrap();

        let root = &tree.roots[0];
        assert_eq!(root.dependencies.len(), 2);

        let types_node = root
            .dependencies
            .iter()
            .find(|d| d.name == "@types/node")
            .unwrap();
        assert_eq!(types_node.version, "20.11.0");

        let babel_core = root
            .dependencies
            .iter()
            .find(|d| d.name == "@babel/core")
            .unwrap();
        assert_eq!(babel_core.version, "7.24.0");
        assert_eq!(babel_core.dependencies.len(), 1);
        assert_eq!(babel_core.dependencies[0].name, "@babel/parser");
    }

    #[test]
    fn test_build_tree_empty_packages() {
        let json = r#"{
            "name": "empty-project",
            "version": "0.0.1"
        }"#;

        let parsed: serde_json::Value = serde_json::from_str(json).unwrap();
        let tree = build_tree(&parsed).unwrap();

        assert_eq!(tree.roots[0].name, "empty-project");
        assert_eq!(tree.roots[0].version, "0.0.1");
        assert!(tree.roots[0].dependencies.is_empty());
    }
}
