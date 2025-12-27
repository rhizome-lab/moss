//! Hex (Elixir/Erlang) ecosystem.

use crate::{
    AuditResult, Dependency, DependencyTree, Ecosystem, LockfileManager, PackageError, PackageInfo,
    PackageQuery, TreeNode,
};
use std::path::Path;

pub struct Hex;

impl Ecosystem for Hex {
    fn name(&self) -> &'static str {
        "hex"
    }

    fn manifest_files(&self) -> &'static [&'static str] {
        &["mix.exs"]
    }

    fn lockfiles(&self) -> &'static [LockfileManager] {
        &[LockfileManager {
            filename: "mix.lock",
            manager: "mix",
        }]
    }

    fn tools(&self) -> &'static [&'static str] {
        &["curl"] // Uses hex.pm API
    }

    fn fetch_info(&self, query: &PackageQuery, _tool: &str) -> Result<PackageInfo, PackageError> {
        fetch_hex_info(&query.name)
    }

    fn installed_version(&self, package: &str, project_root: &Path) -> Option<String> {
        // mix.lock is Elixir term format:
        //   "package_name": {:hex, :package_name, "1.2.3", ...}
        let lockfile = project_root.join("mix.lock");
        let content = std::fs::read_to_string(lockfile).ok()?;

        // Simple text extraction: look for "package": {:hex, :package, "version"
        let pattern = format!("\"{}\": {{:hex, :{}, \"", package, package);
        if let Some(start) = content.find(&pattern) {
            let after = &content[start + pattern.len()..];
            if let Some(end) = after.find('"') {
                return Some(after[..end].to_string());
            }
        }
        None
    }

    fn list_dependencies(&self, project_root: &Path) -> Result<Vec<Dependency>, PackageError> {
        // mix.exs is Elixir code, we can extract deps from defp deps do [...] end
        let mixfile = project_root.join("mix.exs");
        let content = std::fs::read_to_string(&mixfile)
            .map_err(|e| PackageError::ParseError(format!("failed to read mix.exs: {}", e)))?;

        let mut deps = Vec::new();

        // Simple pattern extraction: {:dep_name, "~> 1.0"} or {:dep_name, ">= 1.0"}
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("{:") {
                // Extract atom name after {:
                if let Some(end) = line[2..].find(|c: char| c == ',' || c == '}') {
                    let name = line[2..2 + end].to_string();
                    // Try to find version string
                    let rest = &line[2 + end..];
                    let version_req = if let Some(start) = rest.find('"') {
                        let ver_str = &rest[start + 1..];
                        ver_str.find('"').map(|end| ver_str[..end].to_string())
                    } else {
                        None
                    };
                    let optional = rest.contains("optional: true");
                    deps.push(Dependency {
                        name,
                        version_req,
                        optional,
                    });
                }
            }
        }

        Ok(deps)
    }

    fn dependency_tree(&self, project_root: &Path) -> Result<DependencyTree, PackageError> {
        // Parse mix.lock for all deps
        let lockfile = project_root.join("mix.lock");
        let content = std::fs::read_to_string(&lockfile)
            .map_err(|e| PackageError::ParseError(format!("failed to read mix.lock: {}", e)))?;

        let mut deps = Vec::new();

        // Format: "package": {:hex, :package, "version", ...}
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('"') && line.contains(":hex") {
                // Extract name and version
                if let Some(name_end) = line[1..].find('"') {
                    let name = &line[1..=name_end];
                    // Find version after :hex, :name,
                    let pattern = format!(":hex, :{}, \"", name);
                    if let Some(ver_start) = line.find(&pattern) {
                        let after = &line[ver_start + pattern.len()..];
                        if let Some(ver_end) = after.find('"') {
                            let version = &after[..ver_end];
                            deps.push(TreeNode {
                                name: name.to_string(),
                                version: version.to_string(),
                                dependencies: Vec::new(),
                            });
                        }
                    }
                }
            }
        }

        Ok(DependencyTree {
            roots: vec![TreeNode {
                name: "mix.lock".to_string(),
                version: String::new(),
                dependencies: deps,
            }],
        })
    }

    fn audit(&self, _project_root: &Path) -> Result<AuditResult, PackageError> {
        Err(PackageError::ToolFailed(
            "audit not yet supported for Hex. Use: mix deps.audit".to_string(),
        ))
    }
}

fn fetch_hex_info(package: &str) -> Result<PackageInfo, PackageError> {
    let url = format!("https://hex.pm/api/packages/{}", package);
    let body = crate::http::get(&url)?;
    let v: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| PackageError::ParseError(format!("invalid JSON: {}", e)))?;

    let name = v
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(package)
        .to_string();

    // Get latest version from releases array
    let version = v
        .get("releases")
        .and_then(|r| r.as_array())
        .and_then(|arr| arr.first())
        .and_then(|r| r.get("version"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| PackageError::ParseError("missing version".to_string()))?
        .to_string();

    let meta = v.get("meta");

    let description = meta
        .and_then(|m| m.get("description"))
        .and_then(|d| d.as_str())
        .map(String::from);

    let license = meta
        .and_then(|m| m.get("licenses"))
        .and_then(|l| l.as_array())
        .and_then(|arr| arr.first())
        .and_then(|l| l.as_str())
        .map(String::from);

    let homepage = meta
        .and_then(|m| m.get("links"))
        .and_then(|l| l.get("GitHub").or(l.get("Homepage")))
        .and_then(|u| u.as_str())
        .map(String::from);

    let repository = meta
        .and_then(|m| m.get("links"))
        .and_then(|l| l.get("GitHub"))
        .and_then(|u| u.as_str())
        .map(String::from);

    // Parse requirements from latest release
    let mut dependencies = Vec::new();
    if let Some(latest) = v
        .get("releases")
        .and_then(|r| r.as_array())
        .and_then(|a| a.first())
    {
        if let Some(reqs) = latest.get("requirements").and_then(|r| r.as_object()) {
            for (dep_name, req) in reqs {
                let version_req = req
                    .get("requirement")
                    .and_then(|r| r.as_str())
                    .map(String::from);
                let optional = req
                    .get("optional")
                    .and_then(|o| o.as_bool())
                    .unwrap_or(false);
                dependencies.push(Dependency {
                    name: dep_name.clone(),
                    version_req,
                    optional,
                });
            }
        }
    }

    Ok(PackageInfo {
        name,
        version,
        description,
        license,
        homepage,
        repository,
        features: Vec::new(),
        dependencies,
    })
}
