//! Nix/NixOS package index fetcher.
//!
//! Fetches package metadata from nixpkgs via the NixOS search API.
//! Uses the Bonsai Elasticsearch cluster that powers search.nixos.org.
//!
//! ## API Strategy
//! - **fetch**: `search.nixos.org` Elasticsearch API - Official NixOS search
//! - **fetch_versions**: Queries all configured channels
//! - **search**: `search.nixos.org` Elasticsearch query
//! - **fetch_all**: Not supported (too large, use search instead)
//!
//! ## Multi-repo Support
//! ```rust,ignore
//! use moss_packages::index::nix::{Nix, NixChannel};
//!
//! // All channels (default)
//! let all = Nix::all();
//!
//! // Unstable only
//! let unstable = Nix::unstable();
//!
//! // Stable channels only
//! let stable = Nix::stable();
//!
//! // Custom selection
//! let custom = Nix::with_channels(&[NixChannel::NixosUnstable, NixChannel::Nixos2411]);
//! ```

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use rayon::prelude::*;
use std::collections::HashMap;

/// Simple base64 encoding for Basic Auth.
fn base64_encode(input: &str) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut result = String::with_capacity((bytes.len() + 2) / 3 * 4);

    for chunk in bytes.chunks(3) {
        let mut buf = [0u8; 3];
        buf[..chunk.len()].copy_from_slice(chunk);

        let indices = [
            (buf[0] >> 2) as usize,
            (((buf[0] & 0x03) << 4) | (buf[1] >> 4)) as usize,
            (((buf[1] & 0x0f) << 2) | (buf[2] >> 6)) as usize,
            (buf[2] & 0x3f) as usize,
        ];

        result.push(ALPHABET[indices[0]] as char);
        result.push(ALPHABET[indices[1]] as char);
        result.push(if chunk.len() > 1 {
            ALPHABET[indices[2]] as char
        } else {
            '='
        });
        result.push(if chunk.len() > 2 {
            ALPHABET[indices[3]] as char
        } else {
            '='
        });
    }

    result
}

/// Available Nix channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NixChannel {
    // === Unstable ===
    /// NixOS unstable (rolling release)
    NixosUnstable,
    /// nixpkgs unstable (for non-NixOS systems)
    NixpkgsUnstable,

    // === Stable ===
    /// NixOS 24.11 (latest stable)
    Nixos2411,
    /// NixOS 24.05
    Nixos2405,
    /// NixOS 23.11
    Nixos2311,

    // === Darwin ===
    /// nixpkgs-unstable for macOS
    NixpkgsDarwinUnstable,
}

impl NixChannel {
    /// Get the Elasticsearch index pattern for this channel.
    fn index_pattern(&self) -> &'static str {
        match self {
            Self::NixosUnstable => "latest-*-nixos-unstable",
            Self::NixpkgsUnstable => "latest-*-nixpkgs-unstable",
            Self::Nixos2411 => "latest-*-nixos-24.11",
            Self::Nixos2405 => "latest-*-nixos-24.05",
            Self::Nixos2311 => "latest-*-nixos-23.11",
            Self::NixpkgsDarwinUnstable => "latest-*-nixpkgs-unstable",
        }
    }

    /// Get the channel name for tagging.
    pub fn name(&self) -> &'static str {
        match self {
            Self::NixosUnstable => "nixos-unstable",
            Self::NixpkgsUnstable => "nixpkgs-unstable",
            Self::Nixos2411 => "nixos-24.11",
            Self::Nixos2405 => "nixos-24.05",
            Self::Nixos2311 => "nixos-23.11",
            Self::NixpkgsDarwinUnstable => "nixpkgs-darwin-unstable",
        }
    }

    /// All available channels.
    pub fn all() -> &'static [NixChannel] {
        &[
            Self::NixosUnstable,
            Self::NixpkgsUnstable,
            Self::Nixos2411,
            Self::Nixos2405,
            Self::Nixos2311,
        ]
    }

    /// Unstable channels only.
    pub fn unstable() -> &'static [NixChannel] {
        &[Self::NixosUnstable, Self::NixpkgsUnstable]
    }

    /// Stable channels only.
    pub fn stable() -> &'static [NixChannel] {
        &[Self::Nixos2411, Self::Nixos2405, Self::Nixos2311]
    }

    /// NixOS channels only (not plain nixpkgs).
    pub fn nixos() -> &'static [NixChannel] {
        &[
            Self::NixosUnstable,
            Self::Nixos2411,
            Self::Nixos2405,
            Self::Nixos2311,
        ]
    }

    /// Latest stable only.
    pub fn latest_stable() -> &'static [NixChannel] {
        &[Self::Nixos2411]
    }
}

