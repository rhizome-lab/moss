//! npm/yarn/pnpm (Node.js) ecosystem.

use crate::{Dependency, DependencyTree, Ecosystem, LockfileManager, PackageError, PackageInfo, PackageQuery, TreeNode};
use std::path::Path;
use std::process::Command;

pub struct Npm;

impl Ecosystem for Npm {
    fn name(&self) -> &'static str {
        "npm"
    }

    fn manifest_files(&self) -> &'static [&'static str] {
        &["package.json"]
    }

    fn lockfiles(&self) -> &'static [LockfileManager] {
        &[
            LockfileManager {
                filename: "pnpm-lock.yaml",
                manager: "pnpm",
            },
            LockfileManager {
                filename: "yarn.lock",
                manager: "yarn",
            },
            LockfileManager {
                filename: "package-lock.json",
                manager: "npm",
            },
            LockfileManager {
                filename: "bun.lockb",
                manager: "bun",
            },
        ]
    }

    fn tools(&self) -> &'static [&'static str] {
        // Fastest first
        &["bun", "pnpm", "yarn", "npm"]
    }

    fn fetch_info(&self, query: &PackageQuery, tool: &str) -> Result<PackageInfo, PackageError> {
        // Format: package or package@version
        let pkg_spec = match &query.version {
            Some(v) => format!("{}@{}", query.name, v),
            None => query.name.clone(),
        };
        match tool {
            "npm" => fetch_npm_info(&query.name, "npm", &["view", &pkg_spec, "--json"]),
            "yarn" => fetch_npm_info(&query.name, "yarn", &["info", &pkg_spec, "--json"]),
            "pnpm" => fetch_npm_info(&query.name, "pnpm", &["view", &pkg_spec, "--json"]),
            "bun" => fetch_npm_info(&query.name, "bun", &["pm", "view", &pkg_spec]),
            _ => Err(PackageError::ToolFailed(format!("unknown tool: {}", tool))),
        }
    }

    fn installed_version(&self, package: &str, project_root: &Path) -> Option<String> {
        // Try package-lock.json first (most common)
        let lockfile = project_root.join("package-lock.json");
        if let Ok(content) = std::fs::read_to_string(&lockfile) {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
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
            }
        }

        // Try yarn.lock (text format: "pkg@version": resolved "..." version "x.y.z")
        let yarn_lock = project_root.join("yarn.lock");
        if let Ok(content) = std::fs::read_to_string(yarn_lock) {
            // Look for lines like: "react@^18.0.0":
            // Followed by: version "18.2.0"
            let mut in_package = false;
            for line in content.lines() {
                if line.starts_with(&format!("\"{}@", package)) || line.starts_with(&format!("{}@", package)) {
                    in_package = true;
                } else if in_package && line.trim().starts_with("version ") {
                    let version = line.trim().strip_prefix("version ")?;
                    return Some(version.trim_matches('"').to_string());
                } else if !line.starts_with(' ') && !line.is_empty() {
                    in_package = false;
                }
            }
        }

        None
    }

    fn list_dependencies(&self, project_root: &Path) -> Result<Vec<Dependency>, crate::PackageError> {
        let manifest = project_root.join("package.json");
        let content = std::fs::read_to_string(&manifest)
            .map_err(|e| crate::PackageError::ParseError(format!("failed to read package.json: {}", e)))?;
        let parsed: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| crate::PackageError::ParseError(format!("invalid JSON: {}", e)))?;

        let mut deps = Vec::new();

        if let Some(d) = parsed.get("dependencies").and_then(|d| d.as_object()) {
            for (name, version) in d {
                deps.push(Dependency {
                    name: name.clone(),
                    version_req: version.as_str().map(String::from),
                    optional: false,
                });
            }
        }

        if let Some(d) = parsed.get("devDependencies").and_then(|d| d.as_object()) {
            for (name, version) in d {
                deps.push(Dependency {
                    name: name.clone(),
                    version_req: version.as_str().map(String::from),
                    optional: false,
                });
            }
        }

        if let Some(d) = parsed.get("optionalDependencies").and_then(|d| d.as_object()) {
            for (name, version) in d {
                deps.push(Dependency {
                    name: name.clone(),
                    version_req: version.as_str().map(String::from),
                    optional: true,
                });
            }
        }

        Ok(deps)
    }

    fn dependency_tree(&self, project_root: &Path) -> Result<DependencyTree, crate::PackageError> {
        // Find package-lock.json, searching up for monorepo root
        let lockfile = find_npm_lockfile(project_root)?;
        let content = std::fs::read_to_string(&lockfile)
            .map_err(|e| crate::PackageError::ParseError(format!("failed to read lockfile: {}", e)))?;
        let parsed: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| crate::PackageError::ParseError(format!("invalid JSON: {}", e)))?;
        build_npm_tree(&parsed)
    }
}

/// Find package-lock.json, searching up from project_root
fn find_npm_lockfile(project_root: &Path) -> Result<std::path::PathBuf, crate::PackageError> {
    let mut current = project_root.to_path_buf();
    loop {
        // Try various lockfile names
        for name in ["package-lock.json", "pnpm-lock.yaml", "yarn.lock"] {
            let lockfile = current.join(name);
            if lockfile.exists() {
                // For now, only fully support package-lock.json
                if name == "package-lock.json" {
                    return Ok(lockfile);
                }
            }
        }
        if !current.pop() {
            break;
        }
    }
    Err(crate::PackageError::ParseError(format!(
        "package-lock.json not found in {} or parent directories",
        project_root.display()
    )))
}

