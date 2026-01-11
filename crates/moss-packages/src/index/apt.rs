//! APT package index fetcher (Debian).
//!
//! Fetches package metadata from Debian repositories by parsing
//! Packages files from mirror indices.
//!
//! ## API Strategy
//! - **fetch**: Uses sources.debian.org API
//! - **fetch_versions**: Same API
//! - **search**: Filters cached Packages entries
//! - **fetch_all**: Returns all packages from configured repos (cached 1 hour)
//!
//! ## Multi-repo Support
//! ```rust,ignore
//! use moss_packages::index::apt::{Apt, AptRepo};
//!
//! // All repos (default)
//! let all = Apt::all();
//!
//! // Stable repos only
//! let stable = Apt::stable();
//!
//! // Testing repos
//! let testing = Apt::testing();
//!
//! // Custom selection
//! let custom = Apt::with_repos(&[AptRepo::StableMain, AptRepo::StableContrib]);
//! ```

use super::{IndexError, PackageIndex, PackageIter, PackageMeta, VersionMeta};
use crate::cache;
use flate2::read::GzDecoder;
use rayon::prelude::*;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Cursor, Read};
use std::time::Duration;

/// Default cache TTL for package indices (1 hour).
const INDEX_CACHE_TTL: Duration = Duration::from_secs(60 * 60);

/// Debian mirror URL.
const DEBIAN_MIRROR: &str = "https://deb.debian.org/debian";

/// Available Debian repositories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AptRepo {
    // === Stable (bookworm) ===
    /// Stable main repository
    StableMain,
    /// Stable contrib repository
    StableContrib,
    /// Stable non-free repository
    StableNonFree,
    /// Stable non-free-firmware repository
    StableNonFreeFirmware,

    // === Stable Backports ===
    /// Stable backports main
    StableBackportsMain,
    /// Stable backports contrib
    StableBackportsContrib,
    /// Stable backports non-free
    StableBackportsNonFree,

    // === Testing (trixie) ===
    /// Testing main repository
    TestingMain,
    /// Testing contrib repository
    TestingContrib,
    /// Testing non-free repository
    TestingNonFree,
    /// Testing non-free-firmware repository
    TestingNonFreeFirmware,

    // === Unstable (sid) ===
    /// Unstable main repository
    UnstableMain,
    /// Unstable contrib repository
    UnstableContrib,
    /// Unstable non-free repository
    UnstableNonFree,
    /// Unstable non-free-firmware repository
    UnstableNonFreeFirmware,

    // === Experimental ===
    /// Experimental main repository
    ExperimentalMain,
    /// Experimental contrib repository
    ExperimentalContrib,
    /// Experimental non-free repository
    ExperimentalNonFree,

    // === Oldstable (bullseye) ===
    /// Oldstable main repository
    OldstableMain,
    /// Oldstable contrib repository
    OldstableContrib,
    /// Oldstable non-free repository
    OldstableNonFree,
}

impl AptRepo {
    /// Get the distribution and component parts.
    fn parts(&self) -> (&'static str, &'static str) {
        match self {
            Self::StableMain => ("stable", "main"),
            Self::StableContrib => ("stable", "contrib"),
            Self::StableNonFree => ("stable", "non-free"),
            Self::StableNonFreeFirmware => ("stable", "non-free-firmware"),

            Self::StableBackportsMain => ("stable-backports", "main"),
            Self::StableBackportsContrib => ("stable-backports", "contrib"),
            Self::StableBackportsNonFree => ("stable-backports", "non-free"),

            Self::TestingMain => ("testing", "main"),
            Self::TestingContrib => ("testing", "contrib"),
            Self::TestingNonFree => ("testing", "non-free"),
            Self::TestingNonFreeFirmware => ("testing", "non-free-firmware"),

            Self::UnstableMain => ("unstable", "main"),
            Self::UnstableContrib => ("unstable", "contrib"),
            Self::UnstableNonFree => ("unstable", "non-free"),
            Self::UnstableNonFreeFirmware => ("unstable", "non-free-firmware"),

            Self::ExperimentalMain => ("experimental", "main"),
            Self::ExperimentalContrib => ("experimental", "contrib"),
            Self::ExperimentalNonFree => ("experimental", "non-free"),

            Self::OldstableMain => ("oldstable", "main"),
            Self::OldstableContrib => ("oldstable", "contrib"),
            Self::OldstableNonFree => ("oldstable", "non-free"),
        }
    }

    /// Get the Packages.gz URL for this repository.
    fn packages_url(&self) -> String {
        let (dist, component) = self.parts();
        format!(
            "{}/dists/{}/{}/binary-amd64/Packages.gz",
            DEBIAN_MIRROR, dist, component
        )
    }

