//! FreeBSD package index fetcher (pkg).
//!
//! Fetches package metadata from FreeBSD package repositories.
//!
//! ## API Strategy
//! - **fetch**: Searches cached `pkg.freebsd.org/.../packagesite.pkg` (zstd tar + JSON-lines)
//! - **fetch_versions**: Loads from all configured repos
//! - **search**: Filters cached packagesite data
//! - **fetch_all**: Full packagesite.pkg (cached 1 hour, ~60MB uncompressed per repo)
//!
//! ## Multi-repo Support
//! ```rust,ignore
//! use moss_packages::index::freebsd::{FreeBsd, FreeBsdRepo};
//!
//! // All repos (default)
//! let all = FreeBsd::all();
//!
//! // Latest only for FreeBSD 14
//! let latest = FreeBsd::with_repos(&[FreeBsdRepo::FreeBsd14Latest]);
//!
//! // Quarterly (more stable)
//! let quarterly = FreeBsd::quarterly();
//! ```

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use crate::cache;
use rayon::prelude::*;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::time::Duration;

/// Cache TTL for FreeBSD package index (1 hour).
const CACHE_TTL: Duration = Duration::from_secs(60 * 60);

/// FreeBSD package repository base URL.
const FREEBSD_PKG_URL: &str = "https://pkg.freebsd.org";

/// Available FreeBSD repositories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FreeBsdRepo {
    // === FreeBSD 14 ===
    /// FreeBSD 14 latest packages
    FreeBsd14Latest,
    /// FreeBSD 14 quarterly packages (more stable)
    FreeBsd14Quarterly,

    // === FreeBSD 13 ===
    /// FreeBSD 13 latest packages
    FreeBsd13Latest,
    /// FreeBSD 13 quarterly packages
    FreeBsd13Quarterly,

    // === FreeBSD 15 (beta/development) ===
    /// FreeBSD 15 latest packages (development)
    FreeBsd15Latest,
}

impl FreeBsdRepo {
    /// Get the repository URL path.
    fn url(&self) -> String {
        let (version, branch) = match self {
            Self::FreeBsd14Latest => ("14", "latest"),
            Self::FreeBsd14Quarterly => ("14", "quarterly"),
            Self::FreeBsd13Latest => ("13", "latest"),
            Self::FreeBsd13Quarterly => ("13", "quarterly"),
            Self::FreeBsd15Latest => ("15", "latest"),
        };
        format!(
            "{}/FreeBSD:{}:amd64/{}/packagesite.pkg",
            FREEBSD_PKG_URL, version, branch
        )
    }

    /// Get the repository name for tagging.
    pub fn name(&self) -> &'static str {
        match self {
            Self::FreeBsd14Latest => "freebsd14-latest",
            Self::FreeBsd14Quarterly => "freebsd14-quarterly",
            Self::FreeBsd13Latest => "freebsd13-latest",
            Self::FreeBsd13Quarterly => "freebsd13-quarterly",
            Self::FreeBsd15Latest => "freebsd15-latest",
        }
    }

    /// All available repositories.
    pub fn all() -> &'static [FreeBsdRepo] {
        &[
            Self::FreeBsd14Latest,
            Self::FreeBsd14Quarterly,
            Self::FreeBsd13Latest,
            Self::FreeBsd13Quarterly,
            Self::FreeBsd15Latest,
        ]
    }

    /// Latest branch only (rolling).
    pub fn latest() -> &'static [FreeBsdRepo] {
        &[
            Self::FreeBsd14Latest,
            Self::FreeBsd13Latest,
            Self::FreeBsd15Latest,
        ]
    }

    /// Quarterly branch only (stable).
    pub fn quarterly() -> &'static [FreeBsdRepo] {
        &[Self::FreeBsd14Quarterly, Self::FreeBsd13Quarterly]
    }

    /// FreeBSD 14 repos (current release).
    pub fn freebsd14() -> &'static [FreeBsdRepo] {
        &[Self::FreeBsd14Latest, Self::FreeBsd14Quarterly]
    }
}

/// FreeBSD package index fetcher with configurable repositories.
pub struct FreeBsd {
    repos: Vec<FreeBsdRepo>,
}

impl FreeBsd {
    /// Create a fetcher with all repositories.
    pub fn all() -> Self {
        Self {
            repos: FreeBsdRepo::all().to_vec(),
        }
    }

    /// Create a fetcher with latest repositories only.
    pub fn latest() -> Self {
        Self {
            repos: FreeBsdRepo::latest().to_vec(),
        }
    }

    /// Create a fetcher with quarterly repositories only.
    pub fn quarterly() -> Self {
        Self {
            repos: FreeBsdRepo::quarterly().to_vec(),
        }
    }

    /// Create a fetcher with FreeBSD 14 repositories.
    pub fn freebsd14() -> Self {
        Self {
            repos: FreeBsdRepo::freebsd14().to_vec(),
        }
    }

    /// Create a fetcher with custom repository selection.
    pub fn with_repos(repos: &[FreeBsdRepo]) -> Self {
        Self {
            repos: repos.to_vec(),
        }
    }

