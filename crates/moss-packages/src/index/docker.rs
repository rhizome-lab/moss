//! Docker container registry index fetcher.
//!
//! Fetches image metadata from container registries.
//!
//! ## API Strategy
//! - **fetch**: `hub.docker.com/v2/repositories/{namespace}/{name}` - Docker Hub API
//! - **fetch_versions**: `hub.docker.com/v2/repositories/{namespace}/{name}/tags`
//! - **search**: `hub.docker.com/v2/search/repositories?query=`
//! - **fetch_all**: Not supported (millions of images)
//!
//! ## Multi-registry Support
//! ```rust,ignore
//! use moss_packages::index::docker::{Docker, DockerRegistry};
//!
//! // All registries (default)
//! let all = Docker::all();
//!
//! // Docker Hub only
//! let hub = Docker::hub();
//!
//! // GitHub Container Registry
//! let ghcr = Docker::ghcr();
//! ```

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use std::collections::HashMap;

/// Available container registries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DockerRegistry {
    /// Docker Hub - the main public registry
    DockerHub,
    /// GitHub Container Registry (ghcr.io)
    Ghcr,
    /// Quay.io (Red Hat)
    Quay,
    /// Google Container Registry (gcr.io)
    Gcr,
}

impl DockerRegistry {
    /// Get the registry name for tagging.
    pub fn name(&self) -> &'static str {
        match self {
            Self::DockerHub => "docker-hub",
            Self::Ghcr => "ghcr",
            Self::Quay => "quay",
            Self::Gcr => "gcr",
        }
    }

    /// Get the registry prefix used in image names.
    pub fn prefix(&self) -> &'static str {
        match self {
            Self::DockerHub => "",
            Self::Ghcr => "ghcr.io/",
            Self::Quay => "quay.io/",
            Self::Gcr => "gcr.io/",
        }
    }

    /// All available registries.
    pub fn all() -> &'static [DockerRegistry] {
        &[Self::DockerHub, Self::Ghcr, Self::Quay, Self::Gcr]
    }

    /// Docker Hub only.
    pub fn docker_hub() -> &'static [DockerRegistry] {
        &[Self::DockerHub]
    }

    /// GitHub Container Registry only.
    pub fn ghcr() -> &'static [DockerRegistry] {
        &[Self::Ghcr]
    }

    /// Cloud-native registries (Quay + GCR).
    pub fn cloud() -> &'static [DockerRegistry] {
        &[Self::Quay, Self::Gcr]
    }
}

/// Docker container registry fetcher with configurable registries.
pub struct Docker {
    registries: Vec<DockerRegistry>,
}

impl Docker {
    /// Create a fetcher with all registries.
    pub fn all() -> Self {
        Self {
            registries: DockerRegistry::all().to_vec(),
        }
    }

    /// Create a fetcher with Docker Hub only.
    pub fn hub() -> Self {
        Self {
            registries: DockerRegistry::docker_hub().to_vec(),
        }
    }

    /// Create a fetcher with GitHub Container Registry only.
    pub fn ghcr() -> Self {
        Self {
            registries: DockerRegistry::ghcr().to_vec(),
        }
    }

    /// Create a fetcher with cloud registries (Quay + GCR).
    pub fn cloud() -> Self {
        Self {
            registries: DockerRegistry::cloud().to_vec(),
        }
    }

    /// Create a fetcher with custom registry selection.
    pub fn with_registries(registries: &[DockerRegistry]) -> Self {
        Self {
            registries: registries.to_vec(),
        }
    }

    /// Detect which registry an image name refers to.
    fn detect_registry(name: &str) -> (DockerRegistry, String) {
        if name.starts_with("ghcr.io/") {
            (
                DockerRegistry::Ghcr,
                name.trim_start_matches("ghcr.io/").to_string(),
            )
        } else if name.starts_with("quay.io/") {
            (
                DockerRegistry::Quay,
                name.trim_start_matches("quay.io/").to_string(),
            )
        } else if name.starts_with("gcr.io/") {
            (
                DockerRegistry::Gcr,
                name.trim_start_matches("gcr.io/").to_string(),
            )
        } else {
            (DockerRegistry::DockerHub, name.to_string())
        }
    }

