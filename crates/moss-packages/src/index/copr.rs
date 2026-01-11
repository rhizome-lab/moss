//! Fedora Copr package index fetcher.
//!
//! Fetches package metadata from Copr (Community Projects).
//! Copr is Fedora's community build system, similar to AUR for Arch.
//!
//! ## API Strategy
//! - **fetch**: `copr.fedorainfracloud.org/api_3/project?ownername=&projectname=`
//! - **fetch_versions**: `copr.fedorainfracloud.org/api_3/build/list` - build history
//! - **search**: `copr.fedorainfracloud.org/api_3/project/search?query=`
//! - **fetch_all**: Not supported (would need to enumerate all users)
//!
//! API docs: https://copr.fedorainfracloud.org/api_3/

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use std::collections::HashMap;

/// Copr package index fetcher.
pub struct Copr;

impl Copr {
    /// Copr API v3 base URL.
    const API_BASE: &'static str = "https://copr.fedorainfracloud.org/api_3";

    /// Parse a Copr project into PackageMeta.
    fn parse_project(project: &serde_json::Value) -> Option<PackageMeta> {
        let full_name = project["full_name"].as_str()?;
        let name = project["name"].as_str().unwrap_or(full_name);
        let owner = project["ownername"].as_str().unwrap_or("");

        let mut extra = HashMap::new();

        // Add owner info
        extra.insert(
            "owner".to_string(),
            serde_json::Value::String(owner.to_string()),
        );

        // Add chroot repos
        if let Some(repos) = project["chroot_repos"].as_object() {
            let repo_list: Vec<serde_json::Value> = repos
                .keys()
                .map(|k| serde_json::Value::String(k.clone()))
                .collect();
            extra.insert("chroots".to_string(), serde_json::Value::Array(repo_list));
        }

        Some(PackageMeta {
            name: name.to_string(),
            version: "latest".to_string(), // Copr projects don't have a single version
            description: project["description"].as_str().map(String::from),
            homepage: project["homepage"].as_str().map(String::from).or_else(|| {
                Some(format!(
                    "https://copr.fedorainfracloud.org/coprs/{}/",
                    full_name
                ))
            }),
            repository: None,
            license: None,
            binaries: Vec::new(),
            keywords: Vec::new(),
            maintainers: vec![owner.to_string()],
            published: None,
            downloads: None,
            archive_url: None,
            checksum: None,
            extra,
        })
    }

    /// Parse a build into PackageMeta with actual package info.
    fn parse_build(build: &serde_json::Value, project_name: &str) -> Option<PackageMeta> {
        let source_package = build["source_package"].as_object()?;
        let name = source_package
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or(project_name);
        let version = source_package
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let mut extra = HashMap::new();

        // Add build state
        if let Some(state) = build["state"].as_str() {
            extra.insert(
                "build_state".to_string(),
                serde_json::Value::String(state.to_string()),
            );
        }

        Some(PackageMeta {
            name: name.to_string(),
            version: version.to_string(),
            description: None,
            homepage: Some(format!(
                "https://copr.fedorainfracloud.org/coprs/{}/",
                project_name
            )),
            repository: None,
            license: None,
            binaries: Vec::new(),
            keywords: Vec::new(),
            maintainers: Vec::new(),
            published: None,
            downloads: None,
            archive_url: None,
            checksum: None,
            extra,
        })
    }
}

impl PackageIndex for Copr {
    fn ecosystem(&self) -> &'static str {
        "copr"
    }

    fn display_name(&self) -> &'static str {
        "Fedora Copr"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Name format: owner/project or just project (search for it)
        let (owner, project) = if name.contains('/') {
            let parts: Vec<&str> = name.splitn(2, '/').collect();
            (parts[0], parts[1])
        } else {
            // Search for project by name
            let results = self.search(name)?;
            return results
                .into_iter()
                .find(|p| p.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| IndexError::NotFound(name.to_string()));
        };

        let url = format!(
            "{}/project?ownername={}&projectname={}",
            Self::API_BASE,
            owner,
            project
        );
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        Self::parse_project(&response).ok_or_else(|| IndexError::NotFound(name.to_string()))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // Get builds for the project to find versions
        let (owner, project) = if name.contains('/') {
            let parts: Vec<&str> = name.splitn(2, '/').collect();
            (parts[0], parts[1])
        } else {
            return Ok(vec![VersionMeta {
                version: "latest".to_string(),
                released: None,
                yanked: false,
            }]);
        };

        let url = format!(
            "{}/build/list?ownername={}&projectname={}&limit=20",
            Self::API_BASE,
            owner,
            project
        );

        match ureq::get(&url).call() {
            Ok(resp) => {
                let response: serde_json::Value = resp.into_json()?;
                let mut versions = Vec::new();

                if let Some(items) = response["items"].as_array() {
                    for build in items {
                        if let Some(pkg) = Self::parse_build(build, name) {
                            if !versions
                                .iter()
                                .any(|v: &VersionMeta| v.version == pkg.version)
                            {
                                versions.push(VersionMeta {
                                    version: pkg.version,
                                    released: None,
                                    yanked: false,
                                });
                            }
                        }
                    }
                }

                if versions.is_empty() {
                    versions.push(VersionMeta {
                        version: "latest".to_string(),
                        released: None,
                        yanked: false,
                    });
                }

                Ok(versions)
            }
            Err(_) => Ok(vec![VersionMeta {
                version: "latest".to_string(),
                released: None,
                yanked: false,
            }]),
        }
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!(
            "{}/project/search?query={}&limit=50",
            Self::API_BASE,
            urlencoding::encode(query)
        );
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let packages: Vec<PackageMeta> = response["items"]
            .as_array()
            .map(|items| items.iter().filter_map(Self::parse_project).collect())
            .unwrap_or_default();

        Ok(packages)
    }
}
