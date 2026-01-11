//! Conda package index fetcher.
//!
//! Fetches package metadata from conda channels (conda-forge, defaults, bioconda).
//!
//! ## API Strategy
//! - **fetch**: Searches channel repodata.json
//! - **fetch_versions**: Same index, collects all versions of package
//! - **search**: Filters cached repodata entries
//! - **fetch_all**: Full repodata.json (cached 1 hour, large download)
//!
//! ## Multi-channel Support
//! ```rust,ignore
//! use moss_packages::index::conda::{Conda, CondaChannel};
//!
//! // All channels (default)
//! let all = Conda::all();
//!
//! // conda-forge only
//! let forge = Conda::conda_forge();
//!
//! // Scientific channels
//! let scientific = Conda::scientific();
//! ```

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use crate::cache;
use rayon::prelude::*;
use std::collections::HashMap;
use std::time::Duration;

/// Cache TTL for repodata (1 hour).
const INDEX_CACHE_TTL: Duration = Duration::from_secs(60 * 60);

/// Available Conda channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CondaChannel {
    /// conda-forge - community maintained packages
    CondaForge,
    /// defaults - Anaconda's default channel
    Defaults,
    /// bioconda - bioinformatics packages
    Bioconda,
    /// pytorch - PyTorch packages
    Pytorch,
}

impl CondaChannel {
    /// Get the repodata URL for this channel.
    fn repodata_url(&self) -> &'static str {
        match self {
            Self::CondaForge => "https://conda.anaconda.org/conda-forge/linux-64/repodata.json",
            Self::Defaults => "https://repo.anaconda.com/pkgs/main/linux-64/repodata.json",
            Self::Bioconda => "https://conda.anaconda.org/bioconda/linux-64/repodata.json",
            Self::Pytorch => "https://conda.anaconda.org/pytorch/linux-64/repodata.json",
        }
    }

    /// Get the channel name for tagging.
    pub fn name(&self) -> &'static str {
        match self {
            Self::CondaForge => "conda-forge",
            Self::Defaults => "defaults",
            Self::Bioconda => "bioconda",
            Self::Pytorch => "pytorch",
        }
    }

    /// All available channels.
    pub fn all() -> &'static [CondaChannel] {
        &[
            Self::CondaForge,
            Self::Defaults,
            Self::Bioconda,
            Self::Pytorch,
        ]
    }

    /// conda-forge only.
    pub fn conda_forge() -> &'static [CondaChannel] {
        &[Self::CondaForge]
    }

    /// Scientific channels (conda-forge + bioconda).
    pub fn scientific() -> &'static [CondaChannel] {
        &[Self::CondaForge, Self::Bioconda]
    }

    /// Machine learning focused (conda-forge + pytorch).
    pub fn ml() -> &'static [CondaChannel] {
        &[Self::CondaForge, Self::Pytorch]
    }
}

/// Conda package index fetcher with configurable channels.
pub struct Conda {
    channels: Vec<CondaChannel>,
}

impl Conda {
    /// Create a fetcher with all channels.
    pub fn all() -> Self {
        Self {
            channels: CondaChannel::all().to_vec(),
        }
    }

    /// Create a fetcher with conda-forge only.
    pub fn conda_forge() -> Self {
        Self {
            channels: CondaChannel::conda_forge().to_vec(),
        }
    }

    /// Create a fetcher with scientific channels.
    pub fn scientific() -> Self {
        Self {
            channels: CondaChannel::scientific().to_vec(),
        }
    }

    /// Create a fetcher with ML channels.
    pub fn ml() -> Self {
        Self {
            channels: CondaChannel::ml().to_vec(),
        }
    }

    /// Create a fetcher with custom channel selection.
    pub fn with_channels(channels: &[CondaChannel]) -> Self {
        Self {
            channels: channels.to_vec(),
        }
    }

    /// Fetch and cache the repodata for a channel.
    fn fetch_repodata(channel: CondaChannel) -> Result<serde_json::Value, IndexError> {
        let (data, _was_cached) = cache::fetch_with_cache(
            "conda",
            &format!("{}-repodata", channel.name()),
            channel.repodata_url(),
            INDEX_CACHE_TTL,
        )
        .map_err(IndexError::Network)?;

        let repodata: serde_json::Value = serde_json::from_slice(&data)?;
        Ok(repodata)
    }

