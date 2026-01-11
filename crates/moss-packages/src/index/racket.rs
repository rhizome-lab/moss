//! Racket package index fetcher.
//!
//! Fetches package metadata from pkgs.racket-lang.org.
//! Uses the pkgs-all.json.gz endpoint which contains all package data.
//!
//! ## API Strategy
//! - **fetch**: Searches cached `pkgs.racket-lang.org/pkgs-all.json.gz`
//! - **fetch_versions**: Same, single version per package
//! - **search**: Filters cached pkgs-all.json
//! - **fetch_all**: `pkgs.racket-lang.org/pkgs-all.json.gz` (cached 1 hour)

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use crate::cache;
use std::collections::HashMap;
use std::time::Duration;

/// Cache TTL for Racket package index (1 hour).
const CACHE_TTL: Duration = Duration::from_secs(60 * 60);

/// Racket package index fetcher.
pub struct Racket;

impl Racket {
    /// Racket packages JSON endpoint.
    const PACKAGES_URL: &'static str = "https://pkgs.racket-lang.org/pkgs-all.json.gz";

    /// Parse a package from Racket JSON format.
    fn parse_package(name: &str, pkg: &serde_json::Value) -> Option<PackageMeta> {
        let mut extra = HashMap::new();

        // Extract dependencies
        if let Some(deps) = pkg["dependencies"].as_array() {
            let dep_names: Vec<serde_json::Value> = deps
                .iter()
                .filter_map(|d| {
                    // Dependencies can be strings or arrays like ["base", {"kw": "version"}, "7.6"]
                    if let Some(s) = d.as_str() {
                        Some(serde_json::Value::String(s.to_string()))
                    } else if let Some(arr) = d.as_array() {
                        arr.first()
                            .and_then(|f| f.as_str())
                            .map(|s| serde_json::Value::String(s.to_string()))
                    } else {
                        None
                    }
                })
                .collect();
            if !dep_names.is_empty() {
                extra.insert("depends".to_string(), serde_json::Value::Array(dep_names));
            }
        }

        // Extract tags as keywords
        if let Some(tags) = pkg["tags"].as_array() {
            let tag_list: Vec<serde_json::Value> = tags
                .iter()
                .filter_map(|t| t.as_str().map(|s| serde_json::Value::String(s.to_string())))
                .collect();
            if !tag_list.is_empty() {
                extra.insert("keywords".to_string(), serde_json::Value::Array(tag_list));
            }
        }

        // Extract ring (quality tier)
        if let Some(ring) = pkg["ring"].as_u64() {
            extra.insert("ring".to_string(), serde_json::Value::Number(ring.into()));
        }

        // Get source URL
        let source_url = pkg["source"].as_str().map(String::from);

        // Get checksum
        let checksum = pkg["checksum"].as_str().map(|c| format!("sha1:{}", c));

        // Get authors/maintainers
        let maintainers: Vec<String> = pkg["authors"]
            .as_array()
            .map(|authors| {
                authors
                    .iter()
                    .filter_map(|a| a.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        // Get version (from default version or use checksum as pseudo-version)
        let version = pkg["versions"]["default"]["checksum"]
            .as_str()
            .map(|c| c[..8].to_string()) // Use first 8 chars of checksum as version
            .unwrap_or_else(|| "latest".to_string());

        Some(PackageMeta {
            name: name.to_string(),
            version,
            description: pkg["description"].as_str().map(String::from),
            homepage: Some(format!("https://pkgs.racket-lang.org/package/{}", name)),
            repository: source_url.clone(),
            license: pkg["license"].as_str().map(String::from),
            binaries: Vec::new(),
            archive_url: source_url,
            keywords: Vec::new(),
            maintainers,
            published: None,
            downloads: None,
            checksum,
            extra,
        })
    }

    /// Load all packages from the index.
    fn load_all_packages() -> Result<serde_json::Value, IndexError> {
        let (data, _was_cached) =
            cache::fetch_with_cache("racket", "pkgs-all", Self::PACKAGES_URL, CACHE_TTL)
                .map_err(IndexError::Network)?;

        serde_json::from_slice(&data).map_err(|e| IndexError::Parse(e.to_string()))
    }
}

impl PackageIndex for Racket {
    fn ecosystem(&self) -> &'static str {
        "racket"
    }

    fn display_name(&self) -> &'static str {
        "Racket"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let packages = Self::load_all_packages()?;

        packages
            .get(name)
            .and_then(|pkg| Self::parse_package(name, pkg))
            .ok_or_else(|| IndexError::NotFound(name.to_string()))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let packages = Self::load_all_packages()?;

        let pkg = packages
            .get(name)
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        // Racket packages typically only have one "version" (the current state)
        // But the versions field can contain multiple
        let mut versions = Vec::new();

        if let Some(vers) = pkg["versions"].as_object() {
            for (ver_name, ver_data) in vers {
                if let Some(checksum) = ver_data["checksum"].as_str() {
                    versions.push(VersionMeta {
                        version: if ver_name == "default" {
                            checksum[..8].to_string()
                        } else {
                            ver_name.clone()
                        },
                        released: None,
                        yanked: false,
                    });
                }
            }
        }

        if versions.is_empty() {
            let pkg_meta = Self::parse_package(name, pkg)
                .ok_or_else(|| IndexError::NotFound(name.to_string()))?;
            versions.push(VersionMeta {
                version: pkg_meta.version,
                released: None,
                yanked: false,
            });
        }

        Ok(versions)
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let packages = Self::load_all_packages()?;
        let query_lower = query.to_lowercase();

        let results: Vec<PackageMeta> = packages
            .as_object()
            .ok_or_else(|| IndexError::Parse("expected object".into()))?
            .iter()
            .filter(|(name, pkg)| {
                // Match on name
                name.to_lowercase().contains(&query_lower)
                    // Or description
                    || pkg["description"]
                        .as_str()
                        .map(|d| d.to_lowercase().contains(&query_lower))
                        .unwrap_or(false)
                    // Or tags
                    || pkg["tags"]
                        .as_array()
                        .map(|tags| {
                            tags.iter()
                                .any(|t| t.as_str().map(|s| s.contains(&query_lower)).unwrap_or(false))
                        })
                        .unwrap_or(false)
            })
            .take(50)
            .filter_map(|(name, pkg)| Self::parse_package(name, pkg))
            .collect();

        Ok(results)
    }

    fn fetch_all(&self) -> Result<Vec<PackageMeta>, IndexError> {
        let packages = Self::load_all_packages()?;

        let results: Vec<PackageMeta> = packages
            .as_object()
            .ok_or_else(|| IndexError::Parse("expected object".into()))?
            .iter()
            .filter_map(|(name, pkg)| Self::parse_package(name, pkg))
            .collect();

        Ok(results)
    }
}
