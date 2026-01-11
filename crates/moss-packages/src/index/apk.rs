//! APK package index fetcher (Alpine Linux).
//!
//! Fetches package metadata from Alpine Linux repositories by parsing
//! APKINDEX.tar.gz files from mirrors.
//!
//! ## API Strategy
//! - **fetch**: Parses APKINDEX.tar.gz from `dl-cdn.alpinelinux.org` (official mirror)
//! - **fetch_versions**: Loads from all configured repos
//! - **search**: Filters cached APKINDEX entries
//! - **fetch_all**: Returns all packages from APKINDEX (cached 1 hour)
//!
//! ## Multi-repo Support
//! ```rust,ignore
//! use moss_packages::index::apk::{Apk, AlpineRepo};
//!
//! // All repos (default)
//! let all = Apk::all();
//!
//! // Edge only
//! let edge = Apk::edge();
//!
//! // Specific version
//! let v321 = Apk::version("v3.21");
//!
//! // Custom selection
//! let custom = Apk::with_repos(&[AlpineRepo::EdgeMain, AlpineRepo::EdgeCommunity]);
//! ```

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use crate::cache;
use flate2::read::MultiGzDecoder;
use rayon::prelude::*;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Cursor, Read};
use std::time::Duration;
use tar::Archive;

/// Cache TTL for APKINDEX (1 hour).
const INDEX_CACHE_TTL: Duration = Duration::from_secs(60 * 60);

/// Alpine mirror URL.
const ALPINE_MIRROR: &str = "https://dl-cdn.alpinelinux.org/alpine";

/// Available Alpine Linux repositories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlpineRepo {
    // === Edge (rolling release) ===
    /// Edge main repository
    EdgeMain,
    /// Edge community repository
    EdgeCommunity,
    /// Edge testing repository (unstable)
    EdgeTesting,

    // === v3.21 ===
    /// Alpine 3.21 main repository
    V321Main,
    /// Alpine 3.21 community repository
    V321Community,

    // === v3.20 ===
    /// Alpine 3.20 main repository
    V320Main,
    /// Alpine 3.20 community repository
    V320Community,

    // === v3.19 ===
    /// Alpine 3.19 main repository
    V319Main,
    /// Alpine 3.19 community repository
    V319Community,

    // === v3.18 ===
    /// Alpine 3.18 main repository
    V318Main,
    /// Alpine 3.18 community repository
    V318Community,
}

impl AlpineRepo {
    /// Get the branch and repo parts.
    fn parts(&self) -> (&'static str, &'static str) {
        match self {
            Self::EdgeMain => ("edge", "main"),
            Self::EdgeCommunity => ("edge", "community"),
            Self::EdgeTesting => ("edge", "testing"),
            Self::V321Main => ("v3.21", "main"),
            Self::V321Community => ("v3.21", "community"),
            Self::V320Main => ("v3.20", "main"),
            Self::V320Community => ("v3.20", "community"),
            Self::V319Main => ("v3.19", "main"),
            Self::V319Community => ("v3.19", "community"),
            Self::V318Main => ("v3.18", "main"),
            Self::V318Community => ("v3.18", "community"),
        }
    }

    /// Get the repository name for tagging.
    pub fn name(&self) -> String {
        let (branch, repo) = self.parts();
        format!("{}-{}", branch, repo)
    }

    /// All available repositories.
    pub fn all() -> &'static [AlpineRepo] {
        &[
            Self::EdgeMain,
            Self::EdgeCommunity,
            Self::EdgeTesting,
            Self::V321Main,
            Self::V321Community,
            Self::V320Main,
            Self::V320Community,
            Self::V319Main,
            Self::V319Community,
            Self::V318Main,
            Self::V318Community,
        ]
    }

    /// Edge repositories only.
    pub fn edge() -> &'static [AlpineRepo] {
        &[Self::EdgeMain, Self::EdgeCommunity, Self::EdgeTesting]
    }

    /// Latest stable version (v3.21).
    pub fn latest_stable() -> &'static [AlpineRepo] {
        &[Self::V321Main, Self::V321Community]
    }

    /// Stable versions only (no edge, no testing).
    pub fn stable() -> &'static [AlpineRepo] {
        &[
            Self::V321Main,
            Self::V321Community,
            Self::V320Main,
            Self::V320Community,
            Self::V319Main,
            Self::V319Community,
            Self::V318Main,
            Self::V318Community,
        ]
    }
}

