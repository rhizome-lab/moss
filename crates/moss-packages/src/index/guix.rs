//! GNU Guix package index fetcher.
//!
//! Fetches package metadata from guix.gnu.org/packages.json.
//! The server returns gzip-compressed JSON which ureq decompresses automatically.
//!
//! ## API Strategy
//! - **fetch**: Searches cached `guix.gnu.org/packages.json`
//! - **fetch_versions**: Same, single version per package
//! - **search**: Filters cached packages.json
//! - **fetch_all**: `guix.gnu.org/packages.json` (cached 6 hours, ~25MB)
//!
//! ## Multi-channel Support
//! ```rust,ignore
//! use moss_packages::index::guix::{Guix, GuixChannel};
//!
//! // All channels (default)
//! let all = Guix::all();
//!
//! // Official Guix channel only
//! let official = Guix::official();
//!
//! // With nonguix (community nonfree)
//! let with_nonguix = Guix::with_nonguix();
//! ```

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use crate::cache;
use rayon::prelude::*;
use std::collections::HashMap;
use std::time::Duration;

/// Cache TTL for the Guix package list (6 hours - it's a 25MB download).
const INDEX_CACHE_TTL: Duration = Duration::from_secs(6 * 60 * 60);

/// Available Guix channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GuixChannel {
    /// Official GNU Guix channel
    Guix,
    /// Nonguix - community channel for non-free software
    Nonguix,
}

impl GuixChannel {
    /// Get the packages.json URL for this channel.
    fn packages_url(&self) -> Option<&'static str> {
        match self {
            Self::Guix => Some("https://guix.gnu.org/packages.json"),
            // Nonguix doesn't have a public packages.json API
            Self::Nonguix => None,
        }
    }

    /// Get the channel name for tagging.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Guix => "guix",
            Self::Nonguix => "nonguix",
        }
    }

    /// All available channels.
    pub fn all() -> &'static [GuixChannel] {
        &[Self::Guix, Self::Nonguix]
    }

    /// Official channel only.
    pub fn official() -> &'static [GuixChannel] {
        &[Self::Guix]
    }
}

/// GNU Guix package index fetcher with configurable channels.
pub struct Guix {
    channels: Vec<GuixChannel>,
}

impl Guix {
    /// Create a fetcher with all channels.
    pub fn all() -> Self {
        Self {
            channels: GuixChannel::all().to_vec(),
        }
    }

    /// Create a fetcher with official Guix channel only.
    pub fn official() -> Self {
        Self {
            channels: GuixChannel::official().to_vec(),
        }
    }

    /// Create a fetcher with custom channel selection.
    pub fn with_channels(channels: &[GuixChannel]) -> Self {
        Self {
            channels: channels.to_vec(),
        }
    }

    /// Alias for `all()` including nonguix.
    pub fn with_nonguix() -> Self {
        Self::all()
    }

    /// Fetch the full package list from a channel with caching.
    fn fetch_package_list(channel: GuixChannel) -> Result<Vec<serde_json::Value>, IndexError> {
        let url = channel.packages_url().ok_or_else(|| {
            IndexError::NotImplemented(format!("{} has no public API", channel.name()))
        })?;

        let (data, _was_cached) = cache::fetch_with_cache(
            "guix",
            &format!("{}-packages", channel.name()),
            url,
            INDEX_CACHE_TTL,
        )
        .map_err(IndexError::Network)?;

        let packages: Vec<serde_json::Value> = serde_json::from_slice(&data)?;
        Ok(packages)
    }

    /// Load packages from all configured channels.
    fn load_packages(&self) -> Result<Vec<PackageMeta>, IndexError> {
        let results: Vec<_> = self
            .channels
            .par_iter()
            .filter_map(|&channel| {
                match Self::fetch_package_list(channel) {
                    Ok(packages) => Some(
                        packages
                            .iter()
                            .map(|p| package_to_meta(p, "", channel))
                            .collect::<Vec<_>>(),
                    ),
                    Err(IndexError::NotImplemented(_)) => None, // Skip unsupported channels
                    Err(e) => {
                        eprintln!(
                            "Warning: failed to load Guix channel {}: {}",
                            channel.name(),
                            e
                        );
                        None
                    }
                }
            })
            .flatten()
            .collect();

        Ok(results)
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
        // Try each channel until we find the package
        for &channel in &self.channels {
            if let Ok(packages) = Self::fetch_package_list(channel) {
                if let Some(pkg) = packages.iter().find(|p| p["name"].as_str() == Some(name)) {
                    return Ok(package_to_meta(pkg, name, channel));
                }
            }
        }

        Err(IndexError::NotFound(name.to_string()))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let mut all_versions = Vec::new();

        for &channel in &self.channels {
            if let Ok(packages) = Self::fetch_package_list(channel) {
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
                all_versions.extend(versions);
            }
        }

        if all_versions.is_empty() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        Ok(all_versions)
    }

    fn supports_fetch_all(&self) -> bool {
        true
    }

    fn fetch_all(&self) -> Result<Vec<PackageMeta>, IndexError> {
        self.load_packages()
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let packages = self.load_packages()?;
        let query_lower = query.to_lowercase();

        Ok(packages
            .into_iter()
            .filter(|pkg| {
                pkg.name.to_lowercase().contains(&query_lower)
                    || pkg
                        .description
                        .as_ref()
                        .map(|d| d.to_lowercase().contains(&query_lower))
                        .unwrap_or(false)
            })
            .take(50)
            .collect())
    }
}

/// Convert a Guix package JSON object to PackageMeta.
fn package_to_meta(
    pkg: &serde_json::Value,
    fallback_name: &str,
    channel: GuixChannel,
) -> PackageMeta {
    let mut extra = HashMap::new();
    extra.insert(
        "source_repo".to_string(),
        serde_json::Value::String(channel.name().to_string()),
    );

    PackageMeta {
        name: pkg["name"].as_str().unwrap_or(fallback_name).to_string(),
        version: pkg["version"].as_str().unwrap_or("unknown").to_string(),
        description: pkg["synopsis"].as_str().map(String::from),
        homepage: pkg["homepage"].as_str().map(String::from),
        repository: extract_repo(pkg),
        license: None, // Guix packages.json doesn't include license info
        binaries: Vec::new(),
        keywords: Vec::new(),
        maintainers: Vec::new(),
        published: None,
        downloads: None,
        archive_url: None,
        checksum: None,
        extra,
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