    /// Get the repository name for tagging.
    pub fn name(&self) -> &'static str {
        match self {
            Self::StableMain => "stable-main",
            Self::StableContrib => "stable-contrib",
            Self::StableNonFree => "stable-non-free",
            Self::StableNonFreeFirmware => "stable-non-free-firmware",

            Self::StableBackportsMain => "stable-backports-main",
            Self::StableBackportsContrib => "stable-backports-contrib",
            Self::StableBackportsNonFree => "stable-backports-non-free",

            Self::TestingMain => "testing-main",
            Self::TestingContrib => "testing-contrib",
            Self::TestingNonFree => "testing-non-free",
            Self::TestingNonFreeFirmware => "testing-non-free-firmware",

            Self::UnstableMain => "unstable-main",
            Self::UnstableContrib => "unstable-contrib",
            Self::UnstableNonFree => "unstable-non-free",
            Self::UnstableNonFreeFirmware => "unstable-non-free-firmware",

            Self::ExperimentalMain => "experimental-main",
            Self::ExperimentalContrib => "experimental-contrib",
            Self::ExperimentalNonFree => "experimental-non-free",

            Self::OldstableMain => "oldstable-main",
            Self::OldstableContrib => "oldstable-contrib",
            Self::OldstableNonFree => "oldstable-non-free",
        }
    }

    /// All available repositories.
    pub fn all() -> &'static [AptRepo] {
        &[
            Self::StableMain,
            Self::StableContrib,
            Self::StableNonFree,
            Self::StableNonFreeFirmware,
            Self::StableBackportsMain,
            Self::StableBackportsContrib,
            Self::StableBackportsNonFree,
            Self::TestingMain,
            Self::TestingContrib,
            Self::TestingNonFree,
            Self::TestingNonFreeFirmware,
            Self::UnstableMain,
            Self::UnstableContrib,
            Self::UnstableNonFree,
            Self::UnstableNonFreeFirmware,
            Self::ExperimentalMain,
            Self::ExperimentalContrib,
            Self::ExperimentalNonFree,
            Self::OldstableMain,
            Self::OldstableContrib,
            Self::OldstableNonFree,
        ]
    }

    /// Stable repositories only.
    pub fn stable() -> &'static [AptRepo] {
        &[
            Self::StableMain,
            Self::StableContrib,
            Self::StableNonFree,
            Self::StableNonFreeFirmware,
            Self::StableBackportsMain,
            Self::StableBackportsContrib,
            Self::StableBackportsNonFree,
        ]
    }

    /// Testing repositories only.
    pub fn testing() -> &'static [AptRepo] {
        &[
            Self::TestingMain,
            Self::TestingContrib,
            Self::TestingNonFree,
            Self::TestingNonFreeFirmware,
        ]
    }

    /// Unstable repositories only.
    pub fn unstable() -> &'static [AptRepo] {
        &[
            Self::UnstableMain,
            Self::UnstableContrib,
            Self::UnstableNonFree,
            Self::UnstableNonFreeFirmware,
        ]
    }

    /// Free (main only) repositories.
    pub fn free() -> &'static [AptRepo] {
        &[
            Self::StableMain,
            Self::StableBackportsMain,
            Self::TestingMain,
            Self::UnstableMain,
            Self::ExperimentalMain,
            Self::OldstableMain,
        ]
    }

    /// Oldstable repositories only.
    pub fn oldstable() -> &'static [AptRepo] {
        &[
            Self::OldstableMain,
            Self::OldstableContrib,
            Self::OldstableNonFree,
        ]
    }
}

/// APT package index fetcher with configurable repositories.
pub struct Apt {
    repos: Vec<AptRepo>,
}

impl Apt {
    /// Create a fetcher with all repositories.
    pub fn all() -> Self {
        Self {
            repos: AptRepo::all().to_vec(),
        }
    }

    /// Create a fetcher with stable repositories only.
    pub fn stable() -> Self {
        Self {
            repos: AptRepo::stable().to_vec(),
        }
    }

    /// Create a fetcher with testing repositories only.
    pub fn testing() -> Self {
        Self {
            repos: AptRepo::testing().to_vec(),
        }
    }

    /// Create a fetcher with unstable repositories only.
    pub fn unstable() -> Self {
        Self {
            repos: AptRepo::unstable().to_vec(),
        }
    }

    /// Create a fetcher with free repositories only.
    pub fn free() -> Self {
        Self {
            repos: AptRepo::free().to_vec(),
        }
    }

