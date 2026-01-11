//! Pacman package index fetcher (Arch Linux).
//!
//! Fetches package metadata from Arch Linux repositories and AUR.
//!
//! ## API Strategy
//! - **fetch**: `archlinux.org/packages/search/json/` + AUR fallback - Official Arch JSON API
//! - **fetch_versions**: Same API, single version
//! - **search**: Same API with query parameter
//! - **fetch_all**: Parses repo databases (core.db, extra.db, etc.) + AUR archive
//!
//! ## Multi-repo Support
//! ```rust,ignore
//! use moss_packages::index::pacman::{Pacman, ArchRepo};
//!
//! // All repos (default)
//! let all = Pacman::all();
//!
//! // Stable repos only (core, extra, multilib)
//! let stable = Pacman::stable();
//!
//! // Testing repos
//! let testing = Pacman::testing();
//!
//! // Custom selection
//! let custom = Pacman::with_repos(&[ArchRepo::Core, ArchRepo::Extra]);
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

/// Available Arch Linux repositories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArchRepo {
    // === Stable ===
    /// Core repository (essential system packages)
    Core,
    /// Extra repository (additional official packages)
    Extra,
    /// Multilib repository (32-bit libraries for 64-bit systems)
    Multilib,

    // === Testing ===
    /// Core-testing (packages being tested before core)
    CoreTesting,
    /// Extra-testing (packages being tested before extra)
    ExtraTesting,
    /// Multilib-testing (packages being tested before multilib)
    MultilibTesting,

    // === Staging ===
    /// Core-staging (intermediate before testing)
    CoreStaging,
    /// Extra-staging (intermediate before testing)
    ExtraStaging,
    /// Multilib-staging (intermediate before testing)
    MultilibStaging,

    // === GNOME/KDE unstable ===
    /// GNOME unstable packages
    GnomeUnstable,
    /// KDE unstable packages
    KdeUnstable,

    // === AUR ===
    /// Arch User Repository (community packages)
    Aur,
}

impl ArchRepo {
    /// Get the repository name as used in URLs and metadata.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Extra => "extra",
            Self::Multilib => "multilib",
            Self::CoreTesting => "core-testing",
            Self::ExtraTesting => "extra-testing",
            Self::MultilibTesting => "multilib-testing",
            Self::CoreStaging => "core-staging",
            Self::ExtraStaging => "extra-staging",
            Self::MultilibStaging => "multilib-staging",
            Self::GnomeUnstable => "gnome-unstable",
            Self::KdeUnstable => "kde-unstable",
            Self::Aur => "aur",
        }
    }

    /// Get the database URL for this repository.
    fn db_url(&self) -> Option<&'static str> {
        match self {
            Self::Core => Some("https://mirror.rackspace.com/archlinux/core/os/x86_64/core.db"),
            Self::Extra => Some("https://mirror.rackspace.com/archlinux/extra/os/x86_64/extra.db"),
            Self::Multilib => {
                Some("https://mirror.rackspace.com/archlinux/multilib/os/x86_64/multilib.db")
            }
            Self::CoreTesting => Some(
                "https://mirror.rackspace.com/archlinux/core-testing/os/x86_64/core-testing.db",
            ),
            Self::ExtraTesting => Some(
                "https://mirror.rackspace.com/archlinux/extra-testing/os/x86_64/extra-testing.db",
            ),
            Self::MultilibTesting => Some(
                "https://mirror.rackspace.com/archlinux/multilib-testing/os/x86_64/multilib-testing.db",
            ),
            Self::CoreStaging => Some(
                "https://mirror.rackspace.com/archlinux/core-staging/os/x86_64/core-staging.db",
            ),
            Self::ExtraStaging => Some(
                "https://mirror.rackspace.com/archlinux/extra-staging/os/x86_64/extra-staging.db",
            ),
            Self::MultilibStaging => Some(
                "https://mirror.rackspace.com/archlinux/multilib-staging/os/x86_64/multilib-staging.db",
            ),
            Self::GnomeUnstable => Some(
                "https://mirror.rackspace.com/archlinux/gnome-unstable/os/x86_64/gnome-unstable.db",
            ),
            Self::KdeUnstable => Some(
                "https://mirror.rackspace.com/archlinux/kde-unstable/os/x86_64/kde-unstable.db",
            ),
            Self::Aur => None, // AUR uses JSON API, not .db files
        }
    }

    /// All available repositories.
    pub fn all() -> &'static [ArchRepo] {
        &[
            Self::Core,
            Self::Extra,
            Self::Multilib,
            Self::CoreTesting,
            Self::ExtraTesting,
            Self::MultilibTesting,
            Self::CoreStaging,
            Self::ExtraStaging,
            Self::MultilibStaging,
            Self::GnomeUnstable,
            Self::KdeUnstable,
            Self::Aur,
        ]
    }

    /// Stable repositories only.
    pub fn stable() -> &'static [ArchRepo] {
        &[Self::Core, Self::Extra, Self::Multilib, Self::Aur]
    }

    /// Testing repositories only.
    pub fn testing() -> &'static [ArchRepo] {
        &[Self::CoreTesting, Self::ExtraTesting, Self::MultilibTesting]
    }

    /// Official binary repos only (no AUR).
    pub fn official() -> &'static [ArchRepo] {
        &[
            Self::Core,
            Self::Extra,
            Self::Multilib,
            Self::CoreTesting,
            Self::ExtraTesting,
            Self::MultilibTesting,
            Self::CoreStaging,
            Self::ExtraStaging,
            Self::MultilibStaging,
            Self::GnomeUnstable,
            Self::KdeUnstable,
        ]
    }
}

