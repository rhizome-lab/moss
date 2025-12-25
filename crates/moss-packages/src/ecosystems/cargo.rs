//! Cargo (Rust) ecosystem.

use crate::{Dependency, DependencyTree, Ecosystem, Feature, LockfileManager, PackageError, PackageInfo, PackageQuery, TreeNode};
use std::path::Path;
use std::process::Command;

pub struct Cargo;

impl Ecosystem for Cargo {
    fn name(&self) -> &'static str {
        "cargo"
    }

    fn manifest_files(&self) -> &'static [&'static str] {
        &["Cargo.toml"]
    }

    fn lockfiles(&self) -> &'static [LockfileManager] {
        &[LockfileManager {
            filename: "Cargo.lock",
            manager: "cargo",
        }]
    }

    fn tools(&self) -> &'static [&'static str] {
        &["curl"] // Uses crates.io API
    }

    fn fetch_info(&self, query: &PackageQuery, _tool: &str) -> Result<PackageInfo, PackageError> {
        fetch_crates_io_info(query)
    }

    fn installed_version(&self, package: &str, project_root: &Path) -> Option<String> {
        let lockfile = project_root.join("Cargo.lock");
        let content = std::fs::read_to_string(lockfile).ok()?;
        let parsed: toml::Value = toml::from_str(&content).ok()?;

        parsed
            .get("package")?
            .as_array()?
            .iter()
            .find(|pkg| pkg.get("name").and_then(|n| n.as_str()) == Some(package))
            .and_then(|pkg| pkg.get("version"))
            .and_then(|v| v.as_str())
            .map(String::from)
    }

    fn list_dependencies(&self, project_root: &Path) -> Result<Vec<Dependency>, PackageError> {
        let manifest = project_root.join("Cargo.toml");
        let content = std::fs::read_to_string(&manifest)
            .map_err(|e| PackageError::ParseError(format!("failed to read Cargo.toml: {}", e)))?;
        let parsed: toml::Value = toml::from_str(&content)
            .map_err(|e| PackageError::ParseError(format!("invalid TOML: {}", e)))?;

        let mut deps = Vec::new();

        // Parse [dependencies]
        if let Some(table) = parsed.get("dependencies").and_then(|d| d.as_table()) {
            for (name, value) in table {
                deps.push(parse_cargo_dep(name, value, false));
            }
        }

        // Parse [dev-dependencies]
        if let Some(table) = parsed.get("dev-dependencies").and_then(|d| d.as_table()) {
            for (name, value) in table {
                deps.push(parse_cargo_dep(name, value, false));
            }
        }

        // Parse [build-dependencies]
        if let Some(table) = parsed.get("build-dependencies").and_then(|d| d.as_table()) {
            for (name, value) in table {
                deps.push(parse_cargo_dep(name, value, false));
            }
        }

        Ok(deps)
    }

    fn dependency_tree(&self, project_root: &Path) -> Result<DependencyTree, PackageError> {
        // Find Cargo.lock - may be in parent if this is a workspace member
        let (lockfile, workspace_root) = find_cargo_lock(project_root)?;
        let content = std::fs::read_to_string(&lockfile)
            .map_err(|e| PackageError::ParseError(format!("failed to read Cargo.lock: {}", e)))?;
        let parsed: toml::Value = toml::from_str(&content)
            .map_err(|e| PackageError::ParseError(format!("invalid TOML: {}", e)))?;

        // Get root package name(s) from Cargo.toml
        let manifest = project_root.join("Cargo.toml");
        let manifest_content = std::fs::read_to_string(&manifest)
            .map_err(|e| PackageError::ParseError(format!("failed to read Cargo.toml: {}", e)))?;
        let manifest_parsed: toml::Value = toml::from_str(&manifest_content)
            .map_err(|e| PackageError::ParseError(format!("invalid TOML: {}", e)))?;

        // Determine root package(s) to show
        let root_names: Vec<String> = if let Some(pkg) = manifest_parsed.get("package") {
            pkg.get("name")
                .and_then(|n| n.as_str())
                .map(|s| vec![s.to_string()])
                .unwrap_or_default()
        } else if let Some(workspace) = manifest_parsed.get("workspace") {
            workspace
                .get("members")
                .and_then(|m| m.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|m| m.as_str())
                        .filter_map(|member_path| {
                            let member_manifest = workspace_root.join(member_path).join("Cargo.toml");
                            std::fs::read_to_string(&member_manifest).ok().and_then(|c| {
                                toml::from_str::<toml::Value>(&c).ok().and_then(|v| {
                                    v.get("package")
                                        .and_then(|p| p.get("name"))
                                        .and_then(|n| n.as_str())
                                        .map(String::from)
                                })
                            })
                        })
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        // Build package map: name -> (version, dependencies)
        let mut packages: std::collections::HashMap<String, (String, Vec<String>)> = std::collections::HashMap::new();
        if let Some(pkgs) = parsed.get("package").and_then(|p| p.as_array()) {
            for pkg in pkgs {
                let name = pkg.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let version = pkg.get("version").and_then(|v| v.as_str()).unwrap_or("");
                let deps: Vec<String> = pkg
                    .get("dependencies")
                    .and_then(|d| d.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|d| d.as_str())
                            .map(|s| s.split_whitespace().next().unwrap_or(s).to_string())
                            .collect()
                    })
                    .unwrap_or_default();
                packages.insert(name.to_string(), (version.to_string(), deps));
            }
        }

        // Build tree structure
        let mut visited = std::collections::HashSet::new();

        fn build_node(
            name: &str,
            packages: &std::collections::HashMap<String, (String, Vec<String>)>,
            visited: &mut std::collections::HashSet<String>,
        ) -> TreeNode {
            let (version, deps) = packages
                .get(name)
                .map(|(v, d)| (v.clone(), d.clone()))
                .unwrap_or_default();

            let children = if visited.contains(name) {
                Vec::new() // Already visited, don't recurse
            } else {
                visited.insert(name.to_string());
                deps.iter()
                    .map(|dep| build_node(dep, packages, visited))
                    .collect()
            };

            TreeNode {
                name: name.to_string(),
                version,
                dependencies: children,
            }
        }

        let roots: Vec<TreeNode> = root_names
            .iter()
            .map(|name| build_node(name, &packages, &mut visited))
            .collect();

        Ok(DependencyTree { roots })
    }
}