/// APK package index fetcher with configurable repositories.
pub struct Apk {
    repos: Vec<AlpineRepo>,
    arch: &'static str,
}

impl Apk {
    /// Create a fetcher with all repositories.
    pub fn all() -> Self {
        Self {
            repos: AlpineRepo::all().to_vec(),
            arch: "x86_64",
        }
    }

    /// Create a fetcher with edge repositories only.
    pub fn edge() -> Self {
        Self {
            repos: AlpineRepo::edge().to_vec(),
            arch: "x86_64",
        }
    }

    /// Create a fetcher with the latest stable version.
    pub fn latest_stable() -> Self {
        Self {
            repos: AlpineRepo::latest_stable().to_vec(),
            arch: "x86_64",
        }
    }

    /// Create a fetcher with all stable versions.
    pub fn stable() -> Self {
        Self {
            repos: AlpineRepo::stable().to_vec(),
            arch: "x86_64",
        }
    }

    /// Create a fetcher with custom repository selection.
    pub fn with_repos(repos: &[AlpineRepo]) -> Self {
        Self {
            repos: repos.to_vec(),
            arch: "x86_64",
        }
    }

    /// Set the architecture.
    pub fn with_arch(mut self, arch: &'static str) -> Self {
        self.arch = arch;
        self
    }

    /// Parse APKINDEX format into PackageMeta entries.
    fn parse_apkindex<R: Read>(reader: R, repo: AlpineRepo) -> Vec<PackageMeta> {
        let reader = BufReader::new(reader);
        let mut packages = Vec::new();
        let mut current = ApkPackageBuilder::new(repo);

        for line in reader.lines().map_while(Result::ok) {
            if line.is_empty() {
                // End of stanza
                if let Some(pkg) = current.build() {
                    packages.push(pkg);
                }
                current = ApkPackageBuilder::new(repo);
                continue;
            }

            // Single-letter field format: "X:value"
            if line.len() >= 2 && line.chars().nth(1) == Some(':') {
                let key = line.chars().next().unwrap();
                let value = &line[2..];

                match key {
                    'P' => current.name = Some(value.to_string()),
                    'V' => current.version = Some(value.to_string()),
                    'T' => current.description = Some(value.to_string()),
                    'U' => current.homepage = Some(value.to_string()),
                    'L' => current.license = Some(value.to_string()),
                    'S' => current.size = value.parse().ok(),
                    'C' => current.checksum = Some(value.to_string()),
                    'D' => current.depends = Some(value.to_string()),
                    'm' => current.maintainer = Some(value.to_string()),
                    'o' => current.origin = Some(value.to_string()),
                    'A' => current.arch = Some(value.to_string()),
                    'p' => current.provides = Some(value.to_string()),
                    _ => {}
                }
            }
        }

        // Handle last stanza
        if let Some(pkg) = current.build() {
            packages.push(pkg);
        }

        packages
    }

    /// Fetch and parse APKINDEX.tar.gz from a repository.
    fn load_repo(&self, repo: AlpineRepo) -> Result<Vec<PackageMeta>, IndexError> {
        let (branch, repo_name) = repo.parts();
        let url = format!(
            "{}/{}/{}/{}/APKINDEX.tar.gz",
            ALPINE_MIRROR, branch, repo_name, self.arch
        );

        // Try cache first
        let (data, _was_cached) = cache::fetch_with_cache(
            "apk",
            &format!("apkindex-{}-{}-{}", branch, repo_name, self.arch),
            &url,
            INDEX_CACHE_TTL,
        )
        .map_err(|e| IndexError::Network(e))?;

        // Check if data is gzip compressed
        let tar_data = if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
            let mut decoder = MultiGzDecoder::new(Cursor::new(data));
            let mut decompressed = Vec::new();
            decoder
                .read_to_end(&mut decompressed)
                .map_err(|e| IndexError::Io(e))?;
            decompressed
        } else {
            data
        };

        let mut archive = Archive::new(Cursor::new(tar_data));

