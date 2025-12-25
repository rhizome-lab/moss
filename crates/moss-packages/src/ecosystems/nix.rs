//! Nix ecosystem.

use crate::{Dependency, DependencyTree, Ecosystem, LockfileManager, PackageError, PackageInfo, PackageQuery, TreeNode};
use std::path::Path;
use std::process::Command;

pub struct Nix;

impl Ecosystem for Nix {
    fn name(&self) -> &'static str {
        "nix"
    }

    fn manifest_files(&self) -> &'static [&'static str] {
        &["flake.nix", "default.nix", "shell.nix"]
    }

    fn lockfiles(&self) -> &'static [LockfileManager] {
        &[LockfileManager {
            filename: "flake.lock",
            manager: "nix",
        }]
    }

    fn tools(&self) -> &'static [&'static str] {
        &["nix"]
    }

    fn fetch_info(&self, query: &PackageQuery, _tool: &str) -> Result<PackageInfo, PackageError> {
        fetch_nix_info(&query.name)
    }

    fn installed_version(&self, _package: &str, _project_root: &Path) -> Option<String> {
        // flake.lock contains input revisions, not package versions
        // Nix packages are pinned by nixpkgs revision, not individual versions
        None
    }

    fn list_dependencies(&self, project_root: &Path) -> Result<Vec<Dependency>, PackageError> {
        // Nix flake inputs from flake.nix
        let flake = project_root.join("flake.nix");
        if let Ok(content) = std::fs::read_to_string(&flake) {
            let mut deps = Vec::new();

            // Extract inputs: inputs.name.url = "github:..." or inputs = { name = {...} }
            // Simple approach: look for github: or nixpkgs patterns
            for line in content.lines() {
                let line = line.trim();
                if line.contains("github:") || line.contains("nixpkgs") {
                    // Extract input name from patterns like: name.url = "..." or name = { url = "..." }
                    if let Some(eq) = line.find('=') {
                        let before = line[..eq].trim();
                        let name = before.split('.').next().unwrap_or(before);
                        if !name.is_empty() && !name.starts_with('#') && name != "url" {
                            // Extract URL as version
                            if let Some(start) = line.find('"') {
                                let rest = &line[start + 1..];
                                if let Some(end) = rest.find('"') {
                                    deps.push(Dependency {
                                        name: name.to_string(),
                                        version_req: Some(rest[..end].to_string()),
                                        optional: false,
                                    });
                                }
                            }
                        }
                    }
                }
            }

            return Ok(deps);
        }

        Err(PackageError::ParseError("no flake.nix found".to_string()))
    }

    fn dependency_tree(&self, project_root: &Path) -> Result<DependencyTree, PackageError> {
        // Parse flake.lock for input revisions
        let lockfile = project_root.join("flake.lock");
        let content = std::fs::read_to_string(&lockfile)
            .map_err(|e| PackageError::ParseError(format!("failed to read flake.lock: {}", e)))?;
        let parsed: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| PackageError::ParseError(format!("invalid JSON: {}", e)))?;

        let mut deps = Vec::new();

        if let Some(nodes) = parsed.get("nodes").and_then(|n| n.as_object()) {
            for (name, node) in nodes {
                if name == "root" {
                    continue;
                }
                let rev = node
                    .get("locked")
                    .and_then(|l| l.get("rev"))
                    .and_then(|r| r.as_str())
                    .map(|r| r[..7.min(r.len())].to_string())
                    .unwrap_or_default();
                deps.push(TreeNode {
                    name: name.clone(),
                    version: rev,
                    dependencies: Vec::new(),
                });
            }
        }

        Ok(DependencyTree {
            roots: vec![TreeNode {
                name: "flake.lock".to_string(),
                version: String::new(),
                dependencies: deps,
            }],
        })
    }
}

fn fetch_nix_info(package: &str) -> Result<PackageInfo, PackageError> {
    // Try nix search first
    let output = Command::new("nix")
        .args(["search", "nixpkgs", package, "--json"])
        .output()
        .map_err(|e| PackageError::ToolFailed(format!("nix search failed: {}", e)))?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Ok(results) = serde_json::from_str::<serde_json::Value>(&stdout) {
            if let Some(obj) = results.as_object() {
                // Find exact match or first result
                let (attr, info) = obj
                    .iter()
                    .find(|(k, _)| k.ends_with(&format!(".{}", package)))
                    .or_else(|| obj.iter().next())
                    .ok_or_else(|| PackageError::NotFound(package.to_string()))?;

                let name = attr
                    .split('.')
                    .last()
                    .unwrap_or(package)
                    .to_string();

                let version = info
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                let description = info
                    .get("description")
                    .and_then(|d| d.as_str())
                    .map(String::from);

                return Ok(PackageInfo {
                    name,
                    version,
                    description,
                    license: None,
                    homepage: Some(format!("https://search.nixos.org/packages?query={}", package)),
                    repository: None,
                    features: Vec::new(),
                    dependencies: Vec::new(),
                });
            }
        }
    }

    // Fallback: try nix-env
    let output = Command::new("nix-env")
        .args(["-qaP", package])
        .output()
        .map_err(|e| PackageError::ToolFailed(format!("nix-env failed: {}", e)))?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Some(line) = stdout.lines().next() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                // Format: "nixpkgs.package  package-1.2.3"
                let full_name = parts[1];
                let (name, version) = if let Some(idx) = full_name.rfind('-') {
                    let potential_version = &full_name[idx + 1..];
                    if potential_version.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                        (full_name[..idx].to_string(), potential_version.to_string())
                    } else {
                        (full_name.to_string(), "unknown".to_string())
                    }
                } else {
                    (full_name.to_string(), "unknown".to_string())
                };

                return Ok(PackageInfo {
                    name,
                    version,
                    description: None,
                    license: None,
                    homepage: Some(format!("https://search.nixos.org/packages?query={}", package)),
                    repository: None,
                    features: Vec::new(),
                    dependencies: Vec::new(),
                });
            }
        }
    }

    Err(PackageError::NotFound(package.to_string()))
}
