//! F-Droid package index fetcher (Android FOSS).
//!
//! Fetches package metadata from F-Droid repositories.
//!
//! ## API Strategy
//! - **fetch**: `f-droid.org/api/v1/packages/{name}` - Official F-Droid JSON API
//! - **fetch_versions**: Same API, extracts packages array
//! - **search**: `search.f-droid.org/api/v1/?q=` - F-Droid search API
//! - **fetch_all**: `f-droid.org/api/v1/packages` (all packages)
//!
//! ## Multi-repo Support
//! ```rust,ignore
//! use moss_packages::index::fdroid::{FDroid, FDroidRepo};
//!
//! // All repos (default)
//! let all = FDroid::all();
//!
//! // Main repo only
//! let main = FDroid::main();
//!
//! // Privacy-focused repos
//! let privacy = FDroid::privacy();
//! ```

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use std::collections::HashMap;

/// Available F-Droid repositories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FDroidRepo {
    /// Main F-Droid repository
    Main,
    /// F-Droid Archive - older app versions
    Archive,
    /// IzzyOnDroid - third-party repo with additional apps
    IzzyOnDroid,
    /// Guardian Project - privacy/security focused apps
    Guardian,
}

impl FDroidRepo {
    /// Get the repository name for tagging.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Main => "fdroid",
            Self::Archive => "fdroid-archive",
            Self::IzzyOnDroid => "izzyondroid",
            Self::Guardian => "guardian",
        }
    }

    /// All available repositories.
    pub fn all() -> &'static [FDroidRepo] {
        &[Self::Main, Self::Archive, Self::IzzyOnDroid, Self::Guardian]
    }

    /// Main F-Droid repo only.
    pub fn main() -> &'static [FDroidRepo] {
        &[Self::Main]
    }

    /// Main + Archive repos.
    pub fn official() -> &'static [FDroidRepo] {
        &[Self::Main, Self::Archive]
    }

    /// Privacy-focused repos (Main + Guardian).
    pub fn privacy() -> &'static [FDroidRepo] {
        &[Self::Main, Self::Guardian]
    }

    /// Extended repos (Main + IzzyOnDroid).
    pub fn extended() -> &'static [FDroidRepo] {
        &[Self::Main, Self::IzzyOnDroid]
    }
}

/// F-Droid package index fetcher with configurable repositories.
pub struct FDroid {
    repos: Vec<FDroidRepo>,
}

impl FDroid {
    /// F-Droid search API.
    const SEARCH_API: &'static str = "https://search.f-droid.org/api";

    /// Create a fetcher with all repositories.
    pub fn all() -> Self {
        Self {
            repos: FDroidRepo::all().to_vec(),
        }
    }

    /// Create a fetcher with main repo only.
    pub fn main() -> Self {
        Self {
            repos: FDroidRepo::main().to_vec(),
        }
    }

    /// Create a fetcher with official repos (main + archive).
    pub fn official() -> Self {
        Self {
            repos: FDroidRepo::official().to_vec(),
        }
    }

    /// Create a fetcher with privacy-focused repos.
    pub fn privacy() -> Self {
        Self {
            repos: FDroidRepo::privacy().to_vec(),
        }
    }

    /// Create a fetcher with extended repos (main + IzzyOnDroid).
    pub fn extended() -> Self {
        Self {
            repos: FDroidRepo::extended().to_vec(),
        }
    }

    /// Create a fetcher with custom repository selection.
    pub fn with_repos(repos: &[FDroidRepo]) -> Self {
        Self {
            repos: repos.to_vec(),
        }
    }

    /// Fetch from the main F-Droid API.
    fn fetch_from_api(name: &str) -> Result<(PackageMeta, FDroidRepo), IndexError> {
        let url = format!("https://f-droid.org/api/v1/packages/{}", name);
        let response: serde_json::Value = ureq::get(&url)
            .call()
            .map_err(|_| IndexError::NotFound(name.to_string()))?
            .into_json()?;

        let packages = response["packages"]
            .as_array()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        let suggested_code = response["suggestedVersionCode"].as_u64();
        let latest = packages
            .iter()
            .find(|p| p["versionCode"].as_u64() == suggested_code)
            .or_else(|| packages.first())
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        let mut extra = HashMap::new();
        extra.insert(
            "source_repo".to_string(),
            serde_json::Value::String("fdroid".to_string()),
        );

        Ok((
            PackageMeta {
                name: response["packageName"].as_str().unwrap_or(name).to_string(),
                version: latest["versionName"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string(),
                description: None,
                homepage: Some(format!("https://f-droid.org/packages/{}", name)),
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
            FDroidRepo::Main,
        ))
    }

    /// Fetch versions from the main F-Droid API.
    fn fetch_versions_from_api(name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let url = format!("https://f-droid.org/api/v1/packages/{}", name);
        let response: serde_json::Value = ureq::get(&url)
            .call()
            .map_err(|_| IndexError::NotFound(name.to_string()))?
            .into_json()?;

        let packages = response["packages"]
            .as_array()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        Ok(packages
            .iter()
            .filter_map(|p| {
                Some(VersionMeta {
                    version: format!("{} (fdroid)", p["versionName"].as_str()?),
                    released: None,
                    yanked: false,
                })
            })
            .collect())
    }

    /// Search using the F-Droid search API.
    fn search_api(query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!("{}/search_apps?q={}", Self::SEARCH_API, query);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let apps = response["apps"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("Invalid search response".into()))?;

        let mut extra = HashMap::new();
        extra.insert(
            "source_repo".to_string(),
            serde_json::Value::String("fdroid".to_string()),
        );

        Ok(apps
            .iter()
            .filter_map(|app| {
                let url = app["url"].as_str()?;
                let package_name = url.rsplit('/').next()?;

                Some(PackageMeta {
                    name: package_name.to_string(),
                    version: "latest".to_string(),
                    description: app["summary"].as_str().map(String::from),
                    homepage: Some(url.to_string()),
                    repository: None,
                    license: None,
                    binaries: Vec::new(),
                    keywords: Vec::new(),
                    maintainers: Vec::new(),
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

impl PackageIndex for FDroid {
    fn ecosystem(&self) -> &'static str {
        "fdroid"
    }

    fn display_name(&self) -> &'static str {
        "F-Droid (Android FOSS)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Try main API first if Main repo is configured
        if self.repos.contains(&FDroidRepo::Main) {
            if let Ok((pkg, _)) = Self::fetch_from_api(name) {
                return Ok(pkg);
            }
        }

        // For other repos, we'd need to parse their index files
        // For now, return not found if main API fails
        Err(IndexError::NotFound(name.to_string()))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let mut all_versions = Vec::new();

        // Try main API if configured
        if self.repos.contains(&FDroidRepo::Main) {
            if let Ok(versions) = Self::fetch_versions_from_api(name) {
                all_versions.extend(versions);
            }
        }

        if all_versions.is_empty() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        Ok(all_versions)
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        // F-Droid search API searches across main repo
        if self.repos.contains(&FDroidRepo::Main) {
            return Self::search_api(query);
        }

        // Other repos don't have search APIs
        Err(IndexError::Parse(
            "Search only available for main F-Droid repo".into(),
        ))
    }
}
