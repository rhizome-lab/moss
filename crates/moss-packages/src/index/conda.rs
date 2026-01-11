//! Conda package index fetcher (conda-forge).
//!
//! Fetches package metadata from conda-forge repodata.

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use crate::cache;
use std::collections::HashMap;
use std::time::Duration;

/// Cache TTL for repodata (1 hour).
const INDEX_CACHE_TTL: Duration = Duration::from_secs(60 * 60);

/// Conda package index fetcher.
pub struct Conda;

impl Conda {
    /// conda-forge repodata URL template.
    const REPODATA_URL: &'static str =
        "https://conda.anaconda.org/conda-forge/linux-64/repodata.json";

    /// Fetch and cache the repodata.
    fn fetch_repodata() -> Result<serde_json::Value, IndexError> {
        let (data, _was_cached) = cache::fetch_with_cache(
            "conda",
            "repodata-linux64",
            Self::REPODATA_URL,
            INDEX_CACHE_TTL,
        )
        .map_err(|e| IndexError::Network(e))?;

        let repodata: serde_json::Value = serde_json::from_slice(&data)?;
        Ok(repodata)
    }

    /// Extract unique packages with their latest versions.
    fn extract_packages(
        repodata: &serde_json::Value,
    ) -> HashMap<String, (String, serde_json::Value)> {
        let mut packages: HashMap<String, (String, serde_json::Value)> = HashMap::new();

        if let Some(pkgs) = repodata["packages"].as_object() {
            for (_filename, pkg) in pkgs {
                if let Some(name) = pkg["name"].as_str() {
                    let version = pkg["version"].as_str().unwrap_or("0");
                    let entry = packages
                        .entry(name.to_string())
                        .or_insert_with(|| (version.to_string(), pkg.clone()));
                    // Keep the latest version
                    if version_compare(version, &entry.0) == std::cmp::Ordering::Greater {
                        *entry = (version.to_string(), pkg.clone());
                    }
                }
            }
        }

        // Also check packages.conda (newer format)
        if let Some(pkgs) = repodata["packages.conda"].as_object() {
            for (_filename, pkg) in pkgs {
                if let Some(name) = pkg["name"].as_str() {
                    let version = pkg["version"].as_str().unwrap_or("0");
                    let entry = packages
                        .entry(name.to_string())
                        .or_insert_with(|| (version.to_string(), pkg.clone()));
                    if version_compare(version, &entry.0) == std::cmp::Ordering::Greater {
                        *entry = (version.to_string(), pkg.clone());
                    }
                }
            }
        }

        packages
    }
}

impl PackageIndex for Conda {
    fn ecosystem(&self) -> &'static str {
        "conda"
    }

    fn display_name(&self) -> &'static str {
        "Conda (conda-forge)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let repodata = Self::fetch_repodata()?;
        let packages = Self::extract_packages(&repodata);

        let (version, pkg) = packages
            .get(name)
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        Ok(pkg_to_meta(name, version, pkg))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let repodata = Self::fetch_repodata()?;

        let mut versions: Vec<String> = Vec::new();

        // Collect all versions for this package
        for pkgs_key in ["packages", "packages.conda"] {
            if let Some(pkgs) = repodata[pkgs_key].as_object() {
                for (_filename, pkg) in pkgs {
                    if pkg["name"].as_str() == Some(name) {
                        if let Some(version) = pkg["version"].as_str() {
                            if !versions.contains(&version.to_string()) {
                                versions.push(version.to_string());
                            }
                        }
                    }
                }
            }
        }

        if versions.is_empty() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        // Sort descending
        versions.sort_by(|a, b| version_compare(b, a));

        Ok(versions
            .into_iter()
            .map(|v| VersionMeta {
                version: v,
                released: None,
                yanked: false,
            })
            .collect())
    }

    fn supports_fetch_all(&self) -> bool {
        true
    }

    fn fetch_all(&self) -> Result<Vec<PackageMeta>, IndexError> {
        let repodata = Self::fetch_repodata()?;
        let packages = Self::extract_packages(&repodata);

        Ok(packages
            .iter()
            .map(|(name, (version, pkg))| pkg_to_meta(name, version, pkg))
            .collect())
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let repodata = Self::fetch_repodata()?;
        let packages = Self::extract_packages(&repodata);
        let query_lower = query.to_lowercase();

        Ok(packages
            .iter()
            .filter(|(name, _)| name.to_lowercase().contains(&query_lower))
            .take(50)
            .map(|(name, (version, pkg))| pkg_to_meta(name, version, pkg))
            .collect())
    }
}

fn pkg_to_meta(name: &str, version: &str, pkg: &serde_json::Value) -> PackageMeta {
    let mut extra = std::collections::HashMap::new();

    // Extract dependencies
    if let Some(deps) = pkg["depends"].as_array() {
        let parsed_deps: Vec<serde_json::Value> = deps
            .iter()
            .filter_map(|d| d.as_str())
            .map(|d| {
                // Strip version constraints: "python >=3.8" -> "python"
                let name = d.split_whitespace().next().unwrap_or(d);
                serde_json::Value::String(name.to_string())
            })
            .collect();
        extra.insert("depends".to_string(), serde_json::Value::Array(parsed_deps));
    }

    // Extract size
    if let Some(size) = pkg["size"].as_u64() {
        extra.insert("size".to_string(), serde_json::Value::Number(size.into()));
    }

    PackageMeta {
        name: name.to_string(),
        version: version.to_string(),
        description: None, // repodata doesn't include descriptions
        homepage: None,
        repository: None,
        license: pkg["license"].as_str().map(String::from),
        binaries: Vec::new(),
        checksum: pkg["sha256"].as_str().map(|s| format!("sha256:{}", s)),
        extra,
        ..Default::default()
    }
}

fn version_compare(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| -> Vec<u32> {
        s.split(|c: char| !c.is_ascii_digit())
            .filter_map(|p| p.parse().ok())
            .collect()
    };
    parse(a).cmp(&parse(b))
}