/// Find Cargo.lock, searching up from project_root to find workspace root
fn find_cargo_lock(project_root: &Path) -> Result<(std::path::PathBuf, std::path::PathBuf), PackageError> {
    let mut current = project_root.to_path_buf();
    loop {
        let lockfile = current.join("Cargo.lock");
        if lockfile.exists() {
            return Ok((lockfile, current));
        }
        if !current.pop() {
            break;
        }
    }
    Err(PackageError::ParseError(format!(
        "Cargo.lock not found in {} or parent directories",
        project_root.display()
    )))
}

fn parse_cargo_dep(name: &str, value: &toml::Value, optional: bool) -> Dependency {
    match value {
        toml::Value::String(version) => Dependency {
            name: name.to_string(),
            version_req: Some(version.clone()),
            optional,
        },
        toml::Value::Table(table) => {
            let version = table
                .get("version")
                .and_then(|v| v.as_str())
                .map(String::from);
            let opt = table
                .get("optional")
                .and_then(|o| o.as_bool())
                .unwrap_or(optional);
            Dependency {
                name: name.to_string(),
                version_req: version,
                optional: opt,
            }
        }
        _ => Dependency {
            name: name.to_string(),
            version_req: None,
            optional,
        },
    }
}

fn fetch_crates_io_info(query: &PackageQuery) -> Result<PackageInfo, PackageError> {
    let package = &query.name;

    // If version specified, fetch that version directly
    // Otherwise, get crate metadata first to find latest version
    let version = if let Some(v) = &query.version {
        v.clone()
    } else {
        // Get latest version
        let url = format!("https://crates.io/api/v1/crates/{}", package);
        let output = Command::new("curl")
            .args(["-sS", "-f", "-H", "User-Agent: moss-packages", &url])
            .output()
            .map_err(|e| PackageError::ToolFailed(format!("curl failed: {}", e)))?;

        if !output.status.success() {
            return Err(PackageError::NotFound(package.to_string()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let v: serde_json::Value = serde_json::from_str(&stdout)
            .map_err(|e| PackageError::ParseError(format!("invalid JSON: {}", e)))?;

        v.get("crate")
            .and_then(|c| c.get("max_version"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| PackageError::ParseError("missing max_version".to_string()))?
            .to_string()
    };

    // Get version-specific info
    let version_url = format!("https://crates.io/api/v1/crates/{}/{}", package, version);
    let output = Command::new("curl")
        .args(["-sS", "-f", "-H", "User-Agent: moss-packages", &version_url])
        .output()
        .map_err(|e| PackageError::ToolFailed(format!("curl failed: {}", e)))?;

    if !output.status.success() {
        return Err(PackageError::NotFound(format!("{}@{}", package, version)));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout)
        .map_err(|e| PackageError::ParseError(format!("invalid JSON: {}", e)))?;

    let ver = v
        .get("version")
        .ok_or_else(|| PackageError::ParseError("missing version".to_string()))?;

    let name = ver
        .get("crate")
        .and_then(|c| c.as_str())
        .unwrap_or(package)
        .to_string();

    let version = ver
        .get("num")
        .and_then(|n| n.as_str())
        .unwrap_or(&version)
        .to_string();

    let license = ver.get("license").and_then(|l| l.as_str()).map(String::from);

    let features = ver
        .get("features")
        .and_then(|f| f.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(name, deps)| Feature {
                    name: name.clone(),
                    description: None,
                    dependencies: deps
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|d| d.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();

    // Get crate-level info (description, homepage, repository)
    let crate_url = format!("https://crates.io/api/v1/crates/{}", package);
    let crate_output = Command::new("curl")
        .args(["-sS", "-f", "-H", "User-Agent: moss-packages", &crate_url])
        .output()
        .ok();

    let (description, homepage, repository) = if let Some(out) = crate_output {
        if out.status.success() {
            let crate_stdout = String::from_utf8_lossy(&out.stdout);
            if let Ok(cv) = serde_json::from_str::<serde_json::Value>(&crate_stdout) {
                let crate_info = cv.get("crate");
                (
                    crate_info
                        .and_then(|c| c.get("description"))
                        .and_then(|d| d.as_str())
                        .map(String::from),
                    crate_info
                        .and_then(|c| c.get("homepage"))
                        .and_then(|h| h.as_str())
                        .filter(|s| !s.is_empty())
                        .map(String::from),
                    crate_info
                        .and_then(|c| c.get("repository"))
                        .and_then(|r| r.as_str())
                        .filter(|s| !s.is_empty())
                        .map(String::from),
                )
            } else {
                (None, None, None)
            }
        } else {
            (None, None, None)
        }
    } else {
        (None, None, None)
    };

    Ok(PackageInfo {
        name,
        version,
        description,
        license,
        homepage,
        repository,
        features,
        dependencies: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cargo_ecosystem() {
        let eco = Cargo;
        assert_eq!(eco.name(), "cargo");
        assert_eq!(eco.manifest_files(), &["Cargo.toml"]);
    }
}
