//! Winget package index fetcher (Windows Package Manager).
//!
//! Fetches package metadata from the winget-pkgs repository.
//!
//! ## API Strategy
//! - **fetch**: `api.winget.run/v2/packages/{id}` - Community winget.run JSON API
//! - **fetch_versions**: Same API, extracts versions array
//! - **search**: `api.winget.run/v2/packages?query=`
//! - **fetch_all**: `api.winget.run/v2/packages` (all packages)
//!
//! ## Multi-source Support
//! ```rust,ignore
//! use moss_packages::index::winget::{Winget, WingetSource};
//!
//! // All sources (default)
//! let all = Winget::all();
//!
//! // Winget community only
//! let winget = Winget::winget_only();
//! ```

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use std::collections::HashMap;

/// Available WinGet sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WingetSource {
    /// Winget community repository (via winget.run API)
    Winget,
    /// Microsoft Store (not yet supported via API)
    MsStore,
}

impl WingetSource {
    /// Get the API base URL for this source.
    fn api_url(&self) -> Option<&'static str> {
        match self {
            Self::Winget => Some("https://api.winget.run/v2/packages"),
            Self::MsStore => None, // Not supported via public API
        }
    }

    /// Get the source name for tagging.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Winget => "winget",
            Self::MsStore => "msstore",
        }
    }

    /// All available sources.
    pub fn all() -> &'static [WingetSource] {
        &[Self::Winget, Self::MsStore]
    }

    /// Winget only.
    pub fn winget() -> &'static [WingetSource] {
        &[Self::Winget]
    }
}

/// Winget package index fetcher with configurable sources.
pub struct Winget {
    sources: Vec<WingetSource>,
}

impl Winget {
    /// Create a fetcher with all sources.
    pub fn all() -> Self {
        Self {
            sources: WingetSource::all().to_vec(),
        }
    }

    /// Create a fetcher with winget source only.
    pub fn winget_only() -> Self {
        Self {
            sources: WingetSource::winget().to_vec(),
        }
    }

    /// Create a fetcher with custom source selection.
    pub fn with_sources(sources: &[WingetSource]) -> Self {
        Self {
            sources: sources.to_vec(),
        }
    }

    /// Fetch a package from a specific source.
    fn fetch_from_source(name: &str, source: WingetSource) -> Result<PackageMeta, IndexError> {
        let api_url = source.api_url().ok_or_else(|| {
            IndexError::NotImplemented(format!("{} API not available", source.name()))
        })?;

        let url = format!("{}/{}", api_url, name);
        let response: serde_json::Value = ureq::get(&url)
            .set("Accept", "application/json")
            .call()?
            .into_json()?;

        if response.get("error").is_some() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        let latest = response["versions"]
            .as_array()
            .and_then(|v| v.first())
            .unwrap_or(&response);

        let mut extra = HashMap::new();
        extra.insert(
            "source_repo".to_string(),
            serde_json::Value::String(source.name().to_string()),
        );

        Ok(PackageMeta {
            name: response["id"].as_str().unwrap_or(name).to_string(),
            version: latest["version"].as_str().unwrap_or("unknown").to_string(),
            description: response["description"].as_str().map(String::from),
            homepage: response["homepage"].as_str().map(String::from),
            repository: response["repository"].as_str().map(String::from),
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

    /// Fetch versions from a specific source.
    fn fetch_versions_from_source(
        name: &str,
        source: WingetSource,
    ) -> Result<Vec<VersionMeta>, IndexError> {
        let api_url = source.api_url().ok_or_else(|| {
            IndexError::NotImplemented(format!("{} API not available", source.name()))
        })?;

        let url = format!("{}/{}", api_url, name);
        let response: serde_json::Value = ureq::get(&url)
            .set("Accept", "application/json")
            .call()?
            .into_json()?;

        if response.get("error").is_some() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        let versions = response["versions"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing versions".into()))?;

        Ok(versions
            .iter()
            .filter_map(|v| {
                Some(VersionMeta {
                    version: v["version"].as_str()?.to_string(),
                    released: v["date"].as_str().map(String::from),
                    yanked: false,
                })
            })
            .collect())
    }

    /// Search a specific source.
    fn search_source(query: &str, source: WingetSource) -> Result<Vec<PackageMeta>, IndexError> {
        let api_url = source.api_url().ok_or_else(|| {
            IndexError::NotImplemented(format!("{} API not available", source.name()))
        })?;

        let url = format!("{}?q={}", api_url, query);
        let response: serde_json::Value = ureq::get(&url)
            .set("Accept", "application/json")
            .call()?
            .into_json()?;

        let packages = response["packages"]
            .as_array()
            .or_else(|| response.as_array())
            .ok_or_else(|| IndexError::Parse("missing packages".into()))?;

        Ok(packages
            .iter()
            .filter_map(|pkg| {
                let mut extra = HashMap::new();
                extra.insert(
                    "source_repo".to_string(),
                    serde_json::Value::String(source.name().to_string()),
                );

                Some(PackageMeta {
                    name: pkg["id"].as_str()?.to_string(),
                    version: pkg["version"].as_str().unwrap_or("unknown").to_string(),
                    description: pkg["description"].as_str().map(String::from),
                    homepage: pkg["homepage"].as_str().map(String::from),
                    repository: pkg["repository"].as_str().map(String::from),
                    license: pkg["license"].as_str().map(String::from),
                    binaries: Vec::new(),
                    keywords: Vec::new(),
                    maintainers: Vec::new(),
                    published: None,
                    downloads: None,
                    archive_url: None,
                    checksum: None,
                    extra,
                })
            })
            .collect())
    }
}

impl PackageIndex for Winget {
    fn ecosystem(&self) -> &'static str {
        "winget"
    }

    fn display_name(&self) -> &'static str {
        "Winget (Windows)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Try each configured source until we find the package
        for &source in &self.sources {
            match Self::fetch_from_source(name, source) {
                Ok(pkg) => return Ok(pkg),
                Err(IndexError::NotFound(_)) | Err(IndexError::NotImplemented(_)) => continue,
                Err(e) => return Err(e),
            }
        }

        Err(IndexError::NotFound(name.to_string()))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let mut all_versions = Vec::new();

        for &source in &self.sources {
            if let Ok(versions) = Self::fetch_versions_from_source(name, source) {
                all_versions.extend(versions);
            }
        }

        if all_versions.is_empty() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        Ok(all_versions)
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let mut results = Vec::new();

        for &source in &self.sources {
            if let Ok(packages) = Self::search_source(query, source) {
                results.extend(packages);
            }
        }

        Ok(results)
    }
}
