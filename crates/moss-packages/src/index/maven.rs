//! Maven package index fetcher (Java).
//!
//! Fetches package metadata from Maven repositories.
//!
//! ## API Strategy
//! - **fetch**: `search.maven.org/solrsearch/select?q=g:{group}+AND+a:{artifact}`
//! - **fetch_versions**: Same API with `core=gav` for all versions
//! - **search**: `search.maven.org/solrsearch/select?q=`
//! - **fetch_all**: Not supported (millions of artifacts)
//!
//! ## Multi-repository Support
//! ```rust,ignore
//! use moss_packages::index::maven::{Maven, MavenRepo};
//!
//! // All repositories (default)
//! let all = Maven::all();
//!
//! // Maven Central only
//! let central = Maven::central();
//!
//! // Google Maven (Android)
//! let google = Maven::google();
//! ```

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use std::collections::HashMap;

/// Available Maven repositories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MavenRepo {
    /// Maven Central - the main public repository
    Central,
    /// Google Maven - Android and Google libraries
    Google,
    /// Sonatype OSS - open source snapshots and releases
    Sonatype,
}

impl MavenRepo {
    /// Get the repository base URL.
    fn base_url(&self) -> &'static str {
        match self {
            Self::Central => "https://repo1.maven.org/maven2",
            Self::Google => "https://maven.google.com",
            Self::Sonatype => "https://oss.sonatype.org/content/repositories/releases",
        }
    }

    /// Get the repository name for tagging.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Central => "central",
            Self::Google => "google",
            Self::Sonatype => "sonatype",
        }
    }

    /// All available repositories.
    pub fn all() -> &'static [MavenRepo] {
        &[Self::Central, Self::Google, Self::Sonatype]
    }

    /// Maven Central only.
    pub fn central() -> &'static [MavenRepo] {
        &[Self::Central]
    }

    /// Google Maven only (Android development).
    pub fn google() -> &'static [MavenRepo] {
        &[Self::Google]
    }

    /// Android development repositories (Central + Google).
    pub fn android() -> &'static [MavenRepo] {
        &[Self::Central, Self::Google]
    }
}

/// Maven package index fetcher with configurable repositories.
pub struct Maven {
    repos: Vec<MavenRepo>,
}

impl Maven {
    /// Create a fetcher with all repositories.
    pub fn all() -> Self {
        Self {
            repos: MavenRepo::all().to_vec(),
        }
    }

    /// Create a fetcher with Maven Central only.
    pub fn central() -> Self {
        Self {
            repos: MavenRepo::central().to_vec(),
        }
    }

    /// Create a fetcher with Google Maven only.
    pub fn google() -> Self {
        Self {
            repos: MavenRepo::google().to_vec(),
        }
    }

    /// Create a fetcher for Android development (Central + Google).
    pub fn android() -> Self {
        Self {
            repos: MavenRepo::android().to_vec(),
        }
    }

    /// Create a fetcher with custom repository selection.
    pub fn with_repos(repos: &[MavenRepo]) -> Self {
        Self {
            repos: repos.to_vec(),
        }
    }

    /// Check if a package exists in a repository by trying to fetch metadata.
    fn check_repo_exists(
        group_id: &str,
        artifact_id: &str,
        repo: MavenRepo,
    ) -> Result<(String, MavenRepo), IndexError> {
        // Try to fetch maven-metadata.xml to get latest version
        let group_path = group_id.replace('.', "/");
        let metadata_url = format!(
            "{}/{}/{}/maven-metadata.xml",
            repo.base_url(),
            group_path,
            artifact_id
        );

        let response = ureq::get(&metadata_url)
            .call()
            .map_err(|_| IndexError::NotFound(format!("{}:{}", group_id, artifact_id)))?;

        let body = response
            .into_string()
            .map_err(|e| IndexError::Parse(e.to_string()))?;

        // Parse the latest version from maven-metadata.xml
        let version = extract_version_from_metadata(&body).unwrap_or_else(|| "unknown".to_string());

        Ok((version, repo))
    }

