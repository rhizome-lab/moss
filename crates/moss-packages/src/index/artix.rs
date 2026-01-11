//! Artix Linux package index fetcher.
//!
//! Fetches package metadata from Artix Linux repositories.
//! Artix is an Arch-based distro without systemd.
//!
//! ## API Strategy
//! - **fetch**: `packages.artixlinux.org/packages/search/json/` - Official JSON API
//! - **fetch_versions**: Loads from all repo databases
//! - **search**: Same API with query parameter
//! - **fetch_all**: Parses repo databases (system.db, world.db, etc.)
//!
//! ## Multi-repo Support
//! ```rust,ignore
//! use moss_packages::index::artix::{Artix, ArtixRepo};
//!
//! // All repos (default)
//! let all = Artix::all();
//!
//! // Stable repos only
//! let stable = Artix::stable();
//!
//! // Testing repos (gremlins/goblins)
//! let testing = Artix::testing();
//!
//! // Custom selection
//! let custom = Artix::with_repos(&[ArtixRepo::System, ArtixRepo::World]);
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

/// Artix mirror URL.
const ARTIX_MIRROR: &str = "https://mirror1.artixlinux.org/repos";

/// Available Artix Linux repositories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArtixRepo {
    // === Stable ===
    /// System repository (core packages, init systems)
    System,
    /// World repository (extra packages)
    World,
    /// Galaxy repository (community packages)
    Galaxy,
    /// Lib32 repository (32-bit libraries)
    Lib32,
    /// Asteroids repository (universe packages, similar to AUR)
    Asteroids,

    // === Gremlins (testing for system) ===
    /// System-gremlins (testing for system)
    SystemGremlins,
    /// World-gremlins (testing for world)
    WorldGremlins,
    /// Galaxy-gremlins (testing for galaxy)
    GalaxyGremlins,
    /// Lib32-gremlins (testing for lib32)
    Lib32Gremlins,
    /// Asteroids-gremlins (testing for asteroids)
    AsteroidsGremlins,

    // === Goblins (staging) ===
    /// System-goblins (staging for system)
    SystemGoblins,
    /// World-goblins (staging for world)
    WorldGoblins,
    /// Galaxy-goblins (staging for galaxy)
    GalaxyGoblins,
    /// Lib32-goblins (staging for lib32)
    Lib32Goblins,
    /// Asteroids-goblins (staging for asteroids)
    AsteroidsGoblins,
}

impl ArtixRepo {
    /// Get the repository name as used in URLs and metadata.
    pub fn name(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::World => "world",
            Self::Galaxy => "galaxy",
            Self::Lib32 => "lib32",
            Self::Asteroids => "asteroids",
            Self::SystemGremlins => "system-gremlins",
            Self::WorldGremlins => "world-gremlins",
            Self::GalaxyGremlins => "galaxy-gremlins",
            Self::Lib32Gremlins => "lib32-gremlins",
            Self::AsteroidsGremlins => "asteroids-gremlins",
            Self::SystemGoblins => "system-goblins",
            Self::WorldGoblins => "world-goblins",
            Self::GalaxyGoblins => "galaxy-goblins",
            Self::Lib32Goblins => "lib32-goblins",
            Self::AsteroidsGoblins => "asteroids-goblins",
        }
    }

    /// Get the database URL for this repository.
    fn db_url(&self) -> String {
        format!(
            "{}/{}/os/x86_64/{}.db",
            ARTIX_MIRROR,
            self.name(),
            self.name()
        )
    }

    /// All available repositories.
    pub fn all() -> &'static [ArtixRepo] {
        &[
            Self::System,
            Self::World,
            Self::Galaxy,
            Self::Lib32,
            Self::Asteroids,
            Self::SystemGremlins,
            Self::WorldGremlins,
            Self::GalaxyGremlins,
            Self::Lib32Gremlins,
            Self::AsteroidsGremlins,
            Self::SystemGoblins,
            Self::WorldGoblins,
            Self::GalaxyGoblins,
            Self::Lib32Goblins,
            Self::AsteroidsGoblins,
        ]
    }

    /// Stable repositories only.
    pub fn stable() -> &'static [ArtixRepo] {
        &[
            Self::System,
            Self::World,
            Self::Galaxy,
            Self::Lib32,
            Self::Asteroids,
        ]
    }

    /// Gremlins repositories (testing).
    pub fn gremlins() -> &'static [ArtixRepo] {
        &[
            Self::SystemGremlins,
            Self::WorldGremlins,
            Self::GalaxyGremlins,
            Self::Lib32Gremlins,
            Self::AsteroidsGremlins,
        ]
    }

    /// Goblins repositories (staging).
    pub fn goblins() -> &'static [ArtixRepo] {
        &[
            Self::SystemGoblins,
            Self::WorldGoblins,
            Self::GalaxyGoblins,
            Self::Lib32Goblins,
            Self::AsteroidsGoblins,
        ]
    }

    /// Testing repositories (gremlins + goblins).
    pub fn testing() -> Vec<ArtixRepo> {
        let mut repos = Self::gremlins().to_vec();
        repos.extend_from_slice(Self::goblins());
        repos
    }
}

