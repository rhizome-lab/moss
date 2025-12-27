//! yarn.lock parser (Yarn v1 classic format)

use crate::{DependencyTree, PackageError, TreeNode};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Get installed version from yarn.lock
pub fn installed_version(package: &str, project_root: &Path) -> Option<String> {
    let lockfile = find_lockfile(project_root)?;
    let content = std::fs::read_to_string(&lockfile).ok()?;

    // yarn.lock format:
    // "package@^1.0.0":
    //   version "1.2.3"
    //   resolved "..."
    //   ...
    let mut in_package = false;
    for line in content.lines() {
        // Check if this line starts a package entry
        if line.starts_with(&format!("\"{}@", package))
            || line.starts_with(&format!("{}@", package))
        {
            in_package = true;
        } else if in_package && line.trim().starts_with("version ") {
            // Extract version from: version "1.2.3"
            let version = line.trim().strip_prefix("version ")?;
            return Some(version.trim_matches('"').to_string());
        } else if !line.starts_with(' ') && !line.is_empty() {
            // New package entry started
            in_package = false;
        }
    }

    None
}

/// Build dependency tree from yarn.lock
pub fn dependency_tree(project_root: &Path) -> Option<Result<DependencyTree, PackageError>> {
    let lockfile = find_lockfile(project_root)?;
    let _content = std::fs::read_to_string(&lockfile).ok()?;
    Some(build_tree(project_root))
}

