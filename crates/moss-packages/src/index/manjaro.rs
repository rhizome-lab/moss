//! Manjaro Linux package index fetcher.
//!
//! Fetches package metadata from Manjaro repositories.
//!
//! ## API Strategy
//! - **fetch**: Parses repo databases from `repo.manjaro.org`
//! - **fetch_versions**: Loads from all configured repos
//! - **search**: Filters cached repo data + AUR
//! - **fetch_all**: Loads from all configured repo databases
//!
//! ## Multi-repo Support
//! ```rust,ignore
//! use moss_packages::index::manjaro::{Manjaro, ManjaroRepo};
//!
//! // All repos (default)
//! let all = Manjaro::all();
//!
//! // Stable branch only
//! let stable = Manjaro::stable();
//!
//! // Testing branch
//! let testing = Manjaro::testing();
//!
//! // Custom selection
//! let custom = Manjaro::with_repos(&[ManjaroRepo::StableCore, ManjaroRepo::StableExtra]);
//! ```

use super::arch_common;
use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use crate::cache;
use flate2::read::GzDecoder;
use rayon::prelude::*;
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::time::Duration;
use tar::Archive;

/// Cache TTL for repo databases (1 hour).
const CACHE_TTL: Duration = Duration::from_secs(60 * 60);

/// Manjaro mirror URL.
const MANJARO_MIRROR: &str = "https://repo.manjaro.org/repo";

/// Available Manjaro repositories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ManjaroRepo {
    // === Stable ===
    /// Stable core repository
    StableCore,
    /// Stable extra repository
    StableExtra,
    /// Stable multilib repository
    StableMultilib,

    // === Testing ===
    /// Testing core repository
    TestingCore,
    /// Testing extra repository
    TestingExtra,
    /// Testing multilib repository
    TestingMultilib,

    // === Unstable ===
    /// Unstable core repository
    UnstableCore,
    /// Unstable extra repository
    UnstableExtra,
    /// Unstable multilib repository
    UnstableMultilib,

    // === AUR ===
    /// Arch User Repository
    Aur,
}

impl ManjaroRepo {
    /// Get the branch and repo parts.
    fn parts(&self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::StableCore => Some(("stable", "core")),
            Self::StableExtra => Some(("stable", "extra")),
            Self::StableMultilib => Some(("stable", "multilib")),
            Self::TestingCore => Some(("testing", "core")),
            Self::TestingExtra => Some(("testing", "extra")),
            Self::TestingMultilib => Some(("testing", "multilib")),
            Self::UnstableCore => Some(("unstable", "core")),
            Self::UnstableExtra => Some(("unstable", "extra")),
            Self::UnstableMultilib => Some(("unstable", "multilib")),
            Self::Aur => None,
        }
    }

    /// Get the database URL for this repository.
    fn db_url(&self) -> Option<String> {
        let (branch, repo) = self.parts()?;
        Some(format!(
            "{}/{}/{}/x86_64/{}.db",
            MANJARO_MIRROR, branch, repo, repo
        ))
    }

    /// Get the repository name for tagging.
    pub fn name(&self) -> &'static str {
        match self {
            Self::StableCore => "stable-core",
            Self::StableExtra => "stable-extra",
            Self::StableMultilib => "stable-multilib",
            Self::TestingCore => "testing-core",
            Self::TestingExtra => "testing-extra",
            Self::TestingMultilib => "testing-multilib",
            Self::UnstableCore => "unstable-core",
            Self::UnstableExtra => "unstable-extra",
            Self::UnstableMultilib => "unstable-multilib",
            Self::Aur => "aur",
        }
    }

    /// All available repositories.
    pub fn all() -> &'static [ManjaroRepo] {
        &[
            Self::StableCore,
            Self::StableExtra,
            Self::StableMultilib,
            Self::TestingCore,
            Self::TestingExtra,
            Self::TestingMultilib,
            Self::UnstableCore,
            Self::UnstableExtra,
            Self::UnstableMultilib,
            Self::Aur,
        ]
    }

    /// Stable repositories only.
    pub fn stable() -> &'static [ManjaroRepo] {
        &[
            Self::StableCore,
            Self::StableExtra,
            Self::StableMultilib,
            Self::Aur,
        ]
    }

    /// Testing repositories only.
    pub fn testing() -> &'static [ManjaroRepo] {
        &[Self::TestingCore, Self::TestingExtra, Self::TestingMultilib]
    }

    /// Unstable repositories only.
    pub fn unstable() -> &'static [ManjaroRepo] {
        &[
            Self::UnstableCore,
            Self::UnstableExtra,
            Self::UnstableMultilib,
        ]
    }

    /// Official repos only (no AUR).
    pub fn official() -> &'static [ManjaroRepo] {
        &[
            Self::StableCore,
            Self::StableExtra,
            Self::StableMultilib,
            Self::TestingCore,
            Self::TestingExtra,
            Self::TestingMultilib,
            Self::UnstableCore,
            Self::UnstableExtra,
            Self::UnstableMultilib,
        ]
    }
}

/// Manjaro Linux package index fetcher with configurable repositories.
pub struct Manjaro {
    repos: Vec<ManjaroRepo>,
}

impl Manjaro {
    /// Create a fetcher with all repositories.
    pub fn all() -> Self {
        Self {
            repos: ManjaroRepo::all().to_vec(),
        }
    }

    /// Create a fetcher with stable repositories only.
    pub fn stable() -> Self {
        Self {
            repos: ManjaroRepo::stable().to_vec(),
        }
    }