/// Nix package index fetcher with configurable channels.
pub struct Nix {
    channels: Vec<NixChannel>,
}

impl Nix {
    /// NixOS Elasticsearch-based search API (Bonsai cluster).
    /// Public credentials from nixos-search repository.
    const NIXOS_SEARCH: &'static str =
        "https://nixos-search-7-1733963800.us-east-1.bonsaisearch.net";
    const AUTH_USER: &'static str = "aWVSALXpZv";
    const AUTH_PASS: &'static str = "X8gPHnzL52wFEekuxsfQ9cSh";

    /// Create a fetcher with all channels.
    pub fn all() -> Self {
        Self {
            channels: NixChannel::all().to_vec(),
        }
    }

    /// Create a fetcher with unstable channels only.
    pub fn unstable() -> Self {
        Self {
            channels: NixChannel::unstable().to_vec(),
        }
    }

    /// Create a fetcher with stable channels only.
    pub fn stable() -> Self {
        Self {
            channels: NixChannel::stable().to_vec(),
        }
    }

    /// Create a fetcher with NixOS channels only.
    pub fn nixos() -> Self {
        Self {
            channels: NixChannel::nixos().to_vec(),
        }
    }

    /// Create a fetcher with latest stable only.
    pub fn latest_stable() -> Self {
        Self {
            channels: NixChannel::latest_stable().to_vec(),
        }
    }

    /// Create a fetcher with custom channel selection.
    pub fn with_channels(channels: &[NixChannel]) -> Self {
        Self {
            channels: channels.to_vec(),
        }
    }

    /// Get authorization header value.
    fn auth_header() -> String {
        format!(
            "Basic {}",
            base64_encode(&format!("{}:{}", Self::AUTH_USER, Self::AUTH_PASS))
        )
    }

