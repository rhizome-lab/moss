//! Homebrew Casks package index fetcher (macOS GUI apps).
//!
//! Fetches package metadata from formulae.brew.sh Casks API.

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use crate::cache;
use std::time::Duration;

/// Cache TTL for casks list (1 hour).
const INDEX_CACHE_TTL: Duration = Duration::from_secs(60 * 60);

/// Homebrew Casks package index fetcher.
pub struct HomebrewCasks;

impl HomebrewCasks {
    /// Homebrew Casks API.
    const CASKS_API: &'static str = "https://formulae.brew.sh/api/cask.json";

    /// Fetch and cache the full casks list.
    fn fetch_casks_list() -> Result<Vec<serde_json::Value>, IndexError> {
        let (data, _was_cached) = cache::fetch_with_cache(
            "homebrew_casks",
            "casks-all",
            Self::CASKS_API,
            INDEX_CACHE_TTL,
        )
        .map_err(|e| IndexError::Network(e))?;

        let casks: Vec<serde_json::Value> = serde_json::from_slice(&data)?;
        Ok(casks)
    }
}

impl PackageIndex for HomebrewCasks {
    fn ecosystem(&self) -> &'static str {
        "homebrew_casks"
    }

    fn display_name(&self) -> &'static str {
        "Homebrew Casks (macOS)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let casks = Self::fetch_casks_list()?;

        let cask = casks
            .iter()
            .find(|c| c["token"].as_str() == Some(name))
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        Ok(cask_to_meta(cask))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // Casks only have one version (the current one)
        let casks = Self::fetch_casks_list()?;

        let cask = casks
            .iter()
            .find(|c| c["token"].as_str() == Some(name))
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        let version = cask["version"].as_str().unwrap_or("unknown").to_string();

        Ok(vec![VersionMeta {
            version,
            released: None,
            yanked: cask["disabled"].as_bool().unwrap_or(false),
        }])
    }

    fn supports_fetch_all(&self) -> bool {
        true
    }

    fn fetch_all(&self) -> Result<Vec<PackageMeta>, IndexError> {
        let casks = Self::fetch_casks_list()?;
        Ok(casks.iter().map(cask_to_meta).collect())
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let casks = Self::fetch_casks_list()?;
        let query_lower = query.to_lowercase();

        Ok(casks
            .iter()
            .filter(|cask| {
                let token = cask["token"].as_str().unwrap_or("");
                let name = cask["name"]
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(|n| n.as_str())
                    .unwrap_or("");
                let desc = cask["desc"].as_str().unwrap_or("");

                token.to_lowercase().contains(&query_lower)
                    || name.to_lowercase().contains(&query_lower)
                    || desc.to_lowercase().contains(&query_lower)
            })
            .take(50)
            .map(cask_to_meta)
            .collect())
    }
}

fn cask_to_meta(cask: &serde_json::Value) -> PackageMeta {
    PackageMeta {
        name: cask["token"].as_str().unwrap_or("").to_string(),
        version: cask["version"].as_str().unwrap_or("unknown").to_string(),
        description: cask["desc"].as_str().map(String::from),
        homepage: cask["homepage"].as_str().map(String::from),
        repository: None,
        license: None,
        binaries: Vec::new(),
        archive_url: cask["url"].as_str().map(String::from),
        checksum: cask["sha256"].as_str().map(|s| format!("sha256:{}", s)),
        ..Default::default()
    }
}