    /// Create a fetcher with testing repositories only.
    pub fn testing() -> Self {
        Self {
            repos: ManjaroRepo::testing().to_vec(),
        }
    }

    /// Create a fetcher with unstable repositories only.
    pub fn unstable() -> Self {
        Self {
            repos: ManjaroRepo::unstable().to_vec(),
        }
    }

    /// Create a fetcher with official repos only (no AUR).
    pub fn official() -> Self {
        Self {
            repos: ManjaroRepo::official().to_vec(),
        }
    }

    /// Create a fetcher with custom repository selection.
    pub fn with_repos(repos: &[ManjaroRepo]) -> Self {
        Self {
            repos: repos.to_vec(),
        }
    }

    /// Load packages from a single repository database.
    fn load_repo(repo: ManjaroRepo) -> Result<Vec<PackageMeta>, IndexError> {
        if repo == ManjaroRepo::Aur {
            return arch_common::fetch_all_aur();
        }

        let url = repo
            .db_url()
            .ok_or_else(|| IndexError::NotFound(format!("no db URL for {}", repo.name())))?;

        let (data, _was_cached) =
            cache::fetch_with_cache("manjaro", &format!("{}-db", repo.name()), &url, CACHE_TTL)
                .map_err(IndexError::Network)?;

        Self::parse_db(&data, repo)
    }

    /// Parse a Manjaro .db file (gzipped tar with desc files).
    fn parse_db(data: &[u8], repo: ManjaroRepo) -> Result<Vec<PackageMeta>, IndexError> {
        // Check if data is gzip compressed (magic bytes 0x1f 0x8b)
        let tar_data = if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
            let mut decoder = GzDecoder::new(Cursor::new(data));
            let mut decompressed = Vec::new();
            decoder
                .read_to_end(&mut decompressed)
                .map_err(IndexError::Io)?;
            decompressed
        } else {
            data.to_vec()
        };

        let mut archive = Archive::new(Cursor::new(tar_data));
        let mut packages = Vec::new();

        for entry in archive.entries().map_err(IndexError::Io)? {
            let mut entry = entry.map_err(IndexError::Io)?;
            let path = entry
                .path()
                .map_err(IndexError::Io)?
                .to_string_lossy()
                .to_string();

            if !path.ends_with("/desc") {
                continue;
            }

            let mut content = String::new();
            entry.read_to_string(&mut content).map_err(IndexError::Io)?;

            if let Some(pkg) = Self::parse_desc(&content, repo) {
                packages.push(pkg);
            }
        }

        Ok(packages)
    }

    /// Parse a desc file from a Manjaro database.
    fn parse_desc(content: &str, repo: ManjaroRepo) -> Option<PackageMeta> {
        let mut fields: HashMap<String, String> = HashMap::new();
        let mut current_key: Option<String> = None;
        let mut current_value = String::new();

        for line in content.lines() {
            if line.starts_with('%') && line.ends_with('%') {
                if let Some(key) = current_key.take() {
                    fields.insert(key, current_value.trim().to_string());
                }
                current_key = Some(line[1..line.len() - 1].to_string());
                current_value.clear();
            } else if current_key.is_some() {
                if !current_value.is_empty() {
                    current_value.push('\n');
                }
                current_value.push_str(line);
            }
        }
        if let Some(key) = current_key {
            fields.insert(key, current_value.trim().to_string());
        }

        let name = fields.get("NAME")?.clone();
        let version = fields.get("VERSION")?.clone();

        let mut extra = HashMap::new();

        // Extract dependencies
        if let Some(deps) = fields.get("DEPENDS") {
            let parsed_deps: Vec<serde_json::Value> = deps
                .lines()
                .filter(|l| !l.is_empty())
                .map(|d| {
                    let name = d
                        .split(|c| c == '>' || c == '<' || c == '=' || c == ':')
                        .next()
                        .unwrap_or(d);
                    serde_json::Value::String(name.to_string())
                })
                .collect();
            if !parsed_deps.is_empty() {
                extra.insert("depends".to_string(), serde_json::Value::Array(parsed_deps));
            }
        }

        // Extract size
        if let Some(size) = fields.get("CSIZE").and_then(|s| s.parse::<u64>().ok()) {
            extra.insert("size".to_string(), serde_json::Value::Number(size.into()));
        }

        // Tag with source repo
        extra.insert(
            "source_repo".to_string(),
            serde_json::Value::String(repo.name().to_string()),
        );

        // Build download URL
        let filename = fields.get("FILENAME")?;
        let (branch, repo_name) = repo.parts()?;
        let archive_url = Some(format!(
            "{}/{}/{}/x86_64/{}",
            MANJARO_MIRROR, branch, repo_name, filename
        ));

        // Extract checksum
        let checksum = fields
            .get("SHA256SUM")
            .map(|s| format!("sha256:{}", s))
            .or_else(|| fields.get("MD5SUM").map(|s| format!("md5:{}", s)));

        Some(PackageMeta {
            name,
            version,
            description: fields.get("DESC").cloned(),
            homepage: fields.get("URL").cloned(),
            repository: None,
            license: fields.get("LICENSE").cloned(),
            binaries: Vec::new(),
            keywords: Vec::new(),
            maintainers: Vec::new(),
            published: None,
            downloads: None,
            archive_url,
            checksum,
            extra,
        })
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
                    eprintln!("Warning: failed to load Manjaro repo: {}", e);
                }
            }
        }

        Ok(packages)
    }
}

impl PackageIndex for Manjaro {
    fn ecosystem(&self) -> &'static str {
        "manjaro"
    }

    fn display_name(&self) -> &'static str {
        "Manjaro Linux"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
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
