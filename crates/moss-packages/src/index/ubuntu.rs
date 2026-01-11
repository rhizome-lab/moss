//! Ubuntu package index fetcher.
//!
//! Fetches package metadata from Ubuntu repositories by parsing
//! Packages files from mirror indices.
//!
//! ## API Strategy
//! - **fetch**: Uses Launchpad API
//! - **fetch_versions**: Launchpad API
//! - **search**: Filters cached Packages entries
//! - **fetch_all**: Returns all packages from configured repos (cached 1 hour)
//!
//! ## Multi-repo Support
//! ```rust,ignore
//! use moss_packages::index::ubuntu::{Ubuntu, UbuntuRepo};
//!
//! // All repos (default)
//! let all = Ubuntu::all();
//!
//! // Noble (24.04 LTS) only
//! let noble = Ubuntu::noble();
//!
//! // Jammy (22.04 LTS) only
//! let jammy = Ubuntu::jammy();
//!
//! // Custom selection
//! let custom = Ubuntu::with_repos(&[UbuntuRepo::NobleMain, UbuntuRepo::NobleUniverse]);
//! ```

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use crate::cache;
use flate2::read::GzDecoder;
use rayon::prelude::*;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Cursor, Read};
use std::time::Duration;

/// Default cache TTL for package indices (1 hour).
const INDEX_CACHE_TTL: Duration = Duration::from_secs(60 * 60);

/// Ubuntu mirror URL.
const UBUNTU_MIRROR: &str = "https://archive.ubuntu.com/ubuntu";

/// Available Ubuntu repositories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UbuntuRepo {
    // === Noble Numbat (24.04 LTS) ===
    /// Noble main repository
    NobleMain,
    /// Noble restricted repository
    NobleRestricted,
    /// Noble universe repository
    NobleUniverse,
    /// Noble multiverse repository
    NobleMultiverse,
    /// Noble updates main
    NobleUpdatesMain,
    /// Noble updates universe
    NobleUpdatesUniverse,
    /// Noble security main
    NobleSecurityMain,
    /// Noble security universe
    NobleSecurityUniverse,
    /// Noble backports main
    NobleBackportsMain,
    /// Noble backports universe
    NobleBackportsUniverse,

    // === Jammy Jellyfish (22.04 LTS) ===
    /// Jammy main repository
    JammyMain,
    /// Jammy restricted repository
    JammyRestricted,
    /// Jammy universe repository
    JammyUniverse,
    /// Jammy multiverse repository
    JammyMultiverse,
    /// Jammy updates main
    JammyUpdatesMain,
    /// Jammy updates universe
    JammyUpdatesUniverse,
    /// Jammy security main
    JammySecurityMain,
    /// Jammy security universe
    JammySecurityUniverse,
    /// Jammy backports main
    JammyBackportsMain,
    /// Jammy backports universe
    JammyBackportsUniverse,

    // === Oracular Oriole (24.10) ===
    /// Oracular main repository
    OracularMain,
    /// Oracular universe repository
    OracularUniverse,
}

impl UbuntuRepo {
    /// Get the distribution and component parts.
    fn parts(&self) -> (&'static str, &'static str) {
        match self {
            // Noble 24.04 LTS
            Self::NobleMain => ("noble", "main"),
            Self::NobleRestricted => ("noble", "restricted"),
            Self::NobleUniverse => ("noble", "universe"),
            Self::NobleMultiverse => ("noble", "multiverse"),
            Self::NobleUpdatesMain => ("noble-updates", "main"),
            Self::NobleUpdatesUniverse => ("noble-updates", "universe"),
            Self::NobleSecurityMain => ("noble-security", "main"),
            Self::NobleSecurityUniverse => ("noble-security", "universe"),
            Self::NobleBackportsMain => ("noble-backports", "main"),
            Self::NobleBackportsUniverse => ("noble-backports", "universe"),

            // Jammy 22.04 LTS
            Self::JammyMain => ("jammy", "main"),
            Self::JammyRestricted => ("jammy", "restricted"),
            Self::JammyUniverse => ("jammy", "universe"),
            Self::JammyMultiverse => ("jammy", "multiverse"),
            Self::JammyUpdatesMain => ("jammy-updates", "main"),
            Self::JammyUpdatesUniverse => ("jammy-updates", "universe"),
            Self::JammySecurityMain => ("jammy-security", "main"),
            Self::JammySecurityUniverse => ("jammy-security", "universe"),
            Self::JammyBackportsMain => ("jammy-backports", "main"),
            Self::JammyBackportsUniverse => ("jammy-backports", "universe"),

            // Oracular 24.10
            Self::OracularMain => ("oracular", "main"),
            Self::OracularUniverse => ("oracular", "universe"),
        }
    }

