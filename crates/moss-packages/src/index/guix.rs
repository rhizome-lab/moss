//! GNU Guix package index fetcher.
//!
//! Fetches package metadata from guix.gnu.org/packages.json.
//! The server returns gzip-compressed JSON which ureq decompresses automatically.

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use crate::cache;
use std::time::Duration;

/// Cache TTL for the Guix package list (6 hours - it's a 25MB download).
const INDEX_CACHE_TTL: Duration = Duration::from_secs(6 * 60 * 60);

/// GNU Guix package index fetcher.
pub struct Guix;

impl Guix {
    /// Guix packages JSON endpoint.
    const PACKAGES_URL: &'static str = "https://guix.gnu.org/packages.json";

    /// Fetch the full package list with caching.
    fn fetch_package_list() -> Result<Vec<serde_json::Value>, IndexError> {
        // Try cache first
        let (data, _was_cached) =
            cache::fetch_with_cache("guix", "packages-all", Self::PACKAGES_URL, INDEX_CACHE_TTL)
                .map_err(|e| IndexError::Network(e))?;

        let packages: Vec<serde_json::Value> = serde_json::from_slice(&data)?;
        Ok(packages)
    }
}

impl PackageIndex for Guix {
    fn ecosystem(&self) -> &'static str {
        "guix"
    }

    fn display_name(&self) -> &'static str {
        "GNU Guix"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Guix has no per-package API, so we fetch the full list and filter
        let packages = Self::fetch_package_list()?;

        // Find the package by name (may have multiple versions, take first/latest)
        let pkg = packages
            .iter()
            .find(|p| p["name"].as_str() == Some(name))
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        Ok(package_to_meta(pkg, name))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let packages = Self::fetch_package_list()?;

        // Find all versions of the package
        let versions: Vec<VersionMeta> = packages
            .iter()
            .filter(|p| p["name"].as_str() == Some(name))
            .filter_map(|p| {
                Some(VersionMeta {
                    version: p["version"].as_str()?.to_string(),
                    released: None,
                    yanked: false,
                })
            })
            .collect();

        if versions.is_empty() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        Ok(versions)
    }

    fn supports_fetch_all(&self) -> bool {
        true
    }

    fn fetch_all(&self) -> Result<Vec<PackageMeta>, IndexError> {
        let packages = Self::fetch_package_list()?;
        Ok(packages.iter().map(|p| package_to_meta(p, "")).collect())
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let packages = Self::fetch_package_list()?;
        let query_lower = query.to_lowercase();

        Ok(packages
            .iter()
            .filter(|pkg| {
                let name = pkg["name"].as_str().unwrap_or("");
                let synopsis = pkg["synopsis"].as_str().unwrap_or("");
                name.to_lowercase().contains(&query_lower)
                    || synopsis.to_lowercase().contains(&query_lower)
            })
            .take(50)
            .map(|p| package_to_meta(p, ""))
            .collect())
    }
}

/// Convert a Guix package JSON object to PackageMeta.
fn package_to_meta(pkg: &serde_json::Value, fallback_name: &str) -> PackageMeta {
    PackageMeta {
        name: pkg["name"].as_str().unwrap_or(fallback_name).to_string(),
        version: pkg["version"].as_str().unwrap_or("unknown").to_string(),
        description: pkg["synopsis"].as_str().map(String::from),
        homepage: pkg["homepage"].as_str().map(String::from),
        repository: extract_repo(pkg),
        license: None, // Guix packages.json doesn't include license info
        binaries: Vec::new(),
        ..Default::default()
    }
}

/// Extract repository URL from homepage if it's a known forge.
fn extract_repo(pkg: &serde_json::Value) -> Option<String> {
    let homepage = pkg["homepage"].as_str()?;
    if homepage.contains("github.com")
        || homepage.contains("gitlab.com")
        || homepage.contains("sr.ht")
        || homepage.contains("codeberg.org")
    {
        Some(homepage.to_string())
    } else {
        None
    }
}