    /// Fetch a package from a specific channel.
    fn fetch_from_channel(name: &str, channel: NixChannel) -> Result<PackageMeta, IndexError> {
        let query = serde_json::json!({
            "query": {
                "bool": {
                    "must": [
                        { "term": { "type": "package" } },
                        { "term": { "package_attr_name": name } }
                    ]
                }
            },
            "size": 1
        });

        let response: serde_json::Value = ureq::post(&format!(
            "{}/{}/_search",
            Self::NIXOS_SEARCH,
            channel.index_pattern()
        ))
        .set("Content-Type", "application/json")
        .set("Accept", "application/json")
        .set("Authorization", &Self::auth_header())
        .send_json(&query)?
        .into_json()?;

        let hits = response["hits"]["hits"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing hits".into()))?;

        let hit = hits
            .first()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        let source = &hit["_source"];
        let mut extra = HashMap::new();

        // Tag with source channel
        extra.insert(
            "source_repo".to_string(),
            serde_json::Value::String(channel.name().to_string()),
        );

        Ok(PackageMeta {
            name: source["package_attr_name"]
                .as_str()
                .unwrap_or(name)
                .to_string(),
            version: source["package_version"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            description: source["package_description"].as_str().map(String::from),
            homepage: source["package_homepage"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|u| u.as_str())
                .map(String::from),
            repository: extract_repo(&source["package_homepage"]),
            license: source["package_license"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|l| l["fullName"].as_str().or_else(|| l.as_str()))
                .map(String::from),
            binaries: source["package_programs"]
                .as_array()
                .map(|progs| {
                    progs
                        .iter()
                        .filter_map(|p| p.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            maintainers: source["package_maintainers"]
                .as_array()
                .map(|m| {
                    m.iter()
                        .filter_map(|p| {
                            p["name"]
                                .as_str()
                                .or_else(|| p["github"].as_str())
                                .map(String::from)
                        })
                        .collect()
                })
                .unwrap_or_default(),
            keywords: Vec::new(),
            published: None,
            downloads: None,
            archive_url: None,
            checksum: None,
            extra,
        })
    }

    /// Search in a specific channel.
    fn search_channel(query: &str, channel: NixChannel) -> Result<Vec<PackageMeta>, IndexError> {
        let es_query = serde_json::json!({
            "query": {
                "bool": {
                    "must": [
                        { "term": { "type": "package" } },
                        {
                            "multi_match": {
                                "query": query,
                                "fields": [
                                    "package_attr_name^3",
                                    "package_pname^2",
                                    "package_description"
                                ]
                            }
                        }
                    ]
                }
            },
            "size": 50
        });

        let response: serde_json::Value = ureq::post(&format!(
            "{}/{}/_search",
            Self::NIXOS_SEARCH,
            channel.index_pattern()
        ))
        .set("Content-Type", "application/json")
        .set("Accept", "application/json")
        .set("Authorization", &Self::auth_header())
        .send_json(&es_query)?
        .into_json()?;

        let hits = response["hits"]["hits"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing hits".into()))?;

        Ok(hits
            .iter()
            .filter_map(|hit| {
                let source = &hit["_source"];
                let mut extra = HashMap::new();
                extra.insert(
                    "source_repo".to_string(),
                    serde_json::Value::String(channel.name().to_string()),
                );

                Some(PackageMeta {
                    name: source["package_attr_name"].as_str()?.to_string(),
                    version: source["package_version"]
                        .as_str()
                        .unwrap_or("unknown")
                        .to_string(),
                    description: source["package_description"].as_str().map(String::from),
                    homepage: source["package_homepage"]
                        .as_array()
                        .and_then(|a| a.first())
                        .and_then(|u| u.as_str())
                        .map(String::from),
                    repository: extract_repo(&source["package_homepage"]),
                    license: source["package_license"]
                        .as_array()
                        .and_then(|a| a.first())
                        .and_then(|l| l["fullName"].as_str().or_else(|| l.as_str()))
                        .map(String::from),
                    binaries: source["package_programs"]
                        .as_array()
                        .map(|progs| {
                            progs
                                .iter()
                                .filter_map(|p| p.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default(),
                    maintainers: source["package_maintainers"]
                        .as_array()
                        .map(|m| {
                            m.iter()
                                .filter_map(|p| {
                                    p["name"]
                                        .as_str()
                                        .or_else(|| p["github"].as_str())
                                        .map(String::from)
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    keywords: Vec::new(),
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

impl PackageIndex for Nix {
    fn ecosystem(&self) -> &'static str {
        "nix"
    }

    fn display_name(&self) -> &'static str {
        "Nixpkgs (Nix/NixOS)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Try each configured channel until we find the package
        for channel in &self.channels {
            match Self::fetch_from_channel(name, *channel) {
                Ok(pkg) => return Ok(pkg),
                Err(IndexError::NotFound(_)) => continue,
                Err(e) => return Err(e),
            }
        }

        Err(IndexError::NotFound(name.to_string()))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // Query all configured channels in parallel
        let results: Vec<_> = self
            .channels
            .par_iter()
            .filter_map(|channel| Self::fetch_from_channel(name, *channel).ok())
            .collect();

        if results.is_empty() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        Ok(results
            .into_iter()
            .map(|pkg| VersionMeta {
                version: pkg.version,
                released: None,
                yanked: false,
            })
            .collect())
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        // Search all configured channels in parallel
        let results: Vec<_> = self
            .channels
            .par_iter()
            .filter_map(|channel| Self::search_channel(query, *channel).ok())
            .flatten()
            .collect();

        Ok(results)
    }
}

fn extract_repo(homepage: &serde_json::Value) -> Option<String> {
    homepage.as_array().and_then(|urls| {
        urls.iter()
            .filter_map(|u| u.as_str())
            .find(|u| u.contains("github.com") || u.contains("gitlab.com"))
            .map(String::from)
    })
}