    /// Fetch from Docker Hub.
    fn fetch_from_dockerhub(name: &str) -> Result<(PackageMeta, DockerRegistry), IndexError> {
        let (namespace, repo) = if name.contains('/') {
            let parts: Vec<&str> = name.splitn(2, '/').collect();
            (parts[0], parts[1])
        } else {
            ("library", name)
        };

        let url = format!(
            "https://hub.docker.com/v2/repositories/{}/{}/",
            namespace, repo
        );
        let response: serde_json::Value = ureq::get(&url)
            .call()
            .map_err(|_| IndexError::NotFound(name.to_string()))?
            .into_json()?;

        // Get latest tag info
        let tags_url = format!(
            "https://hub.docker.com/v2/repositories/{}/{}/tags?page_size=1&ordering=-last_updated",
            namespace, repo
        );
        let tags: serde_json::Value = ureq::get(&tags_url)
            .call()
            .map_err(|_| IndexError::NotFound(name.to_string()))?
            .into_json()?;

        let latest_tag = tags["results"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|t| t["name"].as_str())
            .unwrap_or("latest");

        let keywords: Vec<String> = response["categories"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| c["slug"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let mut extra = HashMap::new();
        extra.insert(
            "source_repo".to_string(),
            serde_json::Value::String("docker-hub".to_string()),
        );

        Ok((
            PackageMeta {
                name: format!(
                    "{}/{}",
                    namespace,
                    response["name"].as_str().unwrap_or(repo)
                ),
                version: latest_tag.to_string(),
                description: response["description"].as_str().map(String::from),
                homepage: None,
                repository: None,
                license: None,
                binaries: Vec::new(),
                keywords,
                maintainers: vec![
                    response["namespace"]
                        .as_str()
                        .unwrap_or(namespace)
                        .to_string(),
                ],
                published: response["last_updated"].as_str().map(String::from),
                downloads: response["pull_count"].as_u64(),
                archive_url: None,
                checksum: None,
                extra,
            },
            DockerRegistry::DockerHub,
        ))
    }

    /// Fetch tags from Docker Hub.
    fn fetch_versions_dockerhub(name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let (namespace, repo) = if name.contains('/') {
            let parts: Vec<&str> = name.splitn(2, '/').collect();
            (parts[0], parts[1])
        } else {
            ("library", name)
        };

        let url = format!(
            "https://hub.docker.com/v2/repositories/{}/{}/tags?page_size=50&ordering=-last_updated",
            namespace, repo
        );
        let response: serde_json::Value = ureq::get(&url)
            .call()
            .map_err(|_| IndexError::NotFound(name.to_string()))?
            .into_json()?;

        let tags = response["results"]
            .as_array()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        Ok(tags
            .iter()
            .filter_map(|t| {
                Some(VersionMeta {
                    version: format!("{} (docker-hub)", t["name"].as_str()?),
                    released: t["last_updated"].as_str().map(String::from),
                    yanked: false,
                })
            })
            .collect())
    }

    /// Fetch all tags with full metadata from Docker Hub.
    fn fetch_all_versions_dockerhub(name: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let (namespace, repo) = if name.contains('/') {
            let parts: Vec<&str> = name.splitn(2, '/').collect();
            (parts[0], parts[1])
        } else {
            ("library", name)
        };

        // Get repository info for shared metadata
        let repo_url = format!(
            "https://hub.docker.com/v2/repositories/{}/{}/",
            namespace, repo
        );
        let repo_info: serde_json::Value = ureq::get(&repo_url)
            .call()
            .map_err(|_| IndexError::NotFound(name.to_string()))?
            .into_json()?;

        let description = repo_info["description"].as_str().map(String::from);
        let pull_count = repo_info["pull_count"].as_u64();

        // Get tags with full metadata
        let tags_url = format!(
            "https://hub.docker.com/v2/repositories/{}/{}/tags?page_size=100&ordering=-last_updated",
            namespace, repo
        );
        let response: serde_json::Value = ureq::get(&tags_url)
            .call()
            .map_err(|_| IndexError::NotFound(name.to_string()))?
            .into_json()?;

        let tags = response["results"]
            .as_array()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        let full_name = format!("{}/{}", namespace, repo);

        Ok(tags
            .iter()
            .filter_map(|t| {
                let tag_name = t["name"].as_str()?;
                let mut extra = HashMap::new();

                extra.insert(
                    "source_repo".to_string(),
                    serde_json::Value::String("docker-hub".to_string()),
                );

                // Digest
                if let Some(digest) = t["digest"].as_str() {
                    extra.insert(
                        "digest".to_string(),
                        serde_json::Value::String(digest.to_string()),
                    );
                }

                // Full size in bytes
                if let Some(size) = t["full_size"].as_u64() {
                    extra.insert("size".to_string(), serde_json::Value::Number(size.into()));
                }

                // Architecture info from images array
                if let Some(images) = t["images"].as_array() {
                    let archs: Vec<serde_json::Value> = images
                        .iter()
                        .filter_map(|img| {
                            img["architecture"]
                                .as_str()
                                .map(|a| serde_json::Value::String(a.to_string()))
                        })
                        .collect();
                    if !archs.is_empty() {
                        extra.insert("architectures".to_string(), serde_json::Value::Array(archs));
                    }

                    // OS info
                    let os_list: Vec<serde_json::Value> = images
                        .iter()
                        .filter_map(|img| {
                            img["os"]
                                .as_str()
                                .map(|o| serde_json::Value::String(o.to_string()))
                        })
                        .collect();
                    if !os_list.is_empty() {
                        // Dedupe
                        let unique: std::collections::HashSet<_> =
                            os_list.iter().filter_map(|v| v.as_str()).collect();
                        let unique_vec: Vec<serde_json::Value> = unique
                            .into_iter()
                            .map(|s| serde_json::Value::String(s.to_string()))
                            .collect();
                        extra.insert("os".to_string(), serde_json::Value::Array(unique_vec));
                    }
                }

                Some(PackageMeta {
                    name: full_name.clone(),
                    version: tag_name.to_string(),
                    description: description.clone(),
                    homepage: None,
                    repository: None,
                    license: None,
                    binaries: Vec::new(),
                    keywords: Vec::new(),
                    maintainers: vec![namespace.to_string()],
                    published: t["last_updated"].as_str().map(String::from),
                    downloads: pull_count,
                    archive_url: None,
                    checksum: t["digest"].as_str().map(String::from),
                    extra,
                })
            })
            .collect())
    }

    /// Fetch from Quay.io.
    fn fetch_from_quay(name: &str) -> Result<(PackageMeta, DockerRegistry), IndexError> {
        let (namespace, repo) = if name.contains('/') {
            let parts: Vec<&str> = name.splitn(2, '/').collect();
            (parts[0], parts[1])
        } else {
            return Err(IndexError::Parse(
                "Quay.io requires namespace/repo format".into(),
            ));
        };

        let url = format!("https://quay.io/api/v1/repository/{}/{}", namespace, repo);
        let response: serde_json::Value = ureq::get(&url)
            .call()
            .map_err(|_| IndexError::NotFound(name.to_string()))?
            .into_json()?;

        let latest_tag = response["tags"]
            .as_object()
            .and_then(|tags| tags.keys().next())
            .map(|s| s.as_str())
            .unwrap_or("latest");

        let mut extra = HashMap::new();
        extra.insert(
            "source_repo".to_string(),
            serde_json::Value::String("quay".to_string()),
        );

        Ok((
            PackageMeta {
                name: format!("quay.io/{}/{}", namespace, repo),
                version: latest_tag.to_string(),
                description: response["description"].as_str().map(String::from),
                homepage: None,
                repository: None,
                license: None,
                binaries: Vec::new(),
                keywords: Vec::new(),
                maintainers: vec![namespace.to_string()],
                published: None,
                downloads: None,
                archive_url: None,
                checksum: None,
                extra,
            },
            DockerRegistry::Quay,
        ))
    }

    /// Fetch tags from Quay.io.
    fn fetch_versions_quay(name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let (namespace, repo) = if name.contains('/') {
            let parts: Vec<&str> = name.splitn(2, '/').collect();
            (parts[0], parts[1])
        } else {
            return Err(IndexError::Parse(
                "Quay.io requires namespace/repo format".into(),
            ));
        };

        let url = format!(
            "https://quay.io/api/v1/repository/{}/{}/tag/",
            namespace, repo
        );
        let response: serde_json::Value = ureq::get(&url)
            .call()
            .map_err(|_| IndexError::NotFound(name.to_string()))?
            .into_json()?;

        let tags = response["tags"]
            .as_array()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        Ok(tags
            .iter()
            .filter_map(|t| {
                Some(VersionMeta {
                    version: format!("{} (quay)", t["name"].as_str()?),
                    released: t["last_modified"].as_str().map(String::from),
                    yanked: false,
                })
            })
            .collect())
    }

    /// Search Docker Hub.
    fn search_dockerhub(query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!(
            "https://hub.docker.com/v2/search/repositories?query={}&page_size=25",
            query
        );
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let results = response["results"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("Invalid search response".into()))?;

        let mut extra = HashMap::new();
        extra.insert(
            "source_repo".to_string(),
            serde_json::Value::String("docker-hub".to_string()),
        );

        Ok(results
            .iter()
            .filter_map(|img| {
                let name = if img["is_official"].as_bool().unwrap_or(false) {
                    format!("library/{}", img["repo_name"].as_str()?)
                } else {
                    img["repo_name"].as_str()?.to_string()
                };

                Some(PackageMeta {
                    name,
                    version: "latest".to_string(),
                    description: img["short_description"].as_str().map(String::from),
                    homepage: None,
                    repository: None,
                    license: None,
                    binaries: Vec::new(),
                    keywords: Vec::new(),
                    maintainers: Vec::new(),
                    published: None,
                    downloads: img["pull_count"].as_u64(),
                    archive_url: None,
                    checksum: None,
                    extra: extra.clone(),
                })
            })
            .collect())
    }

    /// Search Quay.io.
    fn search_quay(query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!("https://quay.io/api/v1/find/repositories?query={}", query);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let results = response["results"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("Invalid search response".into()))?;

        let mut extra = HashMap::new();
        extra.insert(
            "source_repo".to_string(),
            serde_json::Value::String("quay".to_string()),
        );

        Ok(results
            .iter()
            .filter_map(|repo| {
                let namespace = repo["namespace"]["name"].as_str()?;
                let name = repo["name"].as_str()?;

                Some(PackageMeta {
                    name: format!("quay.io/{}/{}", namespace, name),
                    version: "latest".to_string(),
                    description: repo["description"].as_str().map(String::from),
                    homepage: None,
                    repository: None,
                    license: None,
                    binaries: Vec::new(),
                    keywords: Vec::new(),
                    maintainers: vec![namespace.to_string()],
                    published: None,
                    downloads: None,
                    archive_url: None,
                    checksum: None,
                    extra: extra.clone(),
                })
            })
            .collect())
    }
}

impl PackageIndex for Docker {
    fn ecosystem(&self) -> &'static str {
        "docker"
    }

    fn display_name(&self) -> &'static str {
        "Container Registries (Docker)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let (detected_registry, clean_name) = Self::detect_registry(name);

        // If the detected registry is in our configured list, use it
        if self.registries.contains(&detected_registry) {
            return match detected_registry {
                DockerRegistry::DockerHub => {
                    Self::fetch_from_dockerhub(&clean_name).map(|(p, _)| p)
                }
                DockerRegistry::Quay => Self::fetch_from_quay(&clean_name).map(|(p, _)| p),
                DockerRegistry::Ghcr | DockerRegistry::Gcr => {
                    // GHCR and GCR require authentication for most operations
                    // Return basic metadata from what we know
                    let mut extra = HashMap::new();
                    extra.insert(
                        "source_repo".to_string(),
                        serde_json::Value::String(detected_registry.name().to_string()),
                    );
                    Ok(PackageMeta {
                        name: format!("{}{}", detected_registry.prefix(), clean_name),
                        version: "latest".to_string(),
                        description: None,
                        homepage: None,
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
            };
        }

        // Try each configured registry
        for &registry in &self.registries {
            let result = match registry {
                DockerRegistry::DockerHub => Self::fetch_from_dockerhub(name),
                DockerRegistry::Quay => Self::fetch_from_quay(name),
                DockerRegistry::Ghcr | DockerRegistry::Gcr => continue, // Skip auth-required registries
            };

            if let Ok((pkg, _)) = result {
                return Ok(pkg);
            }
        }

        Err(IndexError::NotFound(name.to_string()))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let (detected_registry, clean_name) = Self::detect_registry(name);
        let mut all_versions = Vec::new();

        // If the detected registry is in our configured list, use it
        if self.registries.contains(&detected_registry) {
            let versions = match detected_registry {
                DockerRegistry::DockerHub => Self::fetch_versions_dockerhub(&clean_name),
                DockerRegistry::Quay => Self::fetch_versions_quay(&clean_name),
                DockerRegistry::Ghcr | DockerRegistry::Gcr => {
                    // These require authentication
                    Err(IndexError::Parse("Registry requires authentication".into()))
                }
            };

            if let Ok(v) = versions {
                return Ok(v);
            }
        }

        // Try each configured registry
        for &registry in &self.registries {
            let result = match registry {
                DockerRegistry::DockerHub => Self::fetch_versions_dockerhub(name),
                DockerRegistry::Quay => Self::fetch_versions_quay(name),
                DockerRegistry::Ghcr | DockerRegistry::Gcr => continue,
            };

            if let Ok(versions) = result {
                all_versions.extend(versions);
            }
        }

        if all_versions.is_empty() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        Ok(all_versions)
    }

    fn fetch_all_versions(&self, name: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let (detected_registry, clean_name) = Self::detect_registry(name);

        // If the detected registry is in our configured list, use it
        if self.registries.contains(&detected_registry) {
            return match detected_registry {
                DockerRegistry::DockerHub => Self::fetch_all_versions_dockerhub(&clean_name),
                DockerRegistry::Quay | DockerRegistry::Ghcr | DockerRegistry::Gcr => {
                    // Fall back to default implementation for other registries
                    let versions = self.fetch_versions(name)?;
                    Ok(versions
                        .into_iter()
                        .map(|v| PackageMeta {
                            name: name.to_string(),
                            version: v.version,
                            published: v.released,
                            ..Default::default()
                        })
                        .collect())
                }
            };
        }

        // Try Docker Hub if configured
        if self.registries.contains(&DockerRegistry::DockerHub) {
            if let Ok(versions) = Self::fetch_all_versions_dockerhub(name) {
                return Ok(versions);
            }
        }

        Err(IndexError::NotFound(name.to_string()))
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let mut results = Vec::new();

        // Search Docker Hub if configured
        if self.registries.contains(&DockerRegistry::DockerHub) {
            if let Ok(packages) = Self::search_dockerhub(query) {
                results.extend(packages);
            }
        }

        // Search Quay if configured
        if self.registries.contains(&DockerRegistry::Quay) {
            if let Ok(packages) = Self::search_quay(query) {
                results.extend(packages);
            }
        }

        // GHCR and GCR don't have public search APIs

        Ok(results)
    }
}