/// Pacman package index fetcher with configurable repositories.
pub struct Pacman {
    repos: Vec<ArchRepo>,
}

impl Pacman {
    /// AUR RPC endpoint.
    const AUR_RPC: &'static str = "https://aur.archlinux.org/rpc/";

    /// Official package search API.
    const ARCH_API: &'static str = "https://archlinux.org/packages/search/json/";

    /// Create a fetcher with all repositories.
    pub fn all() -> Self {
        Self {
            repos: ArchRepo::all().to_vec(),
        }
    }

    /// Create a fetcher with stable repositories only (core, extra, multilib, AUR).
    pub fn stable() -> Self {
        Self {
            repos: ArchRepo::stable().to_vec(),
        }
    }

    /// Create a fetcher with testing repositories only.
    pub fn testing() -> Self {
        Self {
            repos: ArchRepo::testing().to_vec(),
        }
    }

    /// Create a fetcher with official binary repos only (no AUR).
    pub fn official() -> Self {
        Self {
            repos: ArchRepo::official().to_vec(),
        }
    }

    /// Create a fetcher with custom repository selection.
    pub fn with_repos(repos: &[ArchRepo]) -> Self {
        Self {
            repos: repos.to_vec(),
        }
    }

    /// Load packages from a single repository database.
    fn load_repo(repo: ArchRepo) -> Result<Vec<PackageMeta>, IndexError> {
        if repo == ArchRepo::Aur {
            return arch_common::fetch_all_aur();
        }

        let url = repo
            .db_url()
            .ok_or_else(|| IndexError::NotFound(format!("no db URL for {}", repo.name())))?;

        let (data, _was_cached) =
            cache::fetch_with_cache("pacman", &format!("{}-db", repo.name()), url, CACHE_TTL)
                .map_err(IndexError::Network)?;

        Self::parse_db(&data, repo)
    }

    /// Parse an Arch .db file (gzipped tar with desc files).
    /// Note: data may already be decompressed if ureq auto-decompressed based on Content-Encoding.
    fn parse_db(data: &[u8], repo: ArchRepo) -> Result<Vec<PackageMeta>, IndexError> {
        // Check if data is gzip compressed (magic bytes 0x1f 0x8b)
        let tar_data = if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
            // Data is gzip compressed - decompress it
            let mut decoder = GzDecoder::new(Cursor::new(data));
            let mut decompressed = Vec::new();
            decoder
                .read_to_end(&mut decompressed)
                .map_err(IndexError::Io)?;
            decompressed
        } else {
            // Data is already decompressed (ureq auto-decompressed based on Content-Encoding)
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

            // Only process desc files
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

    /// Parse a desc file from an Arch database.
    fn parse_desc(content: &str, repo: ArchRepo) -> Option<PackageMeta> {
        let mut fields: HashMap<String, String> = HashMap::new();
        let mut current_key: Option<String> = None;
        let mut current_value = String::new();

        for line in content.lines() {
            if line.starts_with('%') && line.ends_with('%') {
                // Save previous key-value pair
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
        // Save last key-value pair
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
                    // Strip version constraints
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
            "https://mirror.rackspace.com/archlinux/{}/os/x86_64/{}",
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
                    eprintln!("Warning: failed to load repo: {}", e);
                }
            }
        }

        Ok(packages)
    }
}

impl PackageIndex for Pacman {
    fn ecosystem(&self) -> &'static str {
        "pacman"
    }

    fn display_name(&self) -> &'static str {
        "Pacman (Arch Linux)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Try official repos first via API, then AUR
        arch_common::fetch_official(Self::ARCH_API, name)
            .or_else(|_| arch_common::fetch_aur(Self::AUR_RPC, name))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // Load from all configured repos to find all versions
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
        // Use API for search (faster than loading all packages)
        let mut packages = arch_common::search_official(Self::ARCH_API, query)?;

        // Also search AUR if included
        if self.repos.contains(&ArchRepo::Aur) {
            if let Ok(aur_packages) = arch_common::search_aur(Self::AUR_RPC, query) {
                packages.extend(aur_packages);
            }
        }

        Ok(packages)
    }
}