    /// Create a fetcher with oldstable repositories only.
    pub fn oldstable() -> Self {
        Self {
            repos: AptRepo::oldstable().to_vec(),
        }
    }

    /// Create a fetcher with custom repository selection.
    pub fn with_repos(repos: &[AptRepo]) -> Self {
        Self {
            repos: repos.to_vec(),
        }
    }

    /// Parse a Packages file in Debian control format.
    fn parse_control<R: Read>(reader: R, repo: AptRepo) -> Vec<PackageMeta> {
        let reader = BufReader::new(reader);
        let mut packages = Vec::new();
        let mut current: Option<PackageBuilder> = None;

        for line in reader.lines().map_while(Result::ok) {
            if line.is_empty() {
                // End of stanza
                if let Some(builder) = current.take() {
                    if let Some(pkg) = builder.build(repo) {
                        packages.push(pkg);
                    }
                }
                continue;
            }

            if line.starts_with(' ') || line.starts_with('\t') {
                // Continuation line - skip for now
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

        // Handle last stanza
        if let Some(builder) = current {
            if let Some(pkg) = builder.build(repo) {
                packages.push(pkg);
            }
        }

        packages
    }

    /// Load packages from a single repository.
    fn load_repo(repo: AptRepo) -> Result<Vec<PackageMeta>, IndexError> {
        let url = repo.packages_url();

        let (data, _was_cached) = cache::fetch_with_cache(
            "apt",
            &format!("packages-{}", repo.name()),
            &url,
            INDEX_CACHE_TTL,
        )
        .map_err(IndexError::Network)?;

        // Check if data is gzip compressed
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
                    eprintln!("Warning: failed to load APT repo: {}", e);
                }
            }
        }

        Ok(packages)
    }

    /// Fetch raw repo data without parsing.
    fn fetch_repo_data(repo: AptRepo) -> Result<Vec<u8>, IndexError> {
        let url = repo.packages_url();
        let (data, _was_cached) = cache::fetch_with_cache(
            "apt",
            &format!("packages-{}", repo.name()),
            &url,
            INDEX_CACHE_TTL,
        )
        .map_err(IndexError::Network)?;
        Ok(data)
    }

    /// Load raw data for all configured repos (parallel fetch).
    fn load_repos_data(&self) -> Vec<(Vec<u8>, AptRepo)> {
        self.repos
            .par_iter()
            .filter_map(|&repo| Self::fetch_repo_data(repo).ok().map(|data| (data, repo)))
            .collect()
    }
}