/// Artix Linux package index fetcher with configurable repositories.
pub struct Artix {
    repos: Vec<ArtixRepo>,
}

impl Artix {
    /// Artix package search API.
    const ARTIX_API: &'static str = "https://packages.artixlinux.org/packages/search/json/";

    /// Arch AUR (Artix users can also use AUR packages).
    const AUR_RPC: &'static str = "https://aur.archlinux.org/rpc/";

    /// Create a fetcher with all repositories.
    pub fn all() -> Self {
        Self {
            repos: ArtixRepo::all().to_vec(),
        }
    }

    /// Create a fetcher with stable repositories only.
    pub fn stable() -> Self {
        Self {
            repos: ArtixRepo::stable().to_vec(),
        }
    }

    /// Create a fetcher with gremlins repositories only (testing).
    pub fn gremlins() -> Self {
        Self {
            repos: ArtixRepo::gremlins().to_vec(),
        }
    }

    /// Create a fetcher with goblins repositories only (staging).
    pub fn goblins() -> Self {
        Self {
            repos: ArtixRepo::goblins().to_vec(),
        }
    }

    /// Create a fetcher with all testing repos (gremlins + goblins).
    pub fn testing() -> Self {
        Self {
            repos: ArtixRepo::testing(),
        }
    }

    /// Create a fetcher with custom repository selection.
    pub fn with_repos(repos: &[ArtixRepo]) -> Self {
        Self {
            repos: repos.to_vec(),
        }
    }

    /// Load packages from a single repository database.
    fn load_repo(repo: ArtixRepo) -> Result<Vec<PackageMeta>, IndexError> {
        let url = repo.db_url();

        let (data, _was_cached) =
            cache::fetch_with_cache("artix", &format!("{}-db", repo.name()), &url, CACHE_TTL)
                .map_err(IndexError::Network)?;

        Self::parse_db(&data, repo)
    }

    /// Parse an Artix .db file (gzipped tar with desc files).
    fn parse_db(data: &[u8], repo: ArtixRepo) -> Result<Vec<PackageMeta>, IndexError> {
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

    /// Parse a desc file from an Artix database.
    fn parse_desc(content: &str, repo: ArtixRepo) -> Option<PackageMeta> {
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
        let archive_url = Some(format!(
            "{}/{}/os/x86_64/{}",
            ARTIX_MIRROR,
            repo.name(),
            filename
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
            archive_url,
            checksum,
            extra,
            ..Default::default()
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
                    eprintln!("Warning: failed to load Artix repo: {}", e);
                }
            }
        }

        Ok(packages)
    }
}

impl PackageIndex for Artix {
    fn ecosystem(&self) -> &'static str {
        "artix"
    }

    fn display_name(&self) -> &'static str {
        "Artix Linux"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Try Artix repos first, then fall back to AUR
        arch_common::fetch_official(Self::ARTIX_API, name)
            .or_else(|_| arch_common::fetch_aur(Self::AUR_RPC, name))
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
        // Search Artix repos
        let mut packages = arch_common::search_official(Self::ARTIX_API, query).unwrap_or_default();

        // Also search AUR
        if let Ok(aur_packages) = arch_common::search_aur(Self::AUR_RPC, query) {
            packages.extend(aur_packages);
        }

        Ok(packages)
    }
}
