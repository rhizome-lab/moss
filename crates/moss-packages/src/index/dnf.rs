//! DNF package index fetcher (Fedora/RHEL).
//!
//! Fetches package metadata from Fedora repositories.
//!
//! ## API Strategy
//! - **fetch**: `mdapi.fedoraproject.org/{release}/pkg/{name}` - Fedora MDAPI JSON
//! - **fetch_versions**: Queries all configured releases
//! - **search**: `apps.fedoraproject.org/packages/fcomm_connector/xapian/query/search_packages`
//! - **fetch_all**: Queries srcpkg list from all configured releases
//!
//! ## Multi-repo Support
//! ```rust,ignore
//! use moss_packages::index::dnf::{Dnf, DnfRepo};
//!
//! // All repos (default)
//! let all = Dnf::all();
//!
//! // Current stable releases only
//! let stable = Dnf::stable();
//!
//! // Rawhide only
//! let rawhide = Dnf::rawhide();
//!
//! // Custom selection
//! let custom = Dnf::with_repos(&[DnfRepo::Fedora41, DnfRepo::Rawhide]);
//! ```

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use crate::cache;
use rayon::prelude::*;
use std::collections::HashMap;
use std::time::Duration;

/// Cache TTL for package lists (1 hour).
const CACHE_TTL: Duration = Duration::from_secs(60 * 60);

/// Available DNF repositories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DnfRepo {
    // === Fedora Stable ===
    /// Fedora 41 (current)
    Fedora41,
    /// Fedora 40
    Fedora40,
    /// Fedora 39
    Fedora39,

    // === Fedora Development ===
    /// Fedora Rawhide (development)
    Rawhide,

    // === EPEL (Enterprise Linux) ===
    /// EPEL 9 (RHEL 9 / CentOS Stream 9)
    Epel9,
    /// EPEL 8 (RHEL 8 / CentOS Stream 8)
    Epel8,
}

impl DnfRepo {
    /// Get the release name used in mdapi URLs.
    fn release(&self) -> &'static str {
        match self {
            Self::Fedora41 => "f41",
            Self::Fedora40 => "f40",
            Self::Fedora39 => "f39",
            Self::Rawhide => "rawhide",
            Self::Epel9 => "epel9",
            Self::Epel8 => "epel8",
        }
    }

    /// Get the repository name for tagging.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Fedora41 => "fedora-41",
            Self::Fedora40 => "fedora-40",
            Self::Fedora39 => "fedora-39",
            Self::Rawhide => "rawhide",
            Self::Epel9 => "epel-9",
            Self::Epel8 => "epel-8",
        }
    }

    /// All available repositories.
    pub fn all() -> &'static [DnfRepo] {
        &[
            Self::Fedora41,
            Self::Fedora40,
            Self::Fedora39,
            Self::Rawhide,
            Self::Epel9,
            Self::Epel8,
        ]
    }

    /// Current stable Fedora releases.
    pub fn stable() -> &'static [DnfRepo] {
        &[Self::Fedora41, Self::Fedora40]
    }

    /// Rawhide only.
    pub fn rawhide() -> &'static [DnfRepo] {
        &[Self::Rawhide]
    }

    /// EPEL repositories only.
    pub fn epel() -> &'static [DnfRepo] {
        &[Self::Epel9, Self::Epel8]
    }

    /// All Fedora releases (no EPEL).
    pub fn fedora() -> &'static [DnfRepo] {
        &[
            Self::Fedora41,
            Self::Fedora40,
            Self::Fedora39,
            Self::Rawhide,
        ]
    }
}

/// DNF package index fetcher with configurable repositories.
pub struct Dnf {
    repos: Vec<DnfRepo>,
}

impl Dnf {
    /// Fedora packages API (fcomm_connector).
    const FEDORA_API: &'static str = "https://apps.fedoraproject.org/packages/fcomm_connector";