    /// Parse a JSON-lines package entry.
    fn parse_package(line: &str, repo: FreeBsdRepo) -> Option<PackageMeta> {
        let pkg: serde_json::Value = serde_json::from_str(line).ok()?;

        let name = pkg["name"].as_str()?;
        let version = pkg["version"].as_str().unwrap_or("unknown");

        let license = pkg["licenses"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|l| l.as_str())
            .map(String::from);

        let mut extra = HashMap::new();
        if let Some(deps) = pkg["deps"].as_object() {
            let dep_names: Vec<serde_json::Value> = deps
                .keys()
                .map(|k| serde_json::Value::String(k.clone()))
                .collect();
            extra.insert("depends".to_string(), serde_json::Value::Array(dep_names));
        }

        // Extract provides (shared libraries)
        if let Some(shlibs) = pkg["shlibs_provided"].as_array() {
            let provides: Vec<serde_json::Value> = shlibs
                .iter()
                .filter_map(|s| s.as_str())
                .map(|s| serde_json::Value::String(s.to_string()))
                .collect();
            if !provides.is_empty() {
                extra.insert("provides".to_string(), serde_json::Value::Array(provides));
            }
        }

        // Tag with source repo
        extra.insert(
            "source_repo".to_string(),
            serde_json::Value::String(repo.name().to_string()),
        );

        // Add package size if available
        if let Some(size) = pkg["pkgsize"].as_u64() {
            extra.insert("size".to_string(), serde_json::Value::Number(size.into()));
        }

        Some(PackageMeta {
            name: name.to_string(),
            version: version.to_string(),
            description: pkg["comment"].as_str().map(String::from),
            homepage: pkg["www"].as_str().map(String::from),
            repository: Some("https://www.freshports.org/".to_string()),
            license,
            maintainers: pkg["maintainer"]
                .as_str()
                .map(|m| vec![m.to_string()])
                .unwrap_or_default(),
            binaries: Vec::new(),
            keywords: Vec::new(),
            published: None,
            downloads: None,
            archive_url: None,
            checksum: None,
            extra,
        })
    }

    /// Load packages from a single repository.
    fn load_repo(repo: FreeBsdRepo) -> Result<Vec<PackageMeta>, IndexError> {
        let url = repo.url();

        let (data, _was_cached) = cache::fetch_with_cache(
            "freebsd",
            &format!("packagesite-{}", repo.name()),
            &url,
            CACHE_TTL,
        )
        .map_err(IndexError::Network)?;

        // Decompress zstd
        let decompressed = zstd::decode_all(std::io::Cursor::new(&data))
            .map_err(|e| IndexError::Decompress(e.to_string()))?;

        // Extract tar
        let mut archive = tar::Archive::new(std::io::Cursor::new(decompressed));
        let mut packages = Vec::new();

        for entry in archive.entries().map_err(IndexError::Io)? {
            let entry = entry.map_err(IndexError::Io)?;
            let path = entry.path().map_err(IndexError::Io)?;
            let path_str = path.to_string_lossy();

            // Match exact filename, not .sig or .pub
            if path_str == "packagesite.yaml" {
                // Parse JSON-lines
                let reader = BufReader::new(entry);
                for line in reader.lines() {
                    if let Ok(line) = line {
                        if !line.is_empty() {
                            if let Some(pkg) = Self::parse_package(&line, repo) {
                                packages.push(pkg);
                            }
                        }
                    }
                }
                break;
            }
        }

        Ok(packages)
    }

    /// Load packages from all configured repositories in parallel.
    fn load_packages(&self) -> Result<Vec<PackageMeta>, IndexError> {
        let results: Vec<_> = self
            .repos
            .par_iter()
            .map(|&repo| Self::load_repo(repo))
            .collect();

        let mut packages = Vec::new();
        for result in results {
            match result {
                Ok(pkgs) => packages.extend(pkgs),
                Err(e) => {
                    eprintln!("Warning: failed to load FreeBSD repo: {}", e);
                }
            }
        }

        Ok(packages)
    }
}

impl PackageIndex for FreeBsd {
    fn ecosystem(&self) -> &'static str {
        "freebsd"
    }

    fn display_name(&self) -> &'static str {
        "FreeBSD (pkg)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let packages = self.load_packages()?;

        packages
            .into_iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| IndexError::NotFound(name.to_string()))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let packages = self.load_packages()?;

        let versions: Vec<_> = packages
            .into_iter()
            .filter(|p| p.name.eq_ignore_ascii_case(name))
            .map(|p| VersionMeta {
                version: p.version,
                released: None,
                yanked: false,
            })
            .collect();

        if versions.is_empty() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        Ok(versions)
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let packages = self.load_packages()?;
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
            .collect())
    }

    fn supports_fetch_all(&self) -> bool {
        true
    }

    fn fetch_all(&self) -> Result<Vec<PackageMeta>, IndexError> {
        self.load_packages()
    }
}
