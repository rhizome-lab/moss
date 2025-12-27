//! Deno ecosystem (deno.json, jsr/npm/url imports, deno.lock)

use crate::{
    AuditResult, Dependency, DependencyTree, Ecosystem, LockfileManager, PackageError, PackageInfo,
    PackageQuery, TreeNode,
};
use std::path::Path;
use std::process::Command;

pub struct Deno;

impl Ecosystem for Deno {
    fn name(&self) -> &'static str {
        "deno"
    }

    fn manifest_files(&self) -> &'static [&'static str] {
        &["deno.json", "deno.jsonc"]
    }

    fn lockfiles(&self) -> &'static [LockfileManager] {
        &[LockfileManager {
            filename: "deno.lock",
            manager: "deno",
        }]
    }

    fn tools(&self) -> &'static [&'static str] {
        &["deno"]
    }

    fn fetch_info(&self, query: &PackageQuery, _tool: &str) -> Result<PackageInfo, PackageError> {
        // Try JSR first, then npm
        if let Ok(info) = fetch_jsr_info(&query.name, query.version.as_deref()) {
            return Ok(info);
        }
        fetch_npm_info(&query.name, query.version.as_deref())
    }

    fn installed_version(&self, package: &str, project_root: &Path) -> Option<String> {
        // Check deno.lock
        let lockfile = project_root.join("deno.lock");
        if let Ok(content) = std::fs::read_to_string(&lockfile) {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                // deno.lock v3+ format has "packages" section
                if let Some(packages) = parsed.get("packages") {
                    // Check jsr packages
                    if let Some(jsr) = packages.get("jsr") {
                        if let Some(pkg) = jsr.get(package) {
                            if let Some(v) = pkg.get("version").and_then(|v| v.as_str()) {
                                return Some(v.to_string());
                            }
                        }
                    }
                    // Check npm packages
                    if let Some(npm) = packages.get("npm") {
                        if let Some(pkg) = npm.get(package) {
                            if let Some(v) = pkg.get("version").and_then(|v| v.as_str()) {
                                return Some(v.to_string());
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn list_dependencies(&self, project_root: &Path) -> Result<Vec<Dependency>, PackageError> {
        // Try deno.json first
        let manifest = project_root.join("deno.json");
        let manifest = if manifest.exists() {
            manifest
        } else {
            project_root.join("deno.jsonc")
        };

        let content = std::fs::read_to_string(&manifest)
            .map_err(|e| PackageError::ParseError(format!("failed to read deno.json: {}", e)))?;

        // Strip JSONC comments for deno.jsonc
        let content = strip_jsonc_comments(&content);

        let parsed: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| PackageError::ParseError(format!("invalid JSON: {}", e)))?;

        let mut deps = Vec::new();

        // imports field contains dependencies
        if let Some(imports) = parsed.get("imports").and_then(|i| i.as_object()) {
            for (name, specifier) in imports {
                let spec_str = specifier.as_str().unwrap_or("");
                let version = extract_version_from_specifier(spec_str);
                deps.push(Dependency {
                    name: name.clone(),
                    version_req: version,
                    optional: false,
                });
            }
        }

        Ok(deps)
    }

    fn dependency_tree(&self, project_root: &Path) -> Result<DependencyTree, PackageError> {
        let lockfile = project_root.join("deno.lock");
        if !lockfile.exists() {
            return Err(PackageError::ParseError("deno.lock not found".to_string()));
        }

        let content = std::fs::read_to_string(&lockfile)
            .map_err(|e| PackageError::ParseError(format!("failed to read deno.lock: {}", e)))?;

        let parsed: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| PackageError::ParseError(format!("invalid JSON: {}", e)))?;

        build_tree(&parsed, project_root)
    }

    fn audit(&self, _project_root: &Path) -> Result<AuditResult, PackageError> {
        // Deno doesn't have a built-in audit command yet
        Ok(AuditResult {
            vulnerabilities: Vec::new(),
        })
    }
}

fn fetch_jsr_info(package: &str, version: Option<&str>) -> Result<PackageInfo, PackageError> {
    // JSR API: https://jsr.io/api/packages/@scope/name
    let url = format!("https://jsr.io/api/packages/{}", package);
    let body = crate::http::get(&url)?;
    let v: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| PackageError::ParseError(format!("invalid JSON: {}", e)))?;

    if v.get("error").is_some() {
        return Err(PackageError::NotFound(package.to_string()));
    }

    let name = v
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or(package)
        .to_string();

    let pkg_version = if let Some(v) = version {
        v.to_string()
    } else {
        v.get("latestVersion")
            .and_then(|v| v.as_str())
            .unwrap_or("0.0.0")
            .to_string()
    };

    let description = v
        .get("description")
        .and_then(|d| d.as_str())
        .map(String::from);

    Ok(PackageInfo {
        name,
        version: pkg_version,
        description,
        license: None,
        homepage: Some(format!("https://jsr.io/{}", package)),
        repository: None,
        features: Vec::new(),
        dependencies: Vec::new(),
    })
}

fn fetch_npm_info(package: &str, version: Option<&str>) -> Result<PackageInfo, PackageError> {
    let pkg_spec = match version {
        Some(v) => format!("{}@{}", package, v),
        None => package.to_string(),
    };

    let output = Command::new("npm")
        .args(["view", &pkg_spec, "--json"])
        .output()
        .map_err(|e| PackageError::ToolFailed(e.to_string()))?;

    if !output.status.success() {
        return Err(PackageError::NotFound(package.to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout)
        .map_err(|e| PackageError::ParseError(format!("invalid JSON: {}", e)))?;

    let name = v
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or(package)
        .to_string();

    let pkg_version = v
        .get("version")
        .and_then(|v| v.as_str())
        .ok_or_else(|| PackageError::ParseError("missing version".to_string()))?
        .to_string();

    let description = v
        .get("description")
        .and_then(|d| d.as_str())
        .map(String::from);

    let license = v.get("license").and_then(|l| l.as_str()).map(String::from);

    let homepage = v.get("homepage").and_then(|h| h.as_str()).map(String::from);

    Ok(PackageInfo {
        name,
        version: pkg_version,
        description,
        license,
        homepage,
        repository: None,
        features: Vec::new(),
        dependencies: Vec::new(),
    })
}

fn strip_jsonc_comments(content: &str) -> String {
    let mut result = String::new();
    let mut in_string = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut chars = content.chars().peekable();

    while let Some(c) = chars.next() {
        if in_line_comment {
            if c == '\n' {
                in_line_comment = false;
                result.push(c);
            }
            continue;
        }

        if in_block_comment {
            if c == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_block_comment = false;
            }
            continue;
        }

        if in_string {
            result.push(c);
            if c == '"' {
                in_string = false;
            } else if c == '\\' {
                if let Some(escaped) = chars.next() {
                    result.push(escaped);
                }
            }
            continue;
        }

        if c == '"' {
            in_string = true;
            result.push(c);
        } else if c == '/' {
            if chars.peek() == Some(&'/') {
                chars.next();
                in_line_comment = true;
            } else if chars.peek() == Some(&'*') {
                chars.next();
                in_block_comment = true;
            } else {
                result.push(c);
            }
        } else {
            result.push(c);
        }
    }

    result
}

fn extract_version_from_specifier(spec: &str) -> Option<String> {
    // Handle jsr:@scope/pkg@version
    if spec.starts_with("jsr:") {
        let rest = &spec[4..];
        if let Some(at_pos) = rest.rfind('@') {
            if at_pos > 0 {
                return Some(rest[at_pos + 1..].to_string());
            }
        }
    }
    // Handle npm:pkg@version
    else if spec.starts_with("npm:") {
        let rest = &spec[4..];
        if let Some(at_pos) = rest.rfind('@') {
            if at_pos > 0 {
                return Some(rest[at_pos + 1..].to_string());
            }
        }
    }
    None
}

fn build_tree(
    parsed: &serde_json::Value,
    project_root: &Path,
) -> Result<DependencyTree, PackageError> {
    // Get project info from deno.json
    let manifest = project_root.join("deno.json");
    let (name, version) = if let Ok(content) = std::fs::read_to_string(&manifest) {
        let content = strip_jsonc_comments(&content);
        if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content) {
            (
                pkg.get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("root")
                    .to_string(),
                pkg.get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("0.0.0")
                    .to_string(),
            )
        } else {
            ("root".to_string(), "0.0.0".to_string())
        }
    } else {
        ("root".to_string(), "0.0.0".to_string())
    };

    let mut root_deps = Vec::new();

    // v3+ format: packages.jsr / packages.npm
    if let Some(packages) = parsed.get("packages") {
        // JSR packages
        if let Some(jsr) = packages.get("jsr").and_then(|j| j.as_object()) {
            for (pkg_name, pkg_info) in jsr {
                let version = pkg_info
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                root_deps.push(TreeNode {
                    name: pkg_name.clone(),
                    version: version.to_string(),
                    dependencies: Vec::new(),
                });
            }
        }

        // NPM packages (via npm: specifier)
        if let Some(npm) = packages.get("npm").and_then(|n| n.as_object()) {
            for (pkg_name, pkg_info) in npm {
                let version = pkg_info
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                root_deps.push(TreeNode {
                    name: pkg_name.clone(),
                    version: version.to_string(),
                    dependencies: Vec::new(),
                });
            }
        }
    }

    // v2 format: remote (URL -> hash mapping)
    if let Some(remote) = parsed.get("remote").and_then(|r| r.as_object()) {
        for (url, _hash) in remote {
            // Extract package info from URL like https://deno.land/std@0.177.0/...
            if let Some((name, version)) = parse_deno_url(url) {
                // Deduplicate by name
                if !root_deps.iter().any(|d| d.name == name) {
                    root_deps.push(TreeNode {
                        name,
                        version,
                        dependencies: Vec::new(),
                    });
                }
            }
        }
    }

    let root = TreeNode {
        name,
        version,
        dependencies: root_deps,
    };

    Ok(DependencyTree { roots: vec![root] })
}

/// Parse deno.land URL into (name, version)
fn parse_deno_url(url: &str) -> Option<(String, String)> {
    // https://deno.land/std@0.177.0/...
    // https://deno.land/x/oak@v12.0.0/...
    if url.starts_with("https://deno.land/") {
        let path = &url[18..]; // strip https://deno.land/
        let parts: Vec<&str> = path.split('/').collect();
        if !parts.is_empty() {
            let first = parts[0];
            // std@version or x/pkg@version
            if first == "x" && parts.len() > 1 {
                let pkg = parts[1];
                if let Some(at_pos) = pkg.find('@') {
                    let name = &pkg[..at_pos];
                    let version = &pkg[at_pos + 1..];
                    return Some((format!("x/{}", name), version.to_string()));
                }
            } else if let Some(at_pos) = first.find('@') {
                let name = &first[..at_pos];
                let version = &first[at_pos + 1..];
                return Some((name.to_string(), version.to_string()));
            }
        }
    }
    None
}
