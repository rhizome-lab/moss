//! NuGet (.NET) ecosystem.

use crate::{
    AuditResult, Dependency, DependencyTree, Ecosystem, LockfileManager, PackageError, PackageInfo,
    PackageQuery, TreeNode,
};
use std::path::Path;

pub struct Nuget;

impl Ecosystem for Nuget {
    fn name(&self) -> &'static str {
        "nuget"
    }

    fn manifest_files(&self) -> &'static [&'static str] {
        &["*.csproj", "*.fsproj", "*.vbproj", "packages.config"]
    }

    fn lockfiles(&self) -> &'static [LockfileManager] {
        &[LockfileManager {
            filename: "packages.lock.json",
            manager: "dotnet",
        }]
    }

    fn tools(&self) -> &'static [&'static str] {
        &["curl"] // Uses NuGet API
    }

    fn fetch_info(&self, query: &PackageQuery, _tool: &str) -> Result<PackageInfo, PackageError> {
        fetch_nuget_info(&query.name)
    }

    fn installed_version(&self, package: &str, project_root: &Path) -> Option<String> {
        // packages.lock.json format:
        // {"dependencies": {"net8.0": {"PackageName": {"resolved": "1.0.0", ...}}}}
        let lockfile = project_root.join("packages.lock.json");
        let content = std::fs::read_to_string(lockfile).ok()?;
        let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;

        // Check all target frameworks
        if let Some(deps) = parsed.get("dependencies").and_then(|d| d.as_object()) {
            for (_, framework_deps) in deps {
                if let Some(pkg) = framework_deps.get(package) {
                    if let Some(v) = pkg.get("resolved").and_then(|v| v.as_str()) {
                        return Some(v.to_string());
                    }
                }
            }
        }
        None
    }

    fn list_dependencies(&self, project_root: &Path) -> Result<Vec<Dependency>, PackageError> {
        // Look for .csproj files and parse PackageReference elements
        let entries = std::fs::read_dir(project_root)
            .map_err(|e| PackageError::ParseError(format!("failed to read directory: {}", e)))?;

        let mut deps = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "csproj" || ext == "fsproj" || ext == "vbproj" {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        // Parse <PackageReference Include="Name" Version="1.0" />
                        for line in content.lines() {
                            if line.contains("PackageReference") {
                                if let Some(include_start) = line.find("Include=\"") {
                                    let after = &line[include_start + 9..];
                                    if let Some(include_end) = after.find('"') {
                                        let name = after[..include_end].to_string();
                                        let version_req =
                                            if let Some(ver_start) = line.find("Version=\"") {
                                                let ver_after = &line[ver_start + 9..];
                                                ver_after
                                                    .find('"')
                                                    .map(|end| ver_after[..end].to_string())
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
                    }
                }
            }
        }

        Ok(deps)
    }

    fn dependency_tree(&self, project_root: &Path) -> Result<DependencyTree, PackageError> {
        // Parse packages.lock.json
        let lockfile = project_root.join("packages.lock.json");
        let content = std::fs::read_to_string(&lockfile).map_err(|e| {
            PackageError::ParseError(format!("failed to read packages.lock.json: {}", e))
        })?;
        let parsed: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| PackageError::ParseError(format!("invalid JSON: {}", e)))?;

        let mut framework_nodes = Vec::new();

        if let Some(deps) = parsed.get("dependencies").and_then(|d| d.as_object()) {
            for (framework, framework_deps) in deps {
                let mut pkg_nodes = Vec::new();
                if let Some(pkgs) = framework_deps.as_object() {
                    for (name, info) in pkgs {
                        let version = info.get("resolved").and_then(|v| v.as_str()).unwrap_or("");
                        pkg_nodes.push(TreeNode {
                            name: name.clone(),
                            version: version.to_string(),
                            dependencies: Vec::new(),
                        });
                    }
                }
                framework_nodes.push(TreeNode {
                    name: framework.clone(),
                    version: String::new(),
                    dependencies: pkg_nodes,
                });
            }
        }

        Ok(DependencyTree {
            roots: vec![TreeNode {
                name: "packages.lock.json".to_string(),
                version: String::new(),
                dependencies: framework_nodes,
            }],
        })
    }

    fn audit(&self, _project_root: &Path) -> Result<AuditResult, PackageError> {
        Err(PackageError::ToolFailed(
            "audit not yet supported for NuGet. Use: dotnet list package --vulnerable".to_string(),
        ))
    }
}