fn find_lockfile(project_root: &Path) -> Option<std::path::PathBuf> {
    let mut current = project_root.to_path_buf();
    loop {
        let lockfile = current.join("yarn.lock");
        if lockfile.exists() {
            return Some(lockfile);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

/// Parsed yarn.lock entry
#[derive(Debug)]
struct YarnEntry {
    version: String,
    dependencies: Vec<String>, // dep names (without version range)
}

/// Parse yarn.lock into a map of package name -> (version, deps)
fn parse_yarn_lock(content: &str) -> HashMap<String, YarnEntry> {
    let mut entries: HashMap<String, YarnEntry> = HashMap::new();
    let mut current_packages: Vec<String> = Vec::new();
    let mut current_version: Option<String> = None;
    let mut current_deps: Vec<String> = Vec::new();
    let mut in_dependencies = false;

    for line in content.lines() {
        // Skip comments and empty lines
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }

        // New package entry (not indented, ends with :)
        if !line.starts_with(' ') && line.ends_with(':') {
            // Save previous entry if exists
            if !current_packages.is_empty() {
                if let Some(ver) = current_version.take() {
                    let entry = YarnEntry {
                        version: ver.clone(),
                        dependencies: current_deps.clone(),
                    };
                    for pkg in &current_packages {
                        entries.insert(
                            pkg.clone(),
                            YarnEntry {
                                version: entry.version.clone(),
                                dependencies: entry.dependencies.clone(),
                            },
                        );
                    }
                }
            }

            // Parse new package names (can be multiple: "pkg@^1.0.0", "pkg@^2.0.0":)
            current_packages.clear();
            current_deps.clear();
            in_dependencies = false;

            let line = line.trim_end_matches(':');
            for part in line.split(", ") {
                let part = part.trim().trim_matches('"');
                // Extract package name from "pkg@version" or "@scope/pkg@version"
                if let Some(name) = extract_package_name(part) {
                    current_packages.push(name);
                }
            }
        } else if line.starts_with("  version ") {
            // Version line
            let ver = line.trim().strip_prefix("version ").unwrap_or("");
            current_version = Some(ver.trim_matches('"').to_string());
            in_dependencies = false;
        } else if line.trim() == "dependencies:" {
            in_dependencies = true;
        } else if in_dependencies && line.starts_with("    ") {
            // Dependency line: "    dep-name "version-range""
            let dep_line = line.trim();
            if let Some(space_pos) = dep_line.find(' ') {
                let dep_name = &dep_line[..space_pos];
                current_deps.push(dep_name.trim_matches('"').to_string());
            }
        } else if !line.starts_with("    ") {
            // End of dependencies section
            in_dependencies = false;
        }
    }

    // Save last entry
    if !current_packages.is_empty() {
        if let Some(ver) = current_version {
            let entry = YarnEntry {
                version: ver.clone(),
                dependencies: current_deps,
            };
            for pkg in &current_packages {
                entries.insert(
                    pkg.clone(),
                    YarnEntry {
                        version: entry.version.clone(),
                        dependencies: entry.dependencies.clone(),
                    },
                );
            }
        }
    }

    entries
}

/// Extract package name from "pkg@version" or "@scope/pkg@version"
fn extract_package_name(spec: &str) -> Option<String> {
    if spec.starts_with('@') {
        // Scoped: @scope/pkg@version
        let first_slash = spec.find('/')?;
        let version_at = spec[first_slash..].find('@').map(|i| i + first_slash)?;
        Some(spec[..version_at].to_string())
    } else {
        // Simple: pkg@version
        let at_pos = spec.find('@')?;
        Some(spec[..at_pos].to_string())
    }
}

/// Recursively build a TreeNode
fn build_node(
    name: &str,
    entries: &HashMap<String, YarnEntry>,
    visited: &mut HashSet<String>,
    max_depth: usize,
) -> TreeNode {
    // Find entry for this package
    let entry = entries.get(name);
    let version = entry
        .map(|e| e.version.clone())
        .unwrap_or_else(|| "?".to_string());

    // Avoid cycles and limit depth
    if visited.contains(name) || max_depth == 0 {
        return TreeNode {
            name: name.to_string(),
            version,
            dependencies: Vec::new(),
        };
    }

    visited.insert(name.to_string());

    let children = if let Some(entry) = entry {
        entry
            .dependencies
            .iter()
            .map(|dep| build_node(dep, entries, visited, max_depth - 1))
            .collect()
    } else {
        Vec::new()
    };

    visited.remove(name);

    TreeNode {
        name: name.to_string(),
        version,
        dependencies: children,
    }
}

fn build_tree(project_root: &Path) -> Result<DependencyTree, PackageError> {
    // Get project info and direct dependencies from package.json
    let pkg_json = project_root.join("package.json");
    let content = std::fs::read_to_string(&pkg_json)
        .map_err(|e| PackageError::ParseError(format!("failed to read package.json: {}", e)))?;
    let pkg: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| PackageError::ParseError(format!("invalid JSON: {}", e)))?;

    let name = pkg.get("name").and_then(|n| n.as_str()).unwrap_or("root");
    let version = pkg
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("0.0.0");

    // Parse yarn.lock
    let lockfile = find_lockfile(project_root)
        .ok_or_else(|| PackageError::ParseError("yarn.lock not found".to_string()))?;
    let lock_content = std::fs::read_to_string(&lockfile)
        .map_err(|e| PackageError::ParseError(format!("failed to read yarn.lock: {}", e)))?;
    let entries = parse_yarn_lock(&lock_content);

    let mut root_deps = Vec::new();
    let mut visited = HashSet::new();
    const MAX_DEPTH: usize = 50;

    // Read direct dependencies from package.json
    for dep_type in ["dependencies", "devDependencies"] {
        if let Some(deps) = pkg.get(dep_type).and_then(|d| d.as_object()) {
            for (dep_name, _version_req) in deps {
                root_deps.push(build_node(dep_name, &entries, &mut visited, MAX_DEPTH));
            }
        }
    }

    let root = TreeNode {
        name: name.to_string(),
        version: version.to_string(),
        dependencies: root_deps,
    };

    Ok(DependencyTree { roots: vec![root] })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_package_name_simple() {
        assert_eq!(
            extract_package_name("react@18.2.0"),
            Some("react".to_string())
        );
        assert_eq!(
            extract_package_name("lodash@^4.17.0"),
            Some("lodash".to_string())
        );
    }

    #[test]
    fn test_extract_package_name_scoped() {
        assert_eq!(
            extract_package_name("@types/node@20.0.0"),
            Some("@types/node".to_string())
        );
        assert_eq!(
            extract_package_name("@babel/core@^7.0.0"),
            Some("@babel/core".to_string())
        );
    }

    #[test]
    fn test_parse_yarn_lock_simple() {
        let content = r#"
# yarn lockfile v1

react@^18.0.0:
  version "18.2.0"
  resolved "https://registry.yarnpkg.com/react/-/react-18.2.0.tgz"
  integrity sha512-...
  dependencies:
    loose-envify "^1.1.0"

loose-envify@^1.1.0:
  version "1.4.0"
  resolved "https://registry.yarnpkg.com/loose-envify/-/loose-envify-1.4.0.tgz"
"#;

        let entries = parse_yarn_lock(content);

        assert!(entries.contains_key("react"));
        let react = entries.get("react").unwrap();
        assert_eq!(react.version, "18.2.0");
        assert_eq!(react.dependencies, vec!["loose-envify"]);

        assert!(entries.contains_key("loose-envify"));
        let loose = entries.get("loose-envify").unwrap();
        assert_eq!(loose.version, "1.4.0");
        assert!(loose.dependencies.is_empty());
    }

    #[test]
    fn test_parse_yarn_lock_scoped() {
        let content = r#"
"@types/node@^20.0.0":
  version "20.11.0"
  resolved "https://registry.yarnpkg.com/@types/node/-/node-20.11.0.tgz"
  dependencies:
    undici-types "~5.26.4"

"@babel/core@^7.24.0", "@babel/core@^7.0.0":
  version "7.24.5"
  resolved "https://registry.yarnpkg.com/@babel/core/-/core-7.24.5.tgz"
"#;

        let entries = parse_yarn_lock(content);

        assert!(entries.contains_key("@types/node"));
        let types_node = entries.get("@types/node").unwrap();
        assert_eq!(types_node.version, "20.11.0");
        assert_eq!(types_node.dependencies, vec!["undici-types"]);

        // Multiple version ranges should all map to same entry
        assert!(entries.contains_key("@babel/core"));
        let babel = entries.get("@babel/core").unwrap();
        assert_eq!(babel.version, "7.24.5");
    }

    #[test]
    fn test_parse_yarn_lock_multiple_deps() {
        let content = r#"
typescript@^5.0.0:
  version "5.4.5"
  resolved "https://registry.yarnpkg.com/typescript/-/typescript-5.4.5.tgz"
  dependencies:
    dep-a "^1.0.0"
    dep-b "^2.0.0"
    dep-c "^3.0.0"
"#;

        let entries = parse_yarn_lock(content);
        let ts = entries.get("typescript").unwrap();
        assert_eq!(ts.version, "5.4.5");
        assert_eq!(ts.dependencies.len(), 3);
        assert!(ts.dependencies.contains(&"dep-a".to_string()));
        assert!(ts.dependencies.contains(&"dep-b".to_string()));
        assert!(ts.dependencies.contains(&"dep-c".to_string()));
    }
}
