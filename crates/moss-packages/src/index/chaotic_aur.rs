//! Chaotic-AUR package index fetcher.
//!
//! Fetches package metadata from Chaotic-AUR, a repository of pre-built
//! AUR packages. Faster than building from AUR source.
//!
//! Uses the Chaotic builder backend JSON API.

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use crate::cache;
use std::time::Duration;

/// Cache TTL for Chaotic-AUR package list (1 hour).
const CACHE_TTL: Duration = Duration::from_secs(60 * 60);

/// Chaotic-AUR package index fetcher.
pub struct ChaoticAur;

impl ChaoticAur {
    /// Chaotic-AUR backend API URL.
    const API_URL: &'static str =
        "https://chaotic-backend.garudalinux.org/builder/packages?repo=true";

    /// Parse a package from the API response.
    fn parse_package(pkg: &serde_json::Value) -> Option<PackageMeta> {
        let name = pkg["pkgname"].as_str()?;
        let version = pkg["version"].as_str().unwrap_or("unknown");
        let pkgrel = pkg["pkgrel"].as_u64().unwrap_or(1);

        let metadata = &pkg["metadata"];
        let description = metadata["desc"].as_str().map(String::from);
        let homepage = metadata["url"].as_str().map(String::from);
        let license = metadata["license"].as_str().map(String::from);
        let filename = metadata["filename"].as_str();

        let archive_url = filename.map(|f| {
            format!(
                "https://builds.garudalinux.org/repos/chaotic-aur/x86_64/{}",
                f
            )
        });

        Some(PackageMeta {
            name: name.to_string(),
            version: format!("{}-{}", version, pkgrel),
            description,
            homepage,
            repository: Some("https://aur.chaotic.cx/".to_string()),
            license,
            binaries: Vec::new(),
            keywords: Vec::new(),
            maintainers: Vec::new(),
            published: None,
            downloads: None,
            archive_url,
            checksum: None,
            extra: Default::default(),
        })
    }

    /// Load all packages from the API.
    fn load_packages() -> Result<Vec<PackageMeta>, IndexError> {
        let (data, _was_cached) =
            cache::fetch_with_cache("chaotic-aur", "packages", Self::API_URL, CACHE_TTL)
                .map_err(IndexError::Network)?;

        let packages: Vec<serde_json::Value> =
            serde_json::from_slice(&data).map_err(|e| IndexError::Parse(e.to_string()))?;

        Ok(packages
            .iter()
            .filter(|p| p["isActive"].as_bool().unwrap_or(false))
            .filter_map(Self::parse_package)
            .collect())
    }
}

impl PackageIndex for ChaoticAur {
    fn ecosystem(&self) -> &'static str {
        "chaotic-aur"
    }

    fn display_name(&self) -> &'static str {
        "Chaotic-AUR"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let packages = Self::load_packages()?;

        packages
            .into_iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| IndexError::NotFound(name.to_string()))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // Chaotic-AUR only has the current version
        let pkg = self.fetch(name)?;
        Ok(vec![VersionMeta {
            version: pkg.version,
            released: None,
            yanked: false,
        }])
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let packages = Self::load_packages()?;
        let query_lower = query.to_lowercase();

        Ok(packages
            .into_iter()
            .filter(|p| {
                p.name.to_lowercase().contains(&query_lower)
                    || p.description
                        .as_ref()
                        .map(|d| d.to_lowercase().contains(&query_lower))
                        .unwrap_or(false)
            })
            .take(50)
            .collect())
    }

    fn fetch_all(&self) -> Result<Vec<PackageMeta>, IndexError> {
        Self::load_packages()
    }
}