    /// Get the Packages.gz URL for this repository.
    fn packages_url(&self) -> String {
        let (dist, component) = self.parts();
        format!(
            "{}/dists/{}/{}/binary-amd64/Packages.gz",
            UBUNTU_MIRROR, dist, component
        )
    }

    /// Get the repository name for tagging.
    pub fn name(&self) -> &'static str {
        match self {
            Self::NobleMain => "noble-main",
            Self::NobleRestricted => "noble-restricted",
            Self::NobleUniverse => "noble-universe",
            Self::NobleMultiverse => "noble-multiverse",
            Self::NobleUpdatesMain => "noble-updates-main",
            Self::NobleUpdatesUniverse => "noble-updates-universe",
            Self::NobleSecurityMain => "noble-security-main",
            Self::NobleSecurityUniverse => "noble-security-universe",
            Self::NobleBackportsMain => "noble-backports-main",
            Self::NobleBackportsUniverse => "noble-backports-universe",

            Self::JammyMain => "jammy-main",
            Self::JammyRestricted => "jammy-restricted",
            Self::JammyUniverse => "jammy-universe",
            Self::JammyMultiverse => "jammy-multiverse",
            Self::JammyUpdatesMain => "jammy-updates-main",
            Self::JammyUpdatesUniverse => "jammy-updates-universe",
            Self::JammySecurityMain => "jammy-security-main",
            Self::JammySecurityUniverse => "jammy-security-universe",
            Self::JammyBackportsMain => "jammy-backports-main",
            Self::JammyBackportsUniverse => "jammy-backports-universe",

            Self::OracularMain => "oracular-main",
            Self::OracularUniverse => "oracular-universe",
        }
    }

    /// All available repositories.
    pub fn all() -> &'static [UbuntuRepo] {
        &[
            Self::NobleMain,
            Self::NobleRestricted,
            Self::NobleUniverse,
            Self::NobleMultiverse,
            Self::NobleUpdatesMain,
            Self::NobleUpdatesUniverse,
            Self::NobleSecurityMain,
            Self::NobleSecurityUniverse,
            Self::NobleBackportsMain,
            Self::NobleBackportsUniverse,
            Self::JammyMain,
            Self::JammyRestricted,
            Self::JammyUniverse,
            Self::JammyMultiverse,
            Self::JammyUpdatesMain,
            Self::JammyUpdatesUniverse,
            Self::JammySecurityMain,
            Self::JammySecurityUniverse,
            Self::JammyBackportsMain,
            Self::JammyBackportsUniverse,
            Self::OracularMain,
            Self::OracularUniverse,
        ]
    }

    /// Noble (24.04 LTS) repositories only.
    pub fn noble() -> &'static [UbuntuRepo] {
        &[
            Self::NobleMain,
            Self::NobleRestricted,
            Self::NobleUniverse,
            Self::NobleMultiverse,
            Self::NobleUpdatesMain,
            Self::NobleUpdatesUniverse,
            Self::NobleSecurityMain,
            Self::NobleSecurityUniverse,
            Self::NobleBackportsMain,
            Self::NobleBackportsUniverse,
        ]
    }

    /// Jammy (22.04 LTS) repositories only.
    pub fn jammy() -> &'static [UbuntuRepo] {
        &[
            Self::JammyMain,
            Self::JammyRestricted,
            Self::JammyUniverse,
            Self::JammyMultiverse,
            Self::JammyUpdatesMain,
            Self::JammyUpdatesUniverse,
            Self::JammySecurityMain,
            Self::JammySecurityUniverse,
            Self::JammyBackportsMain,
            Self::JammyBackportsUniverse,
        ]
    }

    /// LTS releases only (Noble + Jammy).
    pub fn lts() -> &'static [UbuntuRepo] {
        &[
            Self::NobleMain,
            Self::NobleUniverse,
            Self::NobleUpdatesMain,
            Self::NobleUpdatesUniverse,
            Self::JammyMain,
            Self::JammyUniverse,
            Self::JammyUpdatesMain,
            Self::JammyUpdatesUniverse,
        ]
    }

    /// Main repositories only (no universe/multiverse).
    pub fn main_only() -> &'static [UbuntuRepo] {
        &[
            Self::NobleMain,
            Self::NobleUpdatesMain,
            Self::NobleSecurityMain,
            Self::JammyMain,
            Self::JammyUpdatesMain,
            Self::JammySecurityMain,
            Self::OracularMain,
        ]
    }
}

