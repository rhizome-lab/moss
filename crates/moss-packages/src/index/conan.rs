//! Conan package index fetcher (C/C++).
//!
//! Fetches package metadata from ConanCenter using the conan.io search API.

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use crate::cache;
use std::time::Duration;

/// Cache TTL for Conan package list (1 hour).
const CACHE_TTL: Duration = Duration::from_secs(60 * 60);

/// Conan package index fetcher.
pub struct Conan;

impl Conan {
    /// Conan.io search API base URL.
    const API_BASE: &'static str = "https://conan.io/api/search";

    /// Parse a package from API response.
    fn parse_package(name: &str, info: &serde_json::Value) -> Option<PackageMeta> {
        let version = info["version"].as_str().unwrap_or("unknown");
        let description = info["description"].as_str().map(String::from);

        // Extract first license from licenses object
        let license = info["licenses"]
            .as_object()
            .and_then(|obj| obj.keys().next())
            .map(String::from);

        Some(PackageMeta {
            name: name.to_string(),
            version: version.to_string(),
            description,
            homepage: Some(format!("https://conan.io/center/recipes/{}", name)),
            repository: None,
            license,
            binaries: Vec::new(),
            keywords: Vec::new(),
            maintainers: Vec::new(),
            published: None,
            downloads: None,
            archive_url: None,
            checksum: None,
            extra: Default::default(),
        })
    }

    /// Load all packages from API (cached).
    fn load_all_packages() -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!("{}{}?topics=&licenses=", Self::API_BASE, "/all");
        let (data, _was_cached) = cache::fetch_with_cache("conan", "all-packages", &url, CACHE_TTL)
            .map_err(IndexError::Network)?;

        let response: serde_json::Value =
            serde_json::from_slice(&data).map_err(|e| IndexError::Parse(e.to_string()))?;

        // Response is an object with numeric keys: {"0": {...}, "1": {...}, ...}
        let packages: Vec<PackageMeta> = response
            .as_object()
            .map(|obj| {
                obj.values()
                    .filter_map(|pkg| {
                        let name = pkg["name"].as_str()?;
                        let info = &pkg["info"];
                        Self::parse_package(name, info)
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(packages)
    }
}

impl PackageIndex for Conan {
    fn ecosystem(&self) -> &'static str {
        "conan"
    }

    fn display_name(&self) -> &'static str {
        "Conan (C/C++)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Search for exact package name
        let url = format!("{}/{}?topics=&licenses=", Self::API_BASE, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        // Response is object with numeric keys, find exact match
        if let Some(obj) = response.as_object() {
            for pkg in obj.values() {
                if let Some(pkg_name) = pkg["name"].as_str() {
                    if pkg_name.eq_ignore_ascii_case(name) {
                        if let Some(meta) = Self::parse_package(pkg_name, &pkg["info"]) {
                            return Ok(meta);
                        }
                    }
                }
            }
        }

        Err(IndexError::NotFound(name.to_string()))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // The search API only returns latest version
        // For now, return just that
        let pkg = self.fetch(name)?;
        Ok(vec![VersionMeta {
            version: pkg.version,
            released: None,
            yanked: false,
        }])
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!("{}/{}?topics=&licenses=", Self::API_BASE, query);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let packages: Vec<PackageMeta> = response
            .as_object()
            .map(|obj| {
                obj.values()
                    .filter_map(|pkg| {
                        let name = pkg["name"].as_str()?;
                        Self::parse_package(name, &pkg["info"])
                    })
                    .take(50)
                    .collect()
            })
            .unwrap_or_default();

        Ok(packages)
    }

    fn fetch_all(&self) -> Result<Vec<PackageMeta>, IndexError> {
        Self::load_all_packages()
    }
}