fn fetch_nuget_info(package: &str) -> Result<PackageInfo, PackageError> {
    // First get the latest version
    let index_url = format!(
        "https://api.nuget.org/v3-flatcontainer/{}/index.json",
        package.to_lowercase()
    );

    let body = crate::http::get(&index_url)?;
    let index: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| PackageError::ParseError(format!("invalid JSON: {}", e)))?;

    let version = index
        .get("versions")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.last())
        .and_then(|v| v.as_str())
        .ok_or_else(|| PackageError::ParseError("no versions found".to_string()))?;

    // Get package metadata from nuspec
    let nuspec_url = format!(
        "https://api.nuget.org/v3-flatcontainer/{}/{}/{}.nuspec",
        package.to_lowercase(),
        version,
        package.to_lowercase()
    );

    // Return basic info if nuspec not available
    let nuspec = match crate::http::get(&nuspec_url) {
        Ok(body) => body,
        Err(_) => {
            return Ok(PackageInfo {
                name: package.to_string(),
                version: version.to_string(),
                description: None,
                license: None,
                homepage: Some(format!("https://www.nuget.org/packages/{}", package)),
                repository: None,
                features: Vec::new(),
                dependencies: Vec::new(),
            });
        }
    };

    parse_nuspec(&nuspec, package, version)
}

fn parse_nuspec(xml: &str, package: &str, version: &str) -> Result<PackageInfo, PackageError> {
    // Simple XML parsing - extract key fields
    fn extract_tag(xml: &str, tag: &str) -> Option<String> {
        let start_tag = format!("<{}", tag);
        let end_tag = format!("</{}>", tag);

        let start = xml.find(&start_tag)?;
        let content_start = xml[start..].find('>')? + start + 1;
        let end = xml[content_start..].find(&end_tag)? + content_start;

        let content = xml[content_start..end].trim();
        if content.is_empty() {
            None
        } else {
            Some(content.to_string())
        }
    }

    let description = extract_tag(xml, "description");
    let license = extract_tag(xml, "license").or_else(|| extract_tag(xml, "licenseUrl"));
    let homepage = extract_tag(xml, "projectUrl");
    let repository = extract_tag(xml, "repository");

    // Parse dependencies
    let mut dependencies = Vec::new();
    if let Some(deps_start) = xml.find("<dependencies>") {
        if let Some(deps_end) = xml[deps_start..].find("</dependencies>") {
            let deps_section = &xml[deps_start..deps_start + deps_end];
            // Find all <dependency id="..." version="..." />
            for dep_match in deps_section.split("<dependency") {
                if let Some(id_start) = dep_match.find("id=\"") {
                    let id_content = &dep_match[id_start + 4..];
                    if let Some(id_end) = id_content.find('"') {
                        let dep_name = id_content[..id_end].to_string();
                        let version_req = if let Some(ver_start) = dep_match.find("version=\"") {
                            let ver_content = &dep_match[ver_start + 9..];
                            ver_content
                                .find('"')
                                .map(|end| ver_content[..end].to_string())
                        } else {
                            None
                        };
                        dependencies.push(Dependency {
                            name: dep_name,
                            version_req,
                            optional: false,
                        });
                    }
                }
            }
        }
    }

    Ok(PackageInfo {
        name: package.to_string(),
        version: version.to_string(),
        description,
        license,
        homepage,
        repository,
        features: Vec::new(),
        dependencies,
    })
}