    /// mdapi for package metadata.
    const MDAPI: &'static str = "https://mdapi.fedoraproject.org";

    /// Create a fetcher with all repositories.
    pub fn all() -> Self {
        Self {
            repos: DnfRepo::all().to_vec(),
        }
    }

    /// Create a fetcher with stable Fedora releases.
    pub fn stable() -> Self {
        Self {
            repos: DnfRepo::stable().to_vec(),
        }
    }

    /// Create a fetcher with rawhide only.
    pub fn rawhide() -> Self {
        Self {
            repos: DnfRepo::rawhide().to_vec(),
        }
    }

    /// Create a fetcher with EPEL repositories only.
    pub fn epel() -> Self {
        Self {
            repos: DnfRepo::epel().to_vec(),
        }
    }

    /// Create a fetcher with all Fedora releases (no EPEL).
    pub fn fedora() -> Self {
        Self {
            repos: DnfRepo::fedora().to_vec(),
        }
    }

    /// Create a fetcher with custom repository selection.
    pub fn with_repos(repos: &[DnfRepo]) -> Self {
        Self {
            repos: repos.to_vec(),
        }
    }

    /// Fetch package info from a specific release.
    fn fetch_from_release(
        release: &str,
        name: &str,
        repo: DnfRepo,
    ) -> Result<PackageMeta, IndexError> {
        let url = format!("{}/{}/pkg/{}", Self::MDAPI, release, name);
        let response: serde_json::Value = ureq::get(&url)
            .set("Accept", "application/json")
            .call()?
            .into_json()?;

        if response.get("output").is_some() && response["output"].as_str() == Some("notok") {
            return Err(IndexError::NotFound(name.to_string()));
        }

        let mut extra = HashMap::new();

        // Extract dependencies from requires
        if let Some(requires) = response["requires"].as_array() {
            let deps: Vec<serde_json::Value> = requires
                .iter()
                .filter_map(|r| r["name"].as_str())
                .filter(|name| {
                    // Filter out internal deps
                    !name.contains("()") && !name.starts_with("rtld") && !name.contains(".so")
                })
                .map(|name| serde_json::Value::String(name.to_string()))
                .collect();
            if !deps.is_empty() {
                extra.insert("depends".to_string(), serde_json::Value::Array(deps));
            }
        }

        // Extract provides (virtual packages and shared libraries)
        if let Some(provides) = response["provides"].as_array() {
            let parsed_provides: Vec<serde_json::Value> = provides
                .iter()
                .filter_map(|p| p["name"].as_str())
                .map(|name| serde_json::Value::String(name.to_string()))
                .collect();
            if !parsed_provides.is_empty() {
                extra.insert(
                    "provides".to_string(),
                    serde_json::Value::Array(parsed_provides),
                );
            }
        }

        // Extract arch
        if let Some(arch) = response["arch"].as_str() {
            extra.insert(
                "arch".to_string(),
                serde_json::Value::String(arch.to_string()),
            );
        }

        // Tag with source repo
        extra.insert(
            "source_repo".to_string(),
            serde_json::Value::String(repo.name().to_string()),
        );

        Ok(PackageMeta {
            name: response["name"].as_str().unwrap_or(name).to_string(),
            version: format!(
                "{}-{}",
                response["version"].as_str().unwrap_or("unknown"),
                response["release"].as_str().unwrap_or("1")
            ),
            description: response["summary"].as_str().map(String::from),
            homepage: response["url"].as_str().map(String::from),
            repository: response["url"]
                .as_str()
                .filter(|u| u.contains("github.com") || u.contains("gitlab.com"))
                .map(String::from),
            license: response["license"].as_str().map(String::from),
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

    /// Load source package list from a release.
    fn load_repo(repo: DnfRepo) -> Result<Vec<PackageMeta>, IndexError> {
        let release = repo.release();
        let url = format!("{}/{}/srcpkgs", Self::MDAPI, release);

        let (data, _was_cached) =
            cache::fetch_with_cache("dnf", &format!("srcpkgs-{}", repo.name()), &url, CACHE_TTL)
                .map_err(IndexError::Network)?;

        let names: Vec<String> =
            serde_json::from_slice(&data).map_err(|e| IndexError::Parse(e.to_string()))?;

        // For fetch_all, we just return basic package info
        // Getting full details for each would be too slow
        Ok(names
            .into_iter()
            .map(|name| {
                let mut extra = HashMap::new();
                extra.insert(
                    "source_repo".to_string(),
                    serde_json::Value::String(repo.name().to_string()),
                );

                PackageMeta {
                    name,
                    version: String::new(), // Would need individual API call
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
                }
            })
            .collect())
    }

    /// Load packages from all configured repositories in parallel.
    fn load_packages(&self) -> Result<Vec<PackageMeta>, IndexError> {
        let results: Vec<_> = self
            .repos
            .par_iter()
            .map(|&repo| Self::load_repo(repo))
            .collect();

        let mut packages = Vec::new();
        for result in results {
            match result {
                Ok(pkgs) => packages.extend(pkgs),
                Err(e) => {
                    eprintln!("Warning: failed to load DNF repo: {}", e);
                }
            }
        }

        Ok(packages)
    }
}

impl PackageIndex for Dnf {
    fn ecosystem(&self) -> &'static str {
        "dnf"
    }

    fn display_name(&self) -> &'static str {
        "DNF (Fedora/RHEL)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Try each configured repo until we find the package
        for repo in &self.repos {
            match Self::fetch_from_release(repo.release(), name, *repo) {
                Ok(pkg) => return Ok(pkg),
                Err(IndexError::NotFound(_)) => continue,
                Err(e) => return Err(e),
            }
        }

        Err(IndexError::NotFound(name.to_string()))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // Query all configured releases in parallel
        let results: Vec<_> = self
            .repos
            .par_iter()
            .filter_map(|repo| Self::fetch_from_release(repo.release(), name, *repo).ok())
            .collect();

        if results.is_empty() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        Ok(results
            .into_iter()
            .map(|pkg| VersionMeta {
                version: pkg.version,
                released: None,
                yanked: false,
            })
            .collect())
    }

    fn supports_fetch_all(&self) -> bool {
        true
    }

    fn fetch_all(&self) -> Result<Vec<PackageMeta>, IndexError> {
        self.load_packages()
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        // Use the fcomm_connector API with JSON in URL
        let search_json = serde_json::json!({
            "filters": {"search": query},
            "rows_per_page": 50,
            "start_row": 0
        });

        let url = format!(
            "{}/xapian/query/search_packages/{}",
            Self::FEDORA_API,
            urlencoding::encode(&search_json.to_string())
        );

        let response: serde_json::Value = ureq::get(&url)
            .set("Accept", "application/json")
            .call()?
            .into_json()?;

        let rows = response["rows"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing rows".into()))?;

        Ok(rows
            .iter()
            .filter_map(|pkg| {
                Some(PackageMeta {
                    name: pkg["name"].as_str()?.to_string(),
                    version: pkg["version"].as_str().unwrap_or("unknown").to_string(),
                    description: pkg["summary"].as_str().map(String::from),
                    homepage: pkg["upstream_url"].as_str().map(String::from),
                    repository: None,
                    license: pkg["license"].as_str().map(String::from),
                    binaries: Vec::new(),
                    keywords: Vec::new(),
                    maintainers: Vec::new(),
                    published: None,
                    downloads: None,
                    archive_url: None,
                    checksum: None,
                    extra: Default::default(),
                })
            })
            .collect())
    }
}

mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut result = String::with_capacity(s.len() * 3);
        for c in s.chars() {
            match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
                _ => {
                    for b in c.to_string().bytes() {
                        result.push_str(&format!("%{:02X}", b));
                    }
                }
            }
        }
        result
    }
}
