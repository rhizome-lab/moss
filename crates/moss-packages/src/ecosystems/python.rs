//! Python (pip/uv/poetry) ecosystem.

use crate::{
    AuditResult, Dependency, DependencyTree, Ecosystem, Feature, LockfileManager, PackageError,
    PackageInfo, PackageQuery, TreeNode, Vulnerability, VulnerabilitySeverity,
};
use std::path::Path;
use std::process::Command;

pub struct Python;

impl Ecosystem for Python {
    fn name(&self) -> &'static str {
        "python"
    }

    fn manifest_files(&self) -> &'static [&'static str] {
        &["pyproject.toml", "setup.py", "requirements.txt"]
    }

    fn lockfiles(&self) -> &'static [LockfileManager] {
        &[
            LockfileManager {
                filename: "uv.lock",
                manager: "uv",
            },
            LockfileManager {
                filename: "poetry.lock",
                manager: "poetry",
            },
            LockfileManager {
                filename: "Pipfile.lock",
                manager: "pipenv",
            },
            LockfileManager {
                filename: "pdm.lock",
                manager: "pdm",
            },
        ]
    }

    fn tools(&self) -> &'static [&'static str] {
        &["curl"] // Uses PyPI API
    }

    fn fetch_info(&self, query: &PackageQuery, _tool: &str) -> Result<PackageInfo, PackageError> {
        fetch_pypi_info(query)
    }

    fn installed_version(&self, package: &str, project_root: &Path) -> Option<String> {
        // Normalize package name (PEP 503: lowercase, replace - and . with _)
        let normalized = package.to_lowercase().replace(['-', '.'], "_");

        // Try uv.lock (TOML format)
        let uv_lock = project_root.join("uv.lock");
        if let Ok(content) = std::fs::read_to_string(&uv_lock) {
            if let Ok(parsed) = toml::from_str::<toml::Value>(&content) {
                if let Some(packages) = parsed.get("package").and_then(|p| p.as_array()) {
                    for pkg in packages {
                        let name = pkg.get("name").and_then(|n| n.as_str()).unwrap_or("");
                        let name_normalized = name.to_lowercase().replace(['-', '.'], "_");
                        if name_normalized == normalized {
                            if let Some(v) = pkg.get("version").and_then(|v| v.as_str()) {
                                return Some(v.to_string());
                            }
                        }
                    }
                }
            }
        }

        // Try poetry.lock (TOML format)
        let poetry_lock = project_root.join("poetry.lock");
        if let Ok(content) = std::fs::read_to_string(&poetry_lock) {
            if let Ok(parsed) = toml::from_str::<toml::Value>(&content) {
                if let Some(packages) = parsed.get("package").and_then(|p| p.as_array()) {
                    for pkg in packages {
                        let name = pkg.get("name").and_then(|n| n.as_str()).unwrap_or("");
                        let name_normalized = name.to_lowercase().replace(['-', '.'], "_");
                        if name_normalized == normalized {
                            if let Some(v) = pkg.get("version").and_then(|v| v.as_str()) {
                                return Some(v.to_string());
                            }
                        }
                    }
                }
            }
        }

        // Try Pipfile.lock (JSON format)
        let pipfile_lock = project_root.join("Pipfile.lock");
        if let Ok(content) = std::fs::read_to_string(pipfile_lock) {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                // Check default and develop sections
                for section in ["default", "develop"] {
                    if let Some(deps) = parsed.get(section).and_then(|s| s.as_object()) {
                        for (name, info) in deps {
                            let name_normalized = name.to_lowercase().replace(['-', '.'], "_");
                            if name_normalized == normalized {
                                if let Some(v) = info.get("version").and_then(|v| v.as_str()) {
                                    // Strip "==" prefix
                                    return Some(v.strip_prefix("==").unwrap_or(v).to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        None
    }

    fn list_dependencies(
        &self,
        project_root: &Path,
    ) -> Result<Vec<crate::Dependency>, PackageError> {
        // Try pyproject.toml first
        let pyproject = project_root.join("pyproject.toml");
        if let Ok(content) = std::fs::read_to_string(&pyproject) {
            if let Ok(parsed) = toml::from_str::<toml::Value>(&content) {
                let mut deps = Vec::new();

                // PEP 621: [project.dependencies]
                if let Some(project) = parsed.get("project") {
                    if let Some(dependencies) =
                        project.get("dependencies").and_then(|d| d.as_array())
                    {
                        for dep in dependencies {
                            if let Some(req) = dep.as_str().and_then(|s| parse_requirement(s)) {
                                deps.push(req);
                            }
                        }
                    }
                    // Optional dependencies
                    if let Some(optional) = project
                        .get("optional-dependencies")
                        .and_then(|o| o.as_table())
                    {
                        for (_group, group_deps) in optional {
                            if let Some(arr) = group_deps.as_array() {
                                for dep in arr {
                                    if let Some(mut req) =
                                        dep.as_str().and_then(|s| parse_requirement(s))
                                    {
                                        req.optional = true;
                                        deps.push(req);
                                    }
                                }
                            }
                        }
                    }
                }

                // Poetry: [tool.poetry.dependencies]
                if let Some(poetry) = parsed.get("tool").and_then(|t| t.get("poetry")) {
                    if let Some(dependencies) =
                        poetry.get("dependencies").and_then(|d| d.as_table())
                    {
                        for (name, value) in dependencies {
                            if name == "python" {
                                continue;
                            }
                            let version_req = match value {
                                toml::Value::String(v) => Some(v.clone()),
                                toml::Value::Table(t) => {
                                    t.get("version").and_then(|v| v.as_str()).map(String::from)
                                }
                                _ => None,
                            };
                            deps.push(crate::Dependency {
                                name: name.clone(),
                                version_req,
                                optional: false,
                            });
                        }
                    }
                }

                if !deps.is_empty() {
                    return Ok(deps);
                }
            }
        }

        // Fallback: requirements.txt
        let requirements = project_root.join("requirements.txt");
        if let Ok(content) = std::fs::read_to_string(requirements) {
            let deps: Vec<_> = content
                .lines()
                .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
                .filter_map(parse_requirement)
                .collect();
            return Ok(deps);
        }

        Err(PackageError::ParseError("no manifest found".to_string()))
    }

    fn dependency_tree(&self, project_root: &Path) -> Result<DependencyTree, PackageError> {
        // Try uv.lock first (TOML with package entries and dependencies)
        let uv_lock = project_root.join("uv.lock");
        if let Ok(content) = std::fs::read_to_string(&uv_lock) {
            if let Ok(parsed) = toml::from_str::<toml::Value>(&content) {
                return build_python_tree(&parsed, project_root);
            }
        }

        // Try poetry.lock
        let poetry_lock = project_root.join("poetry.lock");
        if let Ok(content) = std::fs::read_to_string(&poetry_lock) {
            if let Ok(parsed) = toml::from_str::<toml::Value>(&content) {
                return build_python_tree(&parsed, project_root);
            }
        }

        Err(PackageError::ParseError(
            "no lockfile found (uv.lock or poetry.lock)".to_string(),
        ))
    }

    fn audit(&self, project_root: &Path) -> Result<AuditResult, PackageError> {
        // Try pip-audit (requires pip-audit installed)
        let output = Command::new("pip-audit")
            .args(["--format", "json"])
            .current_dir(project_root)
            .output();

        let output = match output {
            Ok(o) => o,
            Err(_) => {
                return Err(PackageError::ToolFailed(
                    "pip-audit not installed. Install with: pip install pip-audit".to_string(),
                ));
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim().is_empty() || stdout.trim() == "[]" {
            return Ok(AuditResult {
                vulnerabilities: Vec::new(),
            });
        }

        // Parse pip-audit JSON output (array of vulnerabilities)
        let v: serde_json::Value = serde_json::from_str(&stdout)
            .map_err(|e| PackageError::ParseError(format!("invalid JSON: {}", e)))?;

        let mut vulnerabilities = Vec::new();

        if let Some(arr) = v.as_array() {
            for vuln in arr {
                let package = vuln
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();
                let version = vuln
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                // Each package can have multiple vulnerabilities
                if let Some(vulns) = vuln.get("vulns").and_then(|v| v.as_array()) {
                    for v in vulns {
                        let title = v
                            .get("description")
                            .and_then(|d| d.as_str())
                            .map(|s| {
                                if s.len() > 100 {
                                    format!("{}...", &s[..100])
                                } else {
                                    s.to_string()
                                }
                            })
                            .unwrap_or_default();
                        let cve = v.get("id").and_then(|i| i.as_str()).map(String::from);
                        let fixed_in = v
                            .get("fix_versions")
                            .and_then(|f| f.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            })
                            .filter(|s| !s.is_empty());

                        vulnerabilities.push(Vulnerability {
                            package: package.clone(),
                            version: version.clone(),
                            severity: VulnerabilitySeverity::Unknown, // pip-audit doesn't provide severity
                            title,
                            url: cve
                                .as_ref()
                                .map(|c| format!("https://nvd.nist.gov/vuln/detail/{}", c)),
                            cve,
                            fixed_in,
                        });
                    }
                }
            }
        }

        Ok(AuditResult { vulnerabilities })
    }
}

fn build_python_tree(
    parsed: &toml::Value,
    project_root: &Path,
) -> Result<DependencyTree, PackageError> {
    // Get project name from pyproject.toml
    let pyproject = project_root.join("pyproject.toml");
    let root_name = if let Ok(content) = std::fs::read_to_string(&pyproject) {
        if let Ok(manifest) = toml::from_str::<toml::Value>(&content) {
            manifest
                .get("project")
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
                .map(String::from)
                .or_else(|| {
                    manifest
                        .get("tool")
                        .and_then(|t| t.get("poetry"))
                        .and_then(|p| p.get("name"))
                        .and_then(|n| n.as_str())
                        .map(String::from)
                })
        } else {
            None
        }
    } else {
        None
    };

    let root_name = root_name.unwrap_or_else(|| "root".to_string());

    // Build package map: name -> (version, dependencies)
    let mut packages: std::collections::HashMap<String, (String, Vec<String>)> =
        std::collections::HashMap::new();

    if let Some(pkgs) = parsed.get("package").and_then(|p| p.as_array()) {
        for pkg in pkgs {
            let name = pkg.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let version = pkg.get("version").and_then(|v| v.as_str()).unwrap_or("");
            let deps: Vec<String> = pkg
                .get("dependencies")
                .and_then(|d| d.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|d| d.get("name").and_then(|n| n.as_str()).map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            packages.insert(
                name.to_lowercase().replace(['-', '.'], "_"),
                (version.to_string(), deps),
            );
        }
    }

    fn build_node(
        name: &str,
        packages: &std::collections::HashMap<String, (String, Vec<String>)>,
        visited: &mut std::collections::HashSet<String>,
    ) -> Option<TreeNode> {
        let normalized = name.to_lowercase().replace(['-', '.'], "_");
        let (version, deps) = packages.get(&normalized)?;

        let children = if visited.contains(&normalized) {
            Vec::new()
        } else {
            visited.insert(normalized);
            deps.iter()
                .filter_map(|dep| build_node(dep, packages, visited))
                .collect()
        };

        Some(TreeNode {
            name: name.to_string(),
            version: version.clone(),
            dependencies: children,
        })
    }

    // Build tree from all packages
    let mut visited = std::collections::HashSet::new();
    let mut root_deps = Vec::new();

    if let Some(pkgs) = parsed.get("package").and_then(|p| p.as_array()) {
        for pkg in pkgs {
            let name = pkg.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let normalized = name.to_lowercase().replace(['-', '.'], "_");
            if !visited.contains(&normalized) {
                if let Some(node) = build_node(name, &packages, &mut visited) {
                    root_deps.push(node);
                }
            }
        }
    }

    let root = TreeNode {
        name: root_name,
        version: String::new(),
        dependencies: root_deps,
    };

    Ok(DependencyTree { roots: vec![root] })
}

fn fetch_pypi_info(query: &PackageQuery) -> Result<PackageInfo, PackageError> {
    // PyPI API: /pypi/{package}/json for latest, /pypi/{package}/{version}/json for specific
    let url = match &query.version {
        Some(v) => format!("https://pypi.org/pypi/{}/{}/json", query.name, v),
        None => format!("https://pypi.org/pypi/{}/json", query.name),
    };

    let body = crate::http::get(&url)?;
    parse_pypi_json(&body, &query.name)
}

fn parse_pypi_json(json_str: &str, package: &str) -> Result<PackageInfo, PackageError> {
    let v: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| PackageError::ParseError(format!("invalid JSON: {}", e)))?;

    let info = v
        .get("info")
        .ok_or_else(|| PackageError::ParseError("missing info field".to_string()))?;

    let name = info
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(package)
        .to_string();

    let version = info
        .get("version")
        .and_then(|v| v.as_str())
        .ok_or_else(|| PackageError::ParseError("missing version".to_string()))?
        .to_string();

    let description = info
        .get("summary")
        .and_then(|v| v.as_str())
        .map(String::from);

    let license = info
        .get("license")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);

    let homepage = info
        .get("home_page")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);

    let repository = info.get("project_urls").and_then(|urls| {
        urls.get("Source")
            .or_else(|| urls.get("Repository"))
            .or_else(|| urls.get("GitHub"))
            .and_then(|v| v.as_str())
            .map(String::from)
    });

    // Parse requires_dist for dependencies
    let mut dependencies = Vec::new();
    if let Some(requires) = info.get("requires_dist").and_then(|r| r.as_array()) {
        for req in requires {
            if let Some(req_str) = req.as_str() {
                if let Some(dep) = parse_requirement(req_str) {
                    dependencies.push(dep);
                }
            }
        }
    }

    // Parse extras as features
    let mut features = Vec::new();
    if let Some(extras) = info.get("provides_extra").and_then(|e| e.as_array()) {
        for extra in extras {
            if let Some(extra_name) = extra.as_str() {
                // Find dependencies that require this extra
                let extra_deps: Vec<String> = dependencies
                    .iter()
                    .filter(|d| {
                        d.version_req
                            .as_ref()
                            .is_some_and(|v| v.contains(&format!("extra == '{}'", extra_name)))
                    })
                    .map(|d| d.name.clone())
                    .collect();

                features.push(Feature {
                    name: extra_name.to_string(),
                    description: None,
                    dependencies: extra_deps,
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
        features,
        dependencies,
    })
}

fn parse_requirement(req: &str) -> Option<Dependency> {
    // Parse PEP 508 requirement: "name[extra] (>=1.0) ; marker"
    let req = req.trim();

    // Split on ; to separate requirement from marker
    let (req_part, marker) = req
        .split_once(';')
        .map(|(a, b)| (a.trim(), Some(b)))
        .unwrap_or((req, None));

    // Find the package name (before any [, (, <, >, =, !)
    let name_end = req_part
        .find(|c: char| {
            c == '[' || c == '(' || c == ' ' || c == '<' || c == '>' || c == '=' || c == '!'
        })
        .unwrap_or(req_part.len());

    let name = req_part[..name_end].trim().to_string();
    if name.is_empty() {
        return None;
    }

    // Extract version requirement (only from the part before the marker)
    let version_req = if let Some(start) =
        req_part.find(|c: char| c == '<' || c == '>' || c == '=' || c == '!')
    {
        let version_part = req_part[start..].trim();
        if version_part.is_empty() {
            None
        } else {
            Some(version_part.to_string())
        }
    } else {
        None
    };

    // Check if optional (has marker with "extra")
    let optional = marker.is_some_and(|m| m.contains("extra"));

    Some(Dependency {
        name,
        version_req,
        optional,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_requirement() {
        let dep = parse_requirement("requests>=2.0").unwrap();
        assert_eq!(dep.name, "requests");
        assert_eq!(dep.version_req, Some(">=2.0".to_string()));
        assert!(!dep.optional);

        let dep = parse_requirement("pytest ; extra == 'dev'").unwrap();
        assert_eq!(dep.name, "pytest");
        assert!(dep.optional);

        let dep = parse_requirement("numpy").unwrap();
        assert_eq!(dep.name, "numpy");
        assert_eq!(dep.version_req, None);
    }

    #[test]
    fn test_parse_pypi_json() {
        let json = r#"{
            "info": {
                "name": "requests",
                "version": "2.32.0",
                "summary": "Python HTTP for Humans.",
                "license": "Apache-2.0",
                "home_page": "https://requests.readthedocs.io",
                "project_urls": {
                    "Source": "https://github.com/psf/requests"
                },
                "requires_dist": [
                    "charset-normalizer>=2,<4",
                    "idna>=2.5,<4"
                ],
                "provides_extra": ["socks"]
            }
        }"#;

        let info = parse_pypi_json(json, "requests").unwrap();
        assert_eq!(info.name, "requests");
        assert_eq!(info.version, "2.32.0");
        assert_eq!(info.license, Some("Apache-2.0".to_string()));
        assert_eq!(info.dependencies.len(), 2);
    }
}
