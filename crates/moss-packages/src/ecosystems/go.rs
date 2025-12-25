//! Go modules ecosystem.

use crate::{Dependency, DependencyTree, Ecosystem, LockfileManager, PackageError, PackageInfo, PackageQuery, TreeNode};
use std::path::Path;
use std::process::Command;

pub struct Go;

impl Ecosystem for Go {
    fn name(&self) -> &'static str {
        "go"
    }

    fn manifest_files(&self) -> &'static [&'static str] {
        &["go.mod"]
    }

    fn lockfiles(&self) -> &'static [LockfileManager] {
        &[LockfileManager {
            filename: "go.sum",
            manager: "go",
        }]
    }

    fn tools(&self) -> &'static [&'static str] {
        &["curl"] // Uses Go module proxy API
    }

    fn fetch_info(&self, query: &PackageQuery, _tool: &str) -> Result<PackageInfo, PackageError> {
        fetch_go_proxy_info(&query.name)
    }

    fn installed_version(&self, package: &str, project_root: &Path) -> Option<String> {
        // Parse go.mod for require statements
        let go_mod = project_root.join("go.mod");
        let content = std::fs::read_to_string(go_mod).ok()?;

        for line in content.lines() {
            let line = line.trim();
            // Format: module/path v1.2.3
            if line.starts_with(package) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 && parts[0] == package {
                    return Some(parts[1].to_string());
                }
            }
        }
        None
    }

    fn list_dependencies(&self, project_root: &Path) -> Result<Vec<Dependency>, PackageError> {
        let go_mod = project_root.join("go.mod");
        let content = std::fs::read_to_string(&go_mod)
            .map_err(|e| PackageError::ParseError(format!("failed to read go.mod: {}", e)))?;

        let mut deps = Vec::new();
        let mut in_require_block = false;

        for line in content.lines() {
            let line = line.trim();

            if line == "require (" {
                in_require_block = true;
                continue;
            }
            if line == ")" {
                in_require_block = false;
                continue;
            }

            // Single-line require: require module/path v1.2.3
            if line.starts_with("require ") {
                let rest = line.strip_prefix("require ").unwrap().trim();
                if let Some((module, version)) = rest.split_once(' ') {
                    deps.push(Dependency {
                        name: module.to_string(),
                        version_req: Some(version.to_string()),
                        optional: false,
                    });
                }
            } else if in_require_block && !line.is_empty() && !line.starts_with("//") {
                // Inside require block: module/path v1.2.3
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let indirect = parts.len() > 2 && parts[2..].contains(&"//") && line.contains("indirect");
                    deps.push(Dependency {
                        name: parts[0].to_string(),
                        version_req: Some(parts[1].to_string()),
                        optional: indirect,
                    });
                }
            }
        }

        Ok(deps)
    }

    fn dependency_tree(&self, project_root: &Path) -> Result<DependencyTree, PackageError> {
        // go.sum has all dependencies but not the tree structure
        // go.mod has direct deps, go.sum has transitive
        let go_sum = project_root.join("go.sum");
        let content = std::fs::read_to_string(&go_sum)
            .map_err(|e| PackageError::ParseError(format!("failed to read go.sum: {}", e)))?;

        // Get module name from go.mod
        let go_mod = project_root.join("go.mod");
        let root_name = std::fs::read_to_string(&go_mod)
            .ok()
            .and_then(|mod_content| {
                mod_content
                    .lines()
                    .find(|l| l.starts_with("module "))
                    .map(|line| line.strip_prefix("module ").unwrap_or("root").trim().to_string())
            })
            .unwrap_or_else(|| "root".to_string());

        // Parse go.sum: each line is "module version hash"
        let mut deps = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for line in content.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let module = parts[0];
                let version = parts[1].trim_end_matches("/go.mod");
                if seen.insert(format!("{}@{}", module, version)) {
                    deps.push(TreeNode {
                        name: module.to_string(),
                        version: version.to_string(),
                        dependencies: Vec::new(),
                    });
                }
            }
        }

        Ok(DependencyTree {
            roots: vec![TreeNode {
                name: root_name,
                version: String::new(),
                dependencies: deps,
            }],
        })
    }
}

fn fetch_go_proxy_info(package: &str) -> Result<PackageInfo, PackageError> {
    // Get latest version from proxy
    let url = format!("https://proxy.golang.org/{}/@latest", package);

    let output = Command::new("curl")
        .args(["-sS", "-f", &url])
        .output()
        .map_err(|e| PackageError::ToolFailed(format!("curl failed: {}", e)))?;

    if !output.status.success() {
        return Err(PackageError::NotFound(package.to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout)
        .map_err(|e| PackageError::ParseError(format!("invalid JSON: {}", e)))?;

    let version = v
        .get("Version")
        .and_then(|v| v.as_str())
        .ok_or_else(|| PackageError::ParseError("missing Version".to_string()))?
        .to_string();

    // Try to get more info from pkg.go.dev (optional, may fail)
    let repository = if package.starts_with("github.com/") {
        Some(format!("https://{}", package))
    } else if package.starts_with("golang.org/x/") {
        Some(format!(
            "https://go.googlesource.com/{}",
            package.strip_prefix("golang.org/x/").unwrap()
        ))
    } else {
        None
    };

    Ok(PackageInfo {
        name: package.to_string(),
        version,
        description: None, // Go proxy doesn't provide description
        license: None,
        homepage: Some(format!("https://pkg.go.dev/{}", package)),
        repository,
        features: Vec::new(),
        dependencies: Vec::new(), // Would need to parse go.mod
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_go_ecosystem() {
        let eco = Go;
        assert_eq!(eco.name(), "go");
        assert_eq!(eco.manifest_files(), &["go.mod"]);
    }
}
