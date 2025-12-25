//! RubyGems ecosystem.

use crate::{Dependency, DependencyTree, Ecosystem, LockfileManager, PackageError, PackageInfo, PackageQuery, TreeNode};
use std::path::Path;
use std::process::Command;

pub struct Gem;

impl Ecosystem for Gem {
    fn name(&self) -> &'static str {
        "gem"
    }

    fn manifest_files(&self) -> &'static [&'static str] {
        &["Gemfile", "*.gemspec"]
    }

    fn lockfiles(&self) -> &'static [LockfileManager] {
        &[LockfileManager {
            filename: "Gemfile.lock",
            manager: "bundle",
        }]
    }

    fn tools(&self) -> &'static [&'static str] {
        &["curl"] // Uses rubygems.org API
    }

    fn fetch_info(&self, query: &PackageQuery, _tool: &str) -> Result<PackageInfo, PackageError> {
        fetch_rubygems_info(&query.name)
    }

    fn installed_version(&self, package: &str, project_root: &Path) -> Option<String> {
        // Gemfile.lock format:
        //   specs:
        //     rails (7.0.0)
        let lockfile = project_root.join("Gemfile.lock");
        let content = std::fs::read_to_string(lockfile).ok()?;

        for line in content.lines() {
            let trimmed = line.trim();
            // Format: "gem_name (version)"
            if let Some(rest) = trimmed.strip_prefix(package) {
                if rest.starts_with(' ') || rest.starts_with('(') {
                    if let Some(start) = rest.find('(') {
                        if let Some(end) = rest.find(')') {
                            return Some(rest[start + 1..end].to_string());
                        }
                    }
                }
            }
        }
        None
    }

    fn list_dependencies(&self, project_root: &Path) -> Result<Vec<Dependency>, PackageError> {
        // Parse Gemfile (Ruby DSL, but we can extract simple patterns)
        let gemfile = project_root.join("Gemfile");
        let content = std::fs::read_to_string(&gemfile)
            .map_err(|e| PackageError::ParseError(format!("failed to read Gemfile: {}", e)))?;

        let mut deps = Vec::new();

        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }

            // gem 'name' or gem 'name', 'version' or gem "name", "~> 1.0"
            if line.starts_with("gem ") {
                let rest = &line[4..];
                // Extract gem name from quotes
                let quote = rest.chars().next();
                if quote == Some('\'') || quote == Some('"') {
                    let q = quote.unwrap();
                    if let Some(end) = rest[1..].find(q) {
                        let name = rest[1..=end].to_string();
                        // Try to find version after the gem name
                        let after_name = &rest[end + 2..];
                        let version_req = if let Some(start) = after_name.find(q) {
                            let ver_str = &after_name[start + 1..];
                            ver_str.find(q).map(|end| ver_str[..end].to_string())
                        } else {
                            None
                        };
                        deps.push(Dependency {
                            name,
                            version_req,
                            optional: false,
                        });
                    }
                }
            }
        }

        Ok(deps)
    }

    fn dependency_tree(&self, project_root: &Path) -> Result<DependencyTree, PackageError> {
        // Parse Gemfile.lock for all gems
        let lockfile = project_root.join("Gemfile.lock");
        let content = std::fs::read_to_string(&lockfile)
            .map_err(|e| PackageError::ParseError(format!("failed to read Gemfile.lock: {}", e)))?;

        let mut deps = Vec::new();

        // Parse specs section: "    gem_name (version)"
        let mut in_specs = false;
        for line in content.lines() {
            if line.trim() == "specs:" {
                in_specs = true;
                continue;
            }
            if in_specs && !line.starts_with(' ') {
                in_specs = false;
            }
            if in_specs {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    // Parse "gem_name (version)" format
                    if let Some(paren_start) = trimmed.find('(') {
                        let name = trimmed[..paren_start].trim();
                        let version = trimmed[paren_start + 1..]
                            .trim_end_matches(')')
                            .to_string();
                        deps.push(TreeNode {
                            name: name.to_string(),
                            version,
                            dependencies: Vec::new(),
                        });
                    }
                }
            }
        }

        Ok(DependencyTree {
            roots: vec![TreeNode {
                name: "Gemfile.lock".to_string(),
                version: String::new(),
                dependencies: deps,
            }],
        })
    }
}

fn fetch_rubygems_info(package: &str) -> Result<PackageInfo, PackageError> {
    let url = format!("https://rubygems.org/api/v1/gems/{}.json", package);

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

    let description = v.get("info").and_then(|i| i.as_str()).map(String::from);

    let license = v
        .get("licenses")
        .and_then(|l| l.as_array())
        .and_then(|arr| arr.first())
        .and_then(|l| l.as_str())
        .map(String::from);

    let homepage = v
        .get("homepage_uri")
        .and_then(|u| u.as_str())
        .map(String::from);

    let repository = v
        .get("source_code_uri")
        .and_then(|u| u.as_str())
        .map(String::from);

    // Parse dependencies
    let mut dependencies = Vec::new();
    if let Some(deps) = v.get("dependencies") {
        // Runtime dependencies
        if let Some(runtime) = deps.get("runtime").and_then(|r| r.as_array()) {
            for dep in runtime {
                if let Some(dep_name) = dep.get("name").and_then(|n| n.as_str()) {
                    let version_req = dep
                        .get("requirements")
                        .and_then(|r| r.as_str())
                        .map(String::from);
                    dependencies.push(Dependency {
                        name: dep_name.to_string(),
                        version_req,
                        optional: false,
                    });
                }
            }
        }
        // Development dependencies (marked as optional)
        if let Some(dev) = deps.get("development").and_then(|d| d.as_array()) {
            for dep in dev {
                if let Some(dep_name) = dep.get("name").and_then(|n| n.as_str()) {
                    let version_req = dep
                        .get("requirements")
                        .and_then(|r| r.as_str())
                        .map(String::from);
                    dependencies.push(Dependency {
                        name: dep_name.to_string(),
                        version_req,
                        optional: true,
                    });
                }
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