    /// Extract unique packages with their latest versions from repodata.
    fn extract_packages(
        repodata: &serde_json::Value,
        channel: CondaChannel,
    ) -> HashMap<String, (String, serde_json::Value, CondaChannel)> {
        let mut packages: HashMap<String, (String, serde_json::Value, CondaChannel)> =
            HashMap::new();

        for pkgs_key in ["packages", "packages.conda"] {
            if let Some(pkgs) = repodata[pkgs_key].as_object() {
                for (_filename, pkg) in pkgs {
                    if let Some(name) = pkg["name"].as_str() {
                        let version = pkg["version"].as_str().unwrap_or("0");
                        let entry = packages
                            .entry(name.to_string())
                            .or_insert_with(|| (version.to_string(), pkg.clone(), channel));
                        // Keep the latest version
                        if version_compare(version, &entry.0) == std::cmp::Ordering::Greater {
                            *entry = (version.to_string(), pkg.clone(), channel);
                        }
                    }
                }
            }
        }

        packages
    }

    /// Load packages from all configured channels.
    fn load_all_packages(
        &self,
    ) -> Result<HashMap<String, (String, serde_json::Value, CondaChannel)>, IndexError> {
        let results: Vec<_> = self
            .channels
            .par_iter()
            .filter_map(|&channel| match Self::fetch_repodata(channel) {
                Ok(repodata) => Some(Self::extract_packages(&repodata, channel)),
                Err(e) => {
                    eprintln!(
                        "Warning: failed to load Conda channel {}: {}",
                        channel.name(),
                        e
                    );
                    None
                }
            })
            .collect();

        let mut all_packages = HashMap::new();
        for packages in results {
            for (name, (version, pkg, channel)) in packages {
                let entry = all_packages
                    .entry(name)
                    .or_insert_with(|| (version.clone(), pkg.clone(), channel));
                if version_compare(&version, &entry.0) == std::cmp::Ordering::Greater {
                    *entry = (version, pkg, channel);
                }
            }
        }

        Ok(all_packages)
    }
}

impl PackageIndex for Conda {
    fn ecosystem(&self) -> &'static str {
        "conda"
    }

    fn display_name(&self) -> &'static str {
        "Conda"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let packages = self.load_all_packages()?;

        let (version, pkg, channel) = packages
            .get(name)
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        Ok(pkg_to_meta(name, version, pkg, *channel))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let mut all_versions: Vec<(String, CondaChannel)> = Vec::new();

        for &channel in &self.channels {
            if let Ok(repodata) = Self::fetch_repodata(channel) {
                for pkgs_key in ["packages", "packages.conda"] {
                    if let Some(pkgs) = repodata[pkgs_key].as_object() {
                        for (_filename, pkg) in pkgs {
                            if pkg["name"].as_str() == Some(name) {
                                if let Some(version) = pkg["version"].as_str() {
                                    let ver_with_channel = (version.to_string(), channel);
                                    if !all_versions
                                        .iter()
                                        .any(|(v, c)| v == version && *c == channel)
                                    {
                                        all_versions.push(ver_with_channel);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if all_versions.is_empty() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        // Sort by version descending
        all_versions.sort_by(|a, b| version_compare(&b.0, &a.0));

        Ok(all_versions
            .into_iter()
            .map(|(version, channel)| VersionMeta {
                version: format!("{} ({})", version, channel.name()),
                released: None,
                yanked: false,
            })
            .collect())
    }

    fn supports_fetch_all(&self) -> bool {
        true
    }

    fn fetch_all(&self) -> Result<Vec<PackageMeta>, IndexError> {
        let packages = self.load_all_packages()?;

        Ok(packages
            .iter()
            .map(|(name, (version, pkg, channel))| pkg_to_meta(name, version, pkg, *channel))
            .collect())
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let packages = self.load_all_packages()?;
        let query_lower = query.to_lowercase();

        Ok(packages
            .iter()
            .filter(|(name, _)| name.to_lowercase().contains(&query_lower))
            .take(50)
            .map(|(name, (version, pkg, channel))| pkg_to_meta(name, version, pkg, *channel))
            .collect())
    }
}

fn pkg_to_meta(
    name: &str,
    version: &str,
    pkg: &serde_json::Value,
    channel: CondaChannel,
) -> PackageMeta {
    let mut extra = HashMap::new();
    extra.insert(
        "source_repo".to_string(),
        serde_json::Value::String(channel.name().to_string()),
    );

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
        keywords: Vec::new(),
        maintainers: Vec::new(),
        published: None,
        downloads: None,
        archive_url: None,
        checksum: pkg["sha256"].as_str().map(|s| format!("sha256:{}", s)),
        extra,
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