fn build_npm_tree(parsed: &serde_json::Value) -> Result<DependencyTree, crate::PackageError> {
    let name = parsed.get("name").and_then(|n| n.as_str()).unwrap_or("root");
    let version = parsed.get("version").and_then(|v| v.as_str()).unwrap_or("0.0.0");

    // v2/v3 format: packages["node_modules/..."]
    let packages = parsed.get("packages").and_then(|p| p.as_object());

    if let Some(packages) = packages {
        // Build adjacency map from node_modules structure
        let mut deps_map: std::collections::HashMap<String, Vec<(String, String)>> = std::collections::HashMap::new();

        for (path, info) in packages {
            if path.is_empty() {
                continue; // Skip root
            }

            // Extract package name from path: "node_modules/foo" or "node_modules/foo/node_modules/bar"
            let parts: Vec<&str> = path.split("/node_modules/").collect();
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
            deps_map: &std::collections::HashMap<String, Vec<(String, String)>>,
            visited: &mut std::collections::HashSet<String>,
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
        let mut visited = std::collections::HashSet::new();
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

fn fetch_npm_info(package: &str, tool: &str, args: &[&str]) -> Result<PackageInfo, PackageError> {
    let output = Command::new(tool)
        .args(args)
        .output()
        .map_err(|e| PackageError::ToolFailed(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("404") || stderr.contains("not found") {
            return Err(PackageError::NotFound(package.to_string()));
        }
        return Err(PackageError::ToolFailed(stderr.to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Yarn wraps output in a JSON object with "data" field
    let json_str = if tool == "yarn" {
        extract_yarn_data(&stdout)?
    } else {
        stdout.to_string()
    };

    parse_npm_json(&json_str, package)
}

fn extract_yarn_data(output: &str) -> Result<String, PackageError> {
    // Yarn outputs: {"type":"inspect","data":{...}}
    let parsed: serde_json::Value = serde_json::from_str(output)
        .map_err(|e| PackageError::ParseError(format!("invalid yarn JSON: {}", e)))?;

    if let Some(data) = parsed.get("data") {
        Ok(data.to_string())
    } else {
        // Fallback: maybe it's already the data
        Ok(output.to_string())
    }
}

fn parse_npm_json(json_str: &str, package: &str) -> Result<PackageInfo, PackageError> {
    let v: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| PackageError::ParseError(format!("invalid JSON: {}", e)))?;

    let name = v
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(package)
        .to_string();

    let version = v
        .get("version")
        .and_then(|v| v.as_str())
        .ok_or_else(|| PackageError::ParseError("missing version".to_string()))?
        .to_string();

    let description = v.get("description").and_then(|v| v.as_str()).map(String::from);

    let license = v.get("license").and_then(|v| v.as_str()).map(String::from);

    let homepage = v.get("homepage").and_then(|v| v.as_str()).map(String::from);

    let repository = v
        .get("repository")
        .and_then(|r| {
            if let Some(url) = r.as_str() {
                Some(url.to_string())
            } else {
                r.get("url").and_then(|u| u.as_str()).map(String::from)
            }
        });

    // Dependencies
    let mut dependencies = Vec::new();
    if let Some(deps) = v.get("dependencies").and_then(|d| d.as_object()) {
        for (name, version) in deps {
            dependencies.push(Dependency {
                name: name.clone(),
                version_req: version.as_str().map(String::from),
                optional: false,
            });
        }
    }
    if let Some(deps) = v.get("peerDependencies").and_then(|d| d.as_object()) {
        for (name, version) in deps {
            dependencies.push(Dependency {
                name: name.clone(),
                version_req: version.as_str().map(String::from),
                optional: false,
            });
        }
    }
    if let Some(deps) = v.get("optionalDependencies").and_then(|d| d.as_object()) {
        for (name, version) in deps {
            dependencies.push(Dependency {
                name: name.clone(),
                version_req: version.as_str().map(String::from),
                optional: true,
            });
        }
    }

    // npm doesn't have features like Cargo, but we could map optionalDependencies
    let features = Vec::new();

    Ok(PackageInfo {
        name,
        version,
        description,
        license,
        homepage,
        repository,
        features,
        dependencies,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_npm_json() {
        let json = r#"{
            "name": "react",
            "version": "18.2.0",
            "description": "React is a JavaScript library for building user interfaces.",
            "license": "MIT",
            "homepage": "https://reactjs.org/",
            "repository": {"url": "https://github.com/facebook/react.git"},
            "dependencies": {"loose-envify": "^1.1.0"},
            "peerDependencies": {},
            "optionalDependencies": {}
        }"#;

        let info = parse_npm_json(json, "react").unwrap();
        assert_eq!(info.name, "react");
        assert_eq!(info.version, "18.2.0");
        assert_eq!(info.license, Some("MIT".to_string()));
        assert_eq!(info.dependencies.len(), 1);
        assert_eq!(info.dependencies[0].name, "loose-envify");
    }
}