/// Ubuntu package index fetcher with configurable repositories.
pub struct Ubuntu {
    repos: Vec<UbuntuRepo>,
}

impl Ubuntu {
    /// Create a fetcher with all repositories.
    pub fn all() -> Self {
        Self {
            repos: UbuntuRepo::all().to_vec(),
        }
    }

    /// Create a fetcher with Noble (24.04 LTS) repositories.
    pub fn noble() -> Self {
        Self {
            repos: UbuntuRepo::noble().to_vec(),
        }
    }

    /// Create a fetcher with Jammy (22.04 LTS) repositories.
    pub fn jammy() -> Self {
        Self {
            repos: UbuntuRepo::jammy().to_vec(),
        }
    }

    /// Create a fetcher with LTS releases only.
    pub fn lts() -> Self {
        Self {
            repos: UbuntuRepo::lts().to_vec(),
        }
    }

    /// Create a fetcher with main repositories only.
    pub fn main_only() -> Self {
        Self {
            repos: UbuntuRepo::main_only().to_vec(),
        }
    }

    /// Create a fetcher with custom repository selection.
    pub fn with_repos(repos: &[UbuntuRepo]) -> Self {
        Self {
            repos: repos.to_vec(),
        }
    }

    /// Parse a Packages file in Debian control format.
    fn parse_control<R: Read>(reader: R, repo: UbuntuRepo) -> Vec<PackageMeta> {
        let reader = BufReader::new(reader);
        let mut packages = Vec::new();
        let mut current: Option<PackageBuilder> = None;

        for line in reader.lines().map_while(Result::ok) {
            if line.is_empty() {
                if let Some(builder) = current.take() {
                    if let Some(pkg) = builder.build(repo) {
                        packages.push(pkg);
                    }
                }
                continue;
            }

            if line.starts_with(' ') || line.starts_with('\t') {
                continue;
            }

            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim();
                let value = value.trim();

                let builder = current.get_or_insert_with(PackageBuilder::new);

                match key {
                    "Package" => builder.name = Some(value.to_string()),
                    "Version" => builder.version = Some(value.to_string()),
                    "Description" => builder.description = Some(value.to_string()),
                    "Homepage" => builder.homepage = Some(value.to_string()),
                    "Vcs-Git" | "Vcs-Browser" => {
                        if builder.repository.is_none() {
                            builder.repository = Some(value.to_string());
                        }
                    }
                    "Filename" => builder.filename = Some(value.to_string()),
                    "SHA256" => builder.sha256 = Some(value.to_string()),
                    "Depends" => builder.depends = Some(value.to_string()),
                    "Size" => builder.size = value.parse().ok(),
                    _ => {}
                }
            }
        }

        if let Some(builder) = current {
            if let Some(pkg) = builder.build(repo) {
                packages.push(pkg);
            }
        }

