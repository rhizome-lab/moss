//! Maven (Java) ecosystem.

use crate::{
    AuditResult, Dependency, DependencyTree, Ecosystem, LockfileManager, PackageError, PackageInfo,
    PackageQuery, TreeNode,
};
use std::path::Path;

pub struct Maven;

impl Ecosystem for Maven {
    fn name(&self) -> &'static str {
        "maven"
    }

    fn manifest_files(&self) -> &'static [&'static str] {
        &["pom.xml", "build.gradle", "build.gradle.kts"]
    }

    fn lockfiles(&self) -> &'static [LockfileManager] {
        &[
            LockfileManager {
                filename: "gradle.lockfile",
                manager: "gradle",
            },
            LockfileManager {
                filename: "buildscript-gradle.lockfile",
                manager: "gradle",
            },
        ]
    }

    fn tools(&self) -> &'static [&'static str] {
        &["curl"] // Uses Maven Central API
    }

    fn fetch_info(&self, query: &PackageQuery, _tool: &str) -> Result<PackageInfo, PackageError> {
        fetch_maven_info(&query.name)
    }

    fn installed_version(&self, package: &str, project_root: &Path) -> Option<String> {
        // gradle.lockfile format:
        // group:artifact:version=hash
        let lockfile = project_root.join("gradle.lockfile");
        let content = std::fs::read_to_string(lockfile).ok()?;

        // Package can be "group:artifact" or just "artifact"
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // Format: group:artifact:version=hash
            let coord = line.split('=').next()?;
            let parts: Vec<&str> = coord.split(':').collect();
            if parts.len() >= 3 {
                let coord_str = format!("{}:{}", parts[0], parts[1]);
                if coord_str == package || parts[1] == package {
                    return Some(parts[2].to_string());
                }
            }
        }
        None
    }

    fn list_dependencies(&self, project_root: &Path) -> Result<Vec<Dependency>, PackageError> {
        // Try pom.xml first
        let pom = project_root.join("pom.xml");
        if let Ok(content) = std::fs::read_to_string(&pom) {
            return parse_pom_dependencies(&content);
        }

        // Try build.gradle or build.gradle.kts
        for gradle_file in ["build.gradle", "build.gradle.kts"] {
            let gradle = project_root.join(gradle_file);
            if let Ok(content) = std::fs::read_to_string(&gradle) {
                return parse_gradle_dependencies(&content);
            }
        }

        Err(PackageError::ParseError("no manifest found".to_string()))
    }

    fn dependency_tree(&self, project_root: &Path) -> Result<DependencyTree, PackageError> {
        // Try gradle.lockfile first
        let lockfile = project_root.join("gradle.lockfile");
        if let Ok(content) = std::fs::read_to_string(&lockfile) {
            let mut deps = Vec::new();

            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                // Format: group:artifact:version=hash
                if let Some(coord) = line.split('=').next() {
                    let parts: Vec<&str> = coord.split(':').collect();
                    let (name, version) = if parts.len() >= 3 {
                        (format!("{}:{}", parts[0], parts[1]), parts[2].to_string())
                    } else {
                        (coord.to_string(), String::new())
                    };
                    deps.push(TreeNode {
                        name,
                        version,
                        dependencies: Vec::new(),
                    });
                }
            }

            return Ok(DependencyTree {
                roots: vec![TreeNode {
                    name: "gradle.lockfile".to_string(),
                    version: String::new(),
                    dependencies: deps,
                }],
            });
        }

        // Fall back to listing direct deps from manifest
        let manifest_deps = self.list_dependencies(project_root)?;
        let deps: Vec<TreeNode> = manifest_deps
            .into_iter()
            .map(|dep| TreeNode {
                name: dep.name,
                version: dep.version_req.unwrap_or_default(),
                dependencies: Vec::new(),
            })
            .collect();

        Ok(DependencyTree {
            roots: vec![TreeNode {
                name: "dependencies".to_string(),
                version: String::new(),
                dependencies: deps,
            }],
        })
    }

    fn audit(&self, _project_root: &Path) -> Result<AuditResult, PackageError> {
        Err(PackageError::ToolFailed(
            "audit not yet supported for Maven. Use OWASP dependency-check or Snyk".to_string(),
        ))
    }
}

