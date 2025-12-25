//! Conan (C++) ecosystem.

use crate::{Dependency, DependencyTree, Ecosystem, LockfileManager, PackageError, PackageInfo, PackageQuery, TreeNode};
use std::path::Path;
use std::process::Command;

pub struct Conan;

impl Ecosystem for Conan {
    fn name(&self) -> &'static str {
        "conan"
    }

    fn manifest_files(&self) -> &'static [&'static str] {
        &["conanfile.txt", "conanfile.py"]
    }

    fn lockfiles(&self) -> &'static [LockfileManager] {
        &[LockfileManager {
            filename: "conan.lock",
            manager: "conan",
        }]
    }

    fn tools(&self) -> &'static [&'static str] {
        &["curl"] // Uses ConanCenter GitHub API
    }

    fn fetch_info(&self, query: &PackageQuery, _tool: &str) -> Result<PackageInfo, PackageError> {
        fetch_conancenter_api(&query.name)
    }

    fn installed_version(&self, package: &str, project_root: &Path) -> Option<String> {
        // conan.lock (JSON) format:
        // {"graph_lock": {"nodes": {"1": {"ref": "pkg/1.0.0", ...}}}}
        let lockfile = project_root.join("conan.lock");
        let content = std::fs::read_to_string(lockfile).ok()?;
        let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;

        if let Some(nodes) = parsed.get("graph_lock")?.get("nodes")?.as_object() {
            for (_, node) in nodes {
                if let Some(ref_str) = node.get("ref").and_then(|r| r.as_str()) {
                    // Format: "pkg/version" or "pkg/version@user/channel"
                    if let Some(rest) = ref_str.strip_prefix(&format!("{}/", package)) {
                        let version = rest.split('@').next()?;
                        return Some(version.to_string());
                    }
                }
            }
        }
        None
    }

    fn list_dependencies(&self, project_root: &Path) -> Result<Vec<Dependency>, PackageError> {
        // Parse conanfile.txt
        let conanfile_txt = project_root.join("conanfile.txt");
        if let Ok(content) = std::fs::read_to_string(&conanfile_txt) {
            let mut deps = Vec::new();
            let mut in_requires = false;

            for line in content.lines() {
                let line = line.trim();

                if line == "[requires]" {
                    in_requires = true;
                    continue;
                }
                if line.starts_with('[') {
                    in_requires = false;
                    continue;
                }

                if in_requires && !line.is_empty() {
                    // Format: pkg/version or pkg/version@user/channel
                    if let Some((name, rest)) = line.split_once('/') {
                        let version = rest.split('@').next().unwrap_or(rest);
                        deps.push(Dependency {
                            name: name.to_string(),
                            version_req: Some(version.to_string()),
                            optional: false,
                        });
                    }
                }
            }

            return Ok(deps);
        }

        // Try conanfile.py (Python, harder to parse)
        let conanfile_py = project_root.join("conanfile.py");
        if let Ok(content) = std::fs::read_to_string(&conanfile_py) {
            let mut deps = Vec::new();

            // Look for requires = ["pkg/version", ...]
            for line in content.lines() {
                if line.contains("requires") || line.contains("self.requires(") {
                    // Extract quoted strings
                    let mut pos = 0;
                    while let Some(start) = line[pos..].find('"') {
                        let after = &line[pos + start + 1..];
                        if let Some(end) = after.find('"') {
                            let req = &after[..end];
                            if let Some((name, rest)) = req.split_once('/') {
                                let version = rest.split('@').next().unwrap_or(rest);
                                deps.push(Dependency {
                                    name: name.to_string(),
                                    version_req: Some(version.to_string()),
                                    optional: false,
                                });
                            }
                            pos = pos + start + 2 + end;
                        } else {
                            break;
                        }
                    }
                }
            }

            return Ok(deps);
        }

        Err(PackageError::ParseError("no conanfile found".to_string()))
    }

    fn dependency_tree(&self, project_root: &Path) -> Result<DependencyTree, PackageError> {
        // Parse conan.lock
        let lockfile = project_root.join("conan.lock");
        let content = std::fs::read_to_string(&lockfile)
            .map_err(|e| PackageError::ParseError(format!("failed to read conan.lock: {}", e)))?;
        let parsed: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| PackageError::ParseError(format!("invalid JSON: {}", e)))?;

        let mut deps = Vec::new();

        if let Some(nodes) = parsed.get("graph_lock").and_then(|g| g.get("nodes")).and_then(|n| n.as_object()) {
            for (_, node) in nodes {
                if let Some(ref_str) = node.get("ref").and_then(|r| r.as_str()) {
                    // Format: "pkg/version" or "pkg/version@user/channel"
                    let (name, version) = if let Some((n, rest)) = ref_str.split_once('/') {
                        let v = rest.split('@').next().unwrap_or(rest);
                        (n.to_string(), v.to_string())
                    } else {
                        (ref_str.to_string(), String::new())
                    };
                    deps.push(TreeNode {
                        name,
                        version,
                        dependencies: Vec::new(),
                    });
                }
            }
        }

        Ok(DependencyTree {
            roots: vec![TreeNode {
                name: "conan.lock".to_string(),
                version: String::new(),
                dependencies: deps,
            }],
        })
    }
}

fn fetch_conancenter_api(package: &str) -> Result<PackageInfo, PackageError> {
    // ConanCenter Web API
    let url = format!(
        "https://raw.githubusercontent.com/conan-io/conan-center-index/master/recipes/{}/config.yml",
        package
    );

    let output = Command::new("curl")
        .args(["-sS", "-f", &url])
        .output()
        .map_err(|e| PackageError::ToolFailed(format!("curl failed: {}", e)))?;

    if !output.status.success() {
        return Err(PackageError::NotFound(package.to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse YAML config - extract versions (format: "1.2.3":)
    let version = stdout
        .lines()
        .find(|line| {
            let t = line.trim().trim_start_matches('"');
            t.chars().next().is_some_and(|c| c.is_ascii_digit())
        })
        .and_then(|line| {
            let trimmed = line.trim().trim_matches(|c| c == '"' || c == ':' || c == ' ');
            if trimmed.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                Some(trimmed.to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "latest".to_string());

    Ok(PackageInfo {
        name: package.to_string(),
        version,
        description: None,
        license: None,
        homepage: Some(format!("https://conan.io/center/recipes/{}", package)),
        repository: Some(format!(
            "https://github.com/conan-io/conan-center-index/tree/master/recipes/{}",
            package
        )),
        features: Vec::new(),
        dependencies: Vec::new(),
    })
}