        packages
    }

    /// Load packages from a single repository.
    fn load_repo(repo: UbuntuRepo) -> Result<Vec<PackageMeta>, IndexError> {
        let url = repo.packages_url();

        let (data, _was_cached) = cache::fetch_with_cache(
            "ubuntu",
            &format!("packages-{}", repo.name()),
            &url,
            INDEX_CACHE_TTL,
        )
        .map_err(IndexError::Network)?;

        let reader: Box<dyn Read> = if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
            Box::new(GzDecoder::new(Cursor::new(data)))
        } else {
            Box::new(Cursor::new(data))
        };

        Ok(Self::parse_control(reader, repo))
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
                    eprintln!("Warning: failed to load Ubuntu repo: {}", e);
                }
            }
        }

        Ok(packages)
    }
}

impl PackageIndex for Ubuntu {
    fn ecosystem(&self) -> &'static str {
        "ubuntu"
    }

    fn display_name(&self) -> &'static str {
        "Ubuntu"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Use Launchpad API for single package lookup
        let url = format!(
            "https://api.launchpad.net/1.0/ubuntu/+archive/primary?ws.op=getPublishedSources&source_name={}&exact_match=true",
            urlencoding::encode(name)
        );

        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let entries = response["entries"]
            .as_array()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        let latest = entries
            .first()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        Ok(PackageMeta {
            name: latest["source_package_name"]
                .as_str()
                .unwrap_or(name)
                .to_string(),
            version: latest["source_package_version"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            description: None,
            homepage: None,
            repository: None,
            license: None,
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

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let url = format!(
            "https://api.launchpad.net/1.0/ubuntu/+archive/primary?ws.op=getPublishedSources&source_name={}&exact_match=true",
            urlencoding::encode(name)
        );

        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let entries = response["entries"]
            .as_array()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        if entries.is_empty() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        Ok(entries
            .iter()
            .filter_map(|e| {
                Some(VersionMeta {
                    version: e["source_package_version"].as_str()?.to_string(),
                    released: None,
                    yanked: false,
                })
            })
            .collect())
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
            .collect())
    }
}

#[derive(Default)]
struct PackageBuilder {
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
    homepage: Option<String>,
    repository: Option<String>,
    filename: Option<String>,
    sha256: Option<String>,
    depends: Option<String>,
    size: Option<u64>,
}

impl PackageBuilder {
    fn new() -> Self {
        Self::default()
    }

    fn build(self, repo: UbuntuRepo) -> Option<PackageMeta> {
        let mut extra = HashMap::new();

        if let Some(deps) = self.depends {
            let parsed_deps: Vec<String> = deps
                .split(',')
                .map(|d| {
                    d.trim()
                        .split_once(' ')
                        .map(|(name, _)| name)
                        .unwrap_or(d.trim())
                        .to_string()
                })
                .filter(|d| !d.is_empty())
                .collect();
            extra.insert(
                "depends".to_string(),
                serde_json::Value::Array(
                    parsed_deps
                        .into_iter()
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            );
        }

        if let Some(size) = self.size {
            extra.insert("size".to_string(), serde_json::Value::Number(size.into()));
        }

        extra.insert(
            "source_repo".to_string(),
            serde_json::Value::String(repo.name().to_string()),
        );

        Some(PackageMeta {
            name: self.name?,
            version: self.version?,
            description: self.description,
            homepage: self.homepage,
            repository: self.repository,
            license: None,
            binaries: Vec::new(),
            archive_url: self.filename.map(|f| format!("{}/{}", UBUNTU_MIRROR, f)),
            checksum: self.sha256.map(|h| format!("sha256:{}", h)),
            keywords: Vec::new(),
            maintainers: Vec::new(),
            published: None,
            downloads: None,
            extra,
        })
    }
}

mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut result = String::with_capacity(s.len() * 3);
        for c in s.chars() {
            match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
                _ => {
                    for b in c.to_string().bytes() {
                        result.push_str(&format!("%{:02X}", b));
                    }
                }
            }
        }
        result
    }
}