impl PackageIndex for Apt {
    fn ecosystem(&self) -> &'static str {
        "apt"
    }

    fn display_name(&self) -> &'static str {
        "APT (Debian)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Use the API endpoint for single package lookup
        let url = format!(
            "https://sources.debian.org/api/src/{}/",
            urlencoding::encode(name)
        );

        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        if response.get("error").is_some() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        let versions = response["versions"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing versions".into()))?;

        let latest = versions
            .first()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        Ok(PackageMeta {
            name: name.to_string(),
            version: latest["version"].as_str().unwrap_or("unknown").to_string(),
            description: None,
            homepage: response["homepage"].as_str().map(String::from),
            repository: response["vcs_url"].as_str().map(String::from),
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
            "https://sources.debian.org/api/src/{}/",
            urlencoding::encode(name)
        );

        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        if response.get("error").is_some() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        let versions = response["versions"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing versions".into()))?;

        Ok(versions
            .iter()
            .map(|v| VersionMeta {
                version: v["version"].as_str().unwrap_or("unknown").to_string(),
                released: None,
                yanked: false,
            })
            .collect())
    }

    fn supports_fetch_all(&self) -> bool {
        true
    }

    fn fetch_all(&self) -> Result<Vec<PackageMeta>, IndexError> {
        self.load_packages()
    }

    fn iter_all(&self) -> Result<PackageIter<'_>, IndexError> {
        // Load raw data for all repos (parallel), then stream parse sequentially
        let repos_data = self.load_repos_data();
        if repos_data.is_empty() {
            return Err(IndexError::Network("Failed to load any APT repos".into()));
        }
        Ok(Box::new(AptPackageIter::new(repos_data)))
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        // Try loading from repos first
        let packages = self.load_packages()?;
        let query_lower = query.to_lowercase();

        let results: Vec<PackageMeta> = packages
            .into_iter()
            .filter(|pkg| {
                pkg.name.to_lowercase().contains(&query_lower)
                    || pkg
                        .description
                        .as_ref()
                        .map(|d| d.to_lowercase().contains(&query_lower))
                        .unwrap_or(false)
            })
            .collect();

        if !results.is_empty() {
            return Ok(results);
        }

        // Fall back to search API
        let url = format!(
            "https://sources.debian.org/api/search/{}/?suite=stable",
            urlencoding::encode(query)
        );

        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let api_results = response["results"]["exact"]
            .as_array()
            .or_else(|| response["results"]["other"].as_array())
            .ok_or_else(|| IndexError::Parse("missing results".into()))?;

        api_results
            .iter()
            .map(|r| {
                let name = r["name"].as_str().unwrap_or("").to_string();
                self.fetch(&name)
            })
            .collect()
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

    fn build(self, repo: AptRepo) -> Option<PackageMeta> {
        let mut extra = HashMap::new();

        // Store dependencies in extra
        if let Some(deps) = self.depends {
            let parsed_deps: Vec<String> = deps
                .split(',')
                .map(|d| {
                    // Strip version constraints: "libc6 (>= 2.17)" -> "libc6"
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

        // Store size in extra
        if let Some(size) = self.size {
            extra.insert("size".to_string(), serde_json::Value::Number(size.into()));
        }

        // Tag with source repo
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
            keywords: Vec::new(),
            maintainers: Vec::new(),
            published: None,
            downloads: None,
            archive_url: self.filename.map(|f| format!("{}/{}", DEBIAN_MIRROR, f)),
            checksum: self.sha256.map(|h| format!("sha256:{}", h)),
            extra,
        })
    }
}

/// Owning iterator over packages from multiple APT repos.
/// Holds the loaded data and iterates through repos sequentially.
pub struct AptPackageIter {
    /// Loaded repo data: (data bytes, repo enum)
    repos_data: Vec<(Vec<u8>, AptRepo)>,
    /// Current repo index
    current_repo_idx: usize,
    /// Current reader (Box to handle different reader types)
    current_reader: Option<Box<dyn BufRead + Send>>,
    /// Current repo being processed
    current_repo: Option<AptRepo>,
    /// Package builder for current stanza
    current_builder: Option<PackageBuilder>,
    /// Whether we've finished all repos
    done: bool,
}

impl AptPackageIter {
    fn new(repos_data: Vec<(Vec<u8>, AptRepo)>) -> Self {
        Self {
            repos_data,
            current_repo_idx: 0,
            current_reader: None,
            current_repo: None,
            current_builder: None,
            done: false,
        }
    }

    fn advance_to_next_repo(&mut self) -> bool {
        if self.current_repo_idx >= self.repos_data.len() {
            self.done = true;
            return false;
        }

        let (data, repo) = &self.repos_data[self.current_repo_idx];
        self.current_repo_idx += 1;
        self.current_repo = Some(*repo);

        // Create reader, handling gzip if needed
        let reader: Box<dyn BufRead + Send> =
            if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
                // Gzip compressed - decompress into memory first
                let mut decoder = GzDecoder::new(Cursor::new(data.clone()));
                let mut decompressed = Vec::new();
                if decoder.read_to_end(&mut decompressed).is_ok() {
                    Box::new(BufReader::new(Cursor::new(decompressed)))
                } else {
                    // Decompression failed, skip this repo
                    return self.advance_to_next_repo();
                }
            } else {
                Box::new(BufReader::new(Cursor::new(data.clone())))
            };

        self.current_reader = Some(reader);
        true
    }
}

impl Iterator for AptPackageIter {
    type Item = Result<PackageMeta, IndexError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.done {
                return None;
            }

            // Initialize first repo if needed
            if self.current_reader.is_none() && !self.advance_to_next_repo() {
                return None;
            }

            let reader = self.current_reader.as_mut()?;
            let repo = self.current_repo?;

            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    // EOF for this repo - flush builder and move to next
                    if let Some(builder) = self.current_builder.take() {
                        if let Some(pkg) = builder.build(repo) {
                            // Move to next repo before returning
                            self.current_reader = None;
                            return Some(Ok(pkg));
                        }
                    }
                    // No package to yield, advance to next repo
                    if !self.advance_to_next_repo() {
                        return None;
                    }
                    continue;
                }
                Ok(_) => {
                    let line = line.trim_end_matches('\n');

                    if line.is_empty() {
                        // End of stanza - yield package if complete
                        if let Some(builder) = self.current_builder.take() {
                            if let Some(pkg) = builder.build(repo) {
                                return Some(Ok(pkg));
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
                        let builder = self.current_builder.get_or_insert_with(PackageBuilder::new);

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
                Err(e) => {
                    self.done = true;
                    return Some(Err(IndexError::Io(e)));
                }
            }
        }
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