fn parse_pom_dependencies(content: &str) -> Result<Vec<Dependency>, PackageError> {
    let mut deps = Vec::new();

    // Simple XML parsing: <dependency><groupId>...</groupId><artifactId>...</artifactId><version>...</version></dependency>
    let mut in_dependency = false;
    let mut group_id = String::new();
    let mut artifact_id = String::new();
    let mut version = String::new();
    let mut optional = false;

    for line in content.lines() {
        let line = line.trim();

        if line.contains("<dependency>") {
            in_dependency = true;
            group_id.clear();
            artifact_id.clear();
            version.clear();
            optional = false;
        } else if line.contains("</dependency>") {
            if in_dependency && !artifact_id.is_empty() {
                deps.push(Dependency {
                    name: if group_id.is_empty() {
                        artifact_id.clone()
                    } else {
                        format!("{}:{}", group_id, artifact_id)
                    },
                    version_req: if version.is_empty() {
                        None
                    } else {
                        Some(version.clone())
                    },
                    optional,
                });
            }
            in_dependency = false;
        } else if in_dependency {
            if let Some(val) = extract_xml_value(line, "groupId") {
                group_id = val;
            } else if let Some(val) = extract_xml_value(line, "artifactId") {
                artifact_id = val;
            } else if let Some(val) = extract_xml_value(line, "version") {
                version = val;
            } else if line.contains("<optional>true</optional>") {
                optional = true;
            }
        }
    }

    Ok(deps)
}

fn extract_xml_value(line: &str, tag: &str) -> Option<String> {
    let start_tag = format!("<{}>", tag);
    let end_tag = format!("</{}>", tag);

    if let Some(start) = line.find(&start_tag) {
        let content_start = start + start_tag.len();
        if let Some(end) = line.find(&end_tag) {
            return Some(line[content_start..end].to_string());
        }
    }
    None
}

fn parse_gradle_dependencies(content: &str) -> Result<Vec<Dependency>, PackageError> {
    let mut deps = Vec::new();

    // Parse implementation 'group:artifact:version' or implementation("group:artifact:version")
    for line in content.lines() {
        let line = line.trim();

        // Check for dependency declarations
        for prefix in [
            "implementation",
            "api",
            "compileOnly",
            "runtimeOnly",
            "testImplementation",
        ] {
            if line.starts_with(prefix) {
                // Extract the dependency string from quotes
                let after = &line[prefix.len()..];
                let dep_str = if let Some(start) = after.find('"') {
                    let rest = &after[start + 1..];
                    rest.find('"').map(|end| &rest[..end])
                } else if let Some(start) = after.find('\'') {
                    let rest = &after[start + 1..];
                    rest.find('\'').map(|end| &rest[..end])
                } else {
                    None
                };

                if let Some(dep) = dep_str {
                    let parts: Vec<&str> = dep.split(':').collect();
                    if parts.len() >= 2 {
                        deps.push(Dependency {
                            name: format!("{}:{}", parts[0], parts[1]),
                            version_req: parts.get(2).map(|s| s.to_string()),
                            optional: prefix == "compileOnly" || prefix == "testImplementation",
                        });
                    }
                }
            }
        }
    }

    Ok(deps)
}

fn fetch_maven_info(package: &str) -> Result<PackageInfo, PackageError> {
    // Package format: groupId:artifactId or groupId:artifactId:version
    let parts: Vec<&str> = package.split(':').collect();
    let (group_id, artifact_id) = match parts.len() {
        1 => {
            // Try to find in Maven Central search
            return search_maven_central(package);
        }
        2 => (parts[0], parts[1]),
        _ => (parts[0], parts[1]),
    };

    // Query Maven Central API
    let url = format!(
        "https://search.maven.org/solrsearch/select?q=g:{}+AND+a:{}&rows=1&wt=json",
        group_id, artifact_id
    );

    let body = crate::http::get(&url)?;
    parse_maven_response(&body, package)
}

fn search_maven_central(query: &str) -> Result<PackageInfo, PackageError> {
    let url = format!(
        "https://search.maven.org/solrsearch/select?q={}&rows=1&wt=json",
        query
    );

    let body = crate::http::get(&url)?;
    parse_maven_response(&body, query)
}

fn parse_maven_response(json: &str, package: &str) -> Result<PackageInfo, PackageError> {
    let v: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| PackageError::ParseError(format!("invalid JSON: {}", e)))?;

    let docs = v
        .get("response")
        .and_then(|r| r.get("docs"))
        .and_then(|d| d.as_array())
        .ok_or_else(|| PackageError::ParseError("missing response.docs".to_string()))?;

    let doc = docs
        .first()
        .ok_or_else(|| PackageError::NotFound(package.to_string()))?;

    let group_id = doc.get("g").and_then(|g| g.as_str()).unwrap_or("");
    let artifact_id = doc.get("a").and_then(|a| a.as_str()).unwrap_or(package);

    let name = format!("{}:{}", group_id, artifact_id);

    let version = doc
        .get("latestVersion")
        .and_then(|v| v.as_str())
        .ok_or_else(|| PackageError::ParseError("missing latestVersion".to_string()))?
        .to_string();

    // Maven Central search doesn't provide much metadata
    // Would need to fetch pom.xml for full info
    Ok(PackageInfo {
        name,
        version,
        description: None,
        license: None,
        homepage: Some(format!(
            "https://central.sonatype.com/artifact/{}/{}",
            group_id, artifact_id
        )),
        repository: None,
        features: Vec::new(),
        dependencies: Vec::new(),
    })
}