        for entry in archive.entries().map_err(|e| IndexError::Io(e))? {
            let mut entry = entry.map_err(|e| IndexError::Io(e))?;
            let path = entry
                .path()
                .map_err(|e| IndexError::Io(e))?
                .to_string_lossy()
                .to_string();

            // Read entry content - must consume it to advance the iterator
            let mut content = Vec::new();
            entry
                .read_to_end(&mut content)
                .map_err(|e| IndexError::Io(e))?;

            if path == "APKINDEX" {
                return Ok(Self::parse_apkindex(Cursor::new(content), repo));
            }
        }

        Err(IndexError::Parse("APKINDEX not found in archive".into()))
    }

    /// Load packages from all configured repositories in parallel.
    fn load_packages(&self) -> Result<Vec<PackageMeta>, IndexError> {
        let results: Vec<_> = self
            .repos
            .par_iter()
            .map(|&repo| self.load_repo(repo))
            .collect();

        let mut packages = Vec::new();
        for result in results {
            match result {
                Ok(pkgs) => packages.extend(pkgs),
                Err(e) => {
                    eprintln!("Warning: failed to load Alpine repo: {}", e);
                }
            }
        }

        Ok(packages)
    }
}

impl PackageIndex for Apk {
    fn ecosystem(&self) -> &'static str {
        "apk"
    }

    fn display_name(&self) -> &'static str {
        "APK (Alpine Linux)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Search in all configured repos
        let packages = self.load_packages()?;

        packages
            .into_iter()
            .find(|p| p.name == name)
            .ok_or_else(|| IndexError::NotFound(name.to_string()))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let packages = self.load_packages()?;

        let versions: Vec<_> = packages
            .into_iter()
            .filter(|p| p.name == name)
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
            .filter(|p| {
                p.name.to_lowercase().contains(&query_lower)
                    || p.description
                        .as_ref()
                        .map(|d| d.to_lowercase().contains(&query_lower))
                        .unwrap_or(false)
            })
            .collect())
    }
}

/// Builder for APK package metadata.
#[derive(Default)]
struct ApkPackageBuilder {
    repo: Option<AlpineRepo>,
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
    homepage: Option<String>,
    license: Option<String>,
    size: Option<u64>,
    checksum: Option<String>,
    depends: Option<String>,
    maintainer: Option<String>,
    origin: Option<String>,
    arch: Option<String>,
    provides: Option<String>,
}

impl ApkPackageBuilder {
    fn new(repo: AlpineRepo) -> Self {
        Self {
            repo: Some(repo),
            ..Default::default()
        }
    }

    fn build(self) -> Option<PackageMeta> {
        let name = self.name?;
        let version = self.version?;
        let repo = self.repo?;
        let (branch, repo_name) = repo.parts();

        let mut extra = HashMap::new();

        // Parse dependencies
        if let Some(deps) = self.depends {
            let parsed_deps: Vec<serde_json::Value> = deps
                .split_whitespace()
                .filter(|d| {
                    // Filter out so: dependencies (shared library deps)
                    !d.starts_with("so:")
                })
                .map(|d| {
                    // Strip version constraints and prefixes
                    let name = d
                        .split(|c| c == '>' || c == '<' || c == '=' || c == '~')
                        .next()
                        .unwrap_or(d);
                    serde_json::Value::String(name.to_string())
                })
                .collect();
            if !parsed_deps.is_empty() {
                extra.insert("depends".to_string(), serde_json::Value::Array(parsed_deps));
            }
        }

        // Store size
        if let Some(size) = self.size {
            extra.insert("size".to_string(), serde_json::Value::Number(size.into()));
        }

        // Store origin package
        if let Some(origin) = self.origin {
            extra.insert("origin".to_string(), serde_json::Value::String(origin));
        }

        // Tag with source repo
        extra.insert(
            "source_repo".to_string(),
            serde_json::Value::String(repo.name()),
        );

        // Build download URL
        let archive_url = Some(format!(
            "{}/{}/{}/x86_64/{}-{}.apk",
            ALPINE_MIRROR, branch, repo_name, name, version
        ));

        // Convert checksum (Q1... is SHA1 in base64)
        let checksum = self.checksum.map(|c| {
            if c.starts_with("Q1") {
                format!("sha1-base64:{}", &c[2..])
            } else {
                c
            }
        });

        Some(PackageMeta {
            name,
            version,
            description: self.description,
            homepage: self.homepage,
            repository: None,
            license: self.license,
            binaries: Vec::new(),
            maintainers: self.maintainer.into_iter().collect(),
            archive_url,
            checksum,
            extra,
            ..Default::default()
        })
    }
}
