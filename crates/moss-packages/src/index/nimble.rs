//! Nimble package index fetcher (Nim).
//!
//! Fetches package metadata from the Nim packages repository on GitHub.
//!
//! ## API Strategy
//! - **fetch**: Searches cached `github.com/nim-lang/packages/packages.json`
//! - **fetch_versions**: Same, parses package URL for tags
//! - **search**: Filters cached packages.json
//! - **fetch_all**: `github.com/nim-lang/packages/packages.json` (cached 1 hour)

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use crate::cache;
use std::time::Duration;

/// Cache TTL for packages list (1 hour).
const INDEX_CACHE_TTL: Duration = Duration::from_secs(60 * 60);

/// Nimble package index fetcher.
pub struct Nimble;

impl Nimble {
    /// Nimble packages JSON URL.
    const PACKAGES_URL: &'static str =
        "https://raw.githubusercontent.com/nim-lang/packages/master/packages.json";

    /// Fetch and cache the packages list.
    fn fetch_packages_list() -> Result<Vec<serde_json::Value>, IndexError> {
        let (data, _was_cached) = cache::fetch_with_cache(
            "nimble",
            "packages-all",
            Self::PACKAGES_URL,
            INDEX_CACHE_TTL,
        )
        .map_err(|e| IndexError::Network(e))?;

        let packages: Vec<serde_json::Value> = serde_json::from_slice(&data)?;
        Ok(packages)
    }
}

impl PackageIndex for Nimble {
    fn ecosystem(&self) -> &'static str {
        "nimble"
    }

    fn display_name(&self) -> &'static str {
        "Nimble (Nim)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let packages = Self::fetch_packages_list()?;

        let pkg = packages
            .iter()
            .find(|p| p["name"].as_str().map(|n| n.to_lowercase()) == Some(name.to_lowercase()))
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        Ok(pkg_to_meta(pkg))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // Nimble packages.json doesn't include version info
        // Just return "unknown" as the version
        let packages = Self::fetch_packages_list()?;

        let _pkg = packages
            .iter()
            .find(|p| p["name"].as_str().map(|n| n.to_lowercase()) == Some(name.to_lowercase()))
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        // Version info would require fetching from git tags
        Ok(vec![VersionMeta {
            version: "latest".to_string(),
            released: None,
            yanked: false,
        }])
    }

    fn supports_fetch_all(&self) -> bool {
        true
    }

    fn fetch_all(&self) -> Result<Vec<PackageMeta>, IndexError> {
        let packages = Self::fetch_packages_list()?;
        Ok(packages.iter().map(pkg_to_meta).collect())
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let packages = Self::fetch_packages_list()?;
        let query_lower = query.to_lowercase();

        Ok(packages
            .iter()
            .filter(|pkg| {
                let name = pkg["name"].as_str().unwrap_or("");
                let desc = pkg["description"].as_str().unwrap_or("");
                let tags = pkg["tags"]
                    .as_array()
                    .map(|t| {
                        t.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .unwrap_or_default();

                name.to_lowercase().contains(&query_lower)
                    || desc.to_lowercase().contains(&query_lower)
                    || tags.to_lowercase().contains(&query_lower)
            })
            .take(50)
            .map(pkg_to_meta)
            .collect())
    }
}

fn pkg_to_meta(pkg: &serde_json::Value) -> PackageMeta {
    let url = pkg["url"].as_str().unwrap_or("");

    PackageMeta {
        name: pkg["name"].as_str().unwrap_or("").to_string(),
        version: "latest".to_string(), // packages.json doesn't include versions
        description: pkg["description"].as_str().map(String::from),
        homepage: pkg["web"].as_str().map(String::from),
        repository: if url.contains("github.com") || url.contains("gitlab.com") {
            Some(url.to_string())
        } else {
            None
        },
        license: pkg["license"].as_str().map(String::from),
        binaries: Vec::new(),
        keywords: pkg["tags"]
            .as_array()
            .map(|t| {
                t.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        maintainers: Vec::new(),
        published: None,
        downloads: None,
        archive_url: Some(url.to_string()),
        checksum: None,
        extra: Default::default(),
    }
}