    /// Fetch using Maven Central's search API.
    fn fetch_from_central_api(
        group_id: &str,
        artifact_id: &str,
    ) -> Result<(PackageMeta, MavenRepo), IndexError> {
        let url = format!(
            "https://search.maven.org/solrsearch/select?q=g:{}+AND+a:{}&rows=1&wt=json",
            group_id, artifact_id
        );
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let docs = response["response"]["docs"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing docs".into()))?;

        let doc = docs
            .first()
            .ok_or_else(|| IndexError::NotFound(format!("{}:{}", group_id, artifact_id)))?;

        let mut extra = HashMap::new();
        extra.insert(
            "source_repo".to_string(),
            serde_json::Value::String("central".to_string()),
        );

        let g = doc["g"].as_str().unwrap_or("");
        let a = doc["a"].as_str().unwrap_or("");
        Ok((
            PackageMeta {
                name: format!("{}:{}", g, a),
                version: doc["latestVersion"]
                    .as_str()
                    .or_else(|| doc["v"].as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                description: None,
                homepage: Some(format!("https://mvnrepository.com/artifact/{}/{}", g, a)),
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
            },
            MavenRepo::Central,
        ))
    }

    /// Search using Maven Central's search API.
    fn search_central(query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!(
            "https://search.maven.org/solrsearch/select?q={}&rows=50&wt=json",
            query
        );
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let docs = response["response"]["docs"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing docs".into()))?;

        let mut extra = HashMap::new();
        extra.insert(
            "source_repo".to_string(),
            serde_json::Value::String("central".to_string()),
        );

        Ok(docs
            .iter()
            .filter_map(|doc| {
                Some(PackageMeta {
                    name: format!(
                        "{}:{}",
                        doc["g"].as_str().unwrap_or(""),
                        doc["a"].as_str().unwrap_or("")
                    ),
                    version: doc["latestVersion"]
                        .as_str()
                        .or_else(|| doc["v"].as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    description: None,
                    homepage: Some(format!(
                        "https://mvnrepository.com/artifact/{}/{}",
                        doc["g"].as_str().unwrap_or(""),
                        doc["a"].as_str().unwrap_or("")
                    )),
                    repository: None,
                    license: None,
                    binaries: Vec::new(),
                    extra: extra.clone(),
                    keywords: Vec::new(),
                    maintainers: Vec::new(),
                    published: None,
                    downloads: None,
                    archive_url: None,
                    checksum: None,
                })
            })
            .collect())
    }
}

impl PackageIndex for Maven {
    fn ecosystem(&self) -> &'static str {
        "maven"
    }

    fn display_name(&self) -> &'static str {
        "Maven (Java)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Maven uses groupId:artifactId format
        let (group_id, artifact_id) = if let Some((g, a)) = name.split_once(':') {
            (g, a)
        } else {
            // Assume it's just the artifactId, search for it
            return self
                .search(name)?
                .into_iter()
                .next()
                .ok_or_else(|| IndexError::NotFound(name.to_string()));
        };

        // If Central is in our repos, try the search API first (better metadata)
        if self.repos.contains(&MavenRepo::Central) {
            if let Ok((pkg, _)) = Self::fetch_from_central_api(group_id, artifact_id) {
                return Ok(pkg);
            }
        }

        // Fall back to checking each repository directly
        for &repo in &self.repos {
            if let Ok((version, repo)) = Self::check_repo_exists(group_id, artifact_id, repo) {
                let mut extra = HashMap::new();
                extra.insert(
                    "source_repo".to_string(),
                    serde_json::Value::String(repo.name().to_string()),
                );

                return Ok(PackageMeta {
                    name: format!("{}:{}", group_id, artifact_id),
                    version,
                    description: None,
                    homepage: Some(format!(
                        "https://mvnrepository.com/artifact/{}/{}",
                        group_id, artifact_id
                    )),
                    repository: None,
                    license: None,
                    binaries: Vec::new(),
                    extra,
                    keywords: Vec::new(),
                    maintainers: Vec::new(),
                    published: None,
                    downloads: None,
                    archive_url: None,
                    checksum: None,
                });
            }
        }

        Err(IndexError::NotFound(name.to_string()))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let (group_id, artifact_id) = if let Some((g, a)) = name.split_once(':') {
            (g, a)
        } else {
            return Err(IndexError::Parse(
                "Maven package name must be groupId:artifactId".into(),
            ));
        };

        let mut all_versions: Vec<(String, MavenRepo)> = Vec::new();

        // Try Maven Central's search API first (has better metadata)
        if self.repos.contains(&MavenRepo::Central) {
            let url = format!(
                "https://search.maven.org/solrsearch/select?q=g:{}+AND+a:{}&core=gav&rows=100&wt=json",
                group_id, artifact_id
            );
            if let Ok(response) = ureq::get(&url).call() {
                if let Ok(json) = response.into_json::<serde_json::Value>() {
                    if let Some(docs) = json["response"]["docs"].as_array() {
                        for doc in docs {
                            if let Some(v) = doc["v"].as_str() {
                                all_versions.push((v.to_string(), MavenRepo::Central));
                            }
                        }
                    }
                }
            }
        }

        // Try other repos by fetching maven-metadata.xml
        for &repo in &self.repos {
            if repo == MavenRepo::Central && !all_versions.is_empty() {
                continue; // Already got Central versions
            }

            let group_path = group_id.replace('.', "/");
            let metadata_url = format!(
                "{}/{}/{}/maven-metadata.xml",
                repo.base_url(),
                group_path,
                artifact_id
            );

            if let Ok(response) = ureq::get(&metadata_url).call() {
                if let Ok(body) = response.into_string() {
                    for version in extract_versions_from_metadata(&body) {
                        if !all_versions.iter().any(|(v, _)| v == &version) {
                            all_versions.push((version, repo));
                        }
                    }
                }
            }
        }

        if all_versions.is_empty() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        Ok(all_versions
            .into_iter()
            .map(|(version, repo)| VersionMeta {
                version: format!("{} ({})", version, repo.name()),
                released: None,
                yanked: false,
            })
            .collect())
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        // Only Maven Central has a search API
        if self.repos.contains(&MavenRepo::Central) {
            return Self::search_central(query);
        }

        // Other repos don't support search
        Err(IndexError::Parse(
            "Search only available for Maven Central".into(),
        ))
    }
}

/// Extract the latest version from maven-metadata.xml content.
fn extract_version_from_metadata(xml: &str) -> Option<String> {
    // Look for <latest> or <release> tags
    for tag in ["<latest>", "<release>"] {
        if let Some(start) = xml.find(tag) {
            let start = start + tag.len();
            let end_tag = tag.replace('<', "</");
            if let Some(end) = xml[start..].find(&end_tag) {
                return Some(xml[start..start + end].to_string());
            }
        }
    }
    None
}

/// Extract all versions from maven-metadata.xml content.
fn extract_versions_from_metadata(xml: &str) -> Vec<String> {
    let mut versions = Vec::new();
    let mut search_start = 0;

    while let Some(pos) = xml[search_start..].find("<version>") {
        let start = search_start + pos + "<version>".len();
        if let Some(end) = xml[start..].find("</version>") {
            versions.push(xml[start..start + end].to_string());
            search_start = start + end;
        } else {
            break;
        }
    }

    versions
}
