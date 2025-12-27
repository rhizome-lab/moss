//! Composer (PHP) ecosystem.

use crate::{
    AuditResult, Dependency, DependencyTree, Ecosystem, LockfileManager, PackageError, PackageInfo,
    PackageQuery, TreeNode,
};
use std::path::Path;

pub struct Composer;

impl Ecosystem for Composer {
    fn name(&self) -> &'static str {
        "composer"
    }

    fn manifest_files(&self) -> &'static [&'static str] {
        &["composer.json"]
    }

    fn lockfiles(&self) -> &'static [LockfileManager] {
        &[LockfileManager {
            filename: "composer.lock",
            manager: "composer",
        }]
    }

    fn tools(&self) -> &'static [&'static str] {
        &["curl"] // Uses packagist.org API
    }

    fn fetch_info(&self, query: &PackageQuery, _tool: &str) -> Result<PackageInfo, PackageError> {
        fetch_packagist_info(&query.name)
    }

    fn installed_version(&self, package: &str, project_root: &Path) -> Option<String> {
        let lockfile = project_root.join("composer.lock");
        let content = std::fs::read_to_string(lockfile).ok()?;
        let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;

        // Check both packages and packages-dev
        for key in ["packages", "packages-dev"] {
            if let Some(pkgs) = parsed.get(key).and_then(|p| p.as_array()) {
                for pkg in pkgs {
                    if pkg.get("name").and_then(|n| n.as_str()) == Some(package) {
                        return pkg
                            .get("version")
                            .and_then(|v| v.as_str())
                            .map(String::from);
                    }
                }
            }
        }
        None
    }

    fn list_dependencies(&self, project_root: &Path) -> Result<Vec<Dependency>, PackageError> {
        let manifest = project_root.join("composer.json");
        let content = std::fs::read_to_string(&manifest).map_err(|e| {
            PackageError::ParseError(format!("failed to read composer.json: {}", e))
        })?;
        let parsed: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| PackageError::ParseError(format!("invalid JSON: {}", e)))?;

        let mut deps = Vec::new();

        if let Some(require) = parsed.get("require").and_then(|r| r.as_object()) {
            for (name, version) in require {
                if name == "php" || name.starts_with("ext-") {
                    continue;
                }
                deps.push(Dependency {
                    name: name.clone(),
                    version_req: version.as_str().map(String::from),
                    optional: false,
                });
            }
        }

        if let Some(require_dev) = parsed.get("require-dev").and_then(|r| r.as_object()) {
            for (name, version) in require_dev {
                deps.push(Dependency {
                    name: name.clone(),
                    version_req: version.as_str().map(String::from),
                    optional: false,
                });
            }
        }

        Ok(deps)
    }

    fn dependency_tree(&self, project_root: &Path) -> Result<DependencyTree, PackageError> {
        // Parse composer.lock for full dependency list
        let lockfile = project_root.join("composer.lock");
        let content = std::fs::read_to_string(&lockfile).map_err(|e| {
            PackageError::ParseError(format!("failed to read composer.lock: {}", e))
        })?;
        let parsed: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| PackageError::ParseError(format!("invalid JSON: {}", e)))?;

        let mut deps = Vec::new();

        for key in ["packages", "packages-dev"] {
            if let Some(pkgs) = parsed.get(key).and_then(|p| p.as_array()) {
                for pkg in pkgs {
                    let name = pkg.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    let version = pkg.get("version").and_then(|v| v.as_str()).unwrap_or("");
                    deps.push(TreeNode {
                        name: name.to_string(),
                        version: version.to_string(),
                        dependencies: Vec::new(),
                    });
                }
            }
        }

        Ok(DependencyTree {
            roots: vec![TreeNode {
                name: "composer.lock".to_string(),
                version: String::new(),
                dependencies: deps,
            }],
        })
    }

    fn audit(&self, _project_root: &Path) -> Result<AuditResult, PackageError> {
        // Composer has 'composer audit' in newer versions but JSON output varies
        Err(PackageError::ToolFailed(
            "audit not yet supported for Composer. Use: composer audit".to_string(),
        ))
    }
}

fn fetch_packagist_info(package: &str) -> Result<PackageInfo, PackageError> {
    let url = format!("https://packagist.org/packages/{}.json", package);
    let body = crate::http::get(&url)?;
    let v: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| PackageError::ParseError(format!("invalid JSON: {}", e)))?;

    let pkg = v
        .get("package")
        .ok_or_else(|| PackageError::ParseError("missing package field".to_string()))?;

    let name = pkg
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or(package)
        .to_string();

    let description = pkg
        .get("description")
        .and_then(|d| d.as_str())
        .map(String::from);

    let repository = pkg
        .get("repository")
        .and_then(|r| r.as_str())
        .map(String::from);

    // Get latest version from versions object
    let versions = pkg.get("versions").and_then(|v| v.as_object());

    let (version, license, dependencies) = if let Some(vers) = versions {
        // Find latest non-dev version
        let latest = vers
            .iter()
            .filter(|(k, _)| !k.contains("dev") && !k.starts_with("v"))
            .max_by(|(a, _), (b, _)| {
                // Simple version comparison
                a.cmp(b)
            })
            .or_else(|| vers.iter().next());

        if let Some((ver, data)) = latest {
            let lic = data
                .get("license")
                .and_then(|l| l.as_array())
                .and_then(|arr| arr.first())
                .and_then(|l| l.as_str())
                .map(String::from);

            let mut deps = Vec::new();
            if let Some(require) = data.get("require").and_then(|r| r.as_object()) {
                for (dep_name, ver_req) in require {
                    // Skip PHP version requirements
                    if dep_name == "php" || dep_name.starts_with("ext-") {
                        continue;
                    }
                    deps.push(Dependency {
                        name: dep_name.clone(),
                        version_req: ver_req.as_str().map(String::from),
                        optional: false,
                    });
                }
            }
            if let Some(require_dev) = data.get("require-dev").and_then(|r| r.as_object()) {
                for (dep_name, ver_req) in require_dev {
                    deps.push(Dependency {
                        name: dep_name.clone(),
                        version_req: ver_req.as_str().map(String::from),
                        optional: true,
                    });
                }
            }

            (ver.clone(), lic, deps)
        } else {
            (String::new(), None, Vec::new())
        }
    } else {
        (String::new(), None, Vec::new())
    };

    if version.is_empty() {
        return Err(PackageError::ParseError("no versions found".to_string()));
    }

    Ok(PackageInfo {
        name,
        version,
        description,
        license,
        homepage: repository.clone(), // Packagist doesn't have separate homepage
        repository,
        features: Vec::new(),
        dependencies,
    })
}
