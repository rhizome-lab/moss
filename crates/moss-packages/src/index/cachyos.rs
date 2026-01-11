//! CachyOS package index fetcher.
//!
//! CachyOS is an Arch-based distro with performance-optimized packages.
//! Has its own repositories in addition to Arch repos and AUR.
//!
//! ## API Strategy
//! - **fetch**: CachyOS repos + Arch + AUR fallback
//! - **fetch_versions**: Same sources
//! - **search**: Filters cached repo data + Arch/AUR
//! - **fetch_all**: Parses CachyOS + Arch repo databases
//!
//! ## Multi-repo Support
//! ```rust,ignore
//! use moss_packages::index::cachyos::{CachyOs, CachyOsRepo};
//!
//! // All repos (default)
//! let all = CachyOs::all();
//!
//! // CachyOS repos only
//! let cachyos = CachyOs::cachyos_only();
//!
//! // With Arch repos
//! let with_arch = CachyOs::with_arch();
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

/// CachyOS mirror URL.
const CACHYOS_MIRROR: &str = "https://mirror.cachyos.org/repo";

/// Available CachyOS repositories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CachyOsRepo {
    // === CachyOS Repos ===
    /// CachyOS main repo (x86-64-v3 optimized)
    CachyOs,
    /// CachyOS v4 repo (x86-64-v4 optimized)
    CachyOsV4,
    /// CachyOS core repo
    CachyOsCore,
    /// CachyOS extra repo
    CachyOsExtra,

    // === Arch Repos (via CachyOS mirrors) ===
    /// Arch core
    Core,
    /// Arch extra
    Extra,
    /// Arch multilib
    Multilib,

    // === AUR ===
    /// Arch User Repository
    Aur,
}

impl CachyOsRepo {
    /// Get the database URL for this repository.
    fn db_url(&self) -> Option<String> {
        match self {
            Self::CachyOs => Some(format!("{}/x86_64_v3/cachyos/cachyos.db", CACHYOS_MIRROR)),
            Self::CachyOsV4 => Some(format!(
                "{}/x86_64_v4/cachyos-v4/cachyos-v4.db",
                CACHYOS_MIRROR
            )),
            Self::CachyOsCore => Some(format!(
                "{}/x86_64_v3/cachyos-core-v3/cachyos-core-v3.db",
                CACHYOS_MIRROR
            )),
            Self::CachyOsExtra => Some(format!(
                "{}/x86_64_v3/cachyos-extra-v3/cachyos-extra-v3.db",
                CACHYOS_MIRROR
            )),
            Self::Core => Some(format!("{}/x86_64/core/core.db", CACHYOS_MIRROR)),
            Self::Extra => Some(format!("{}/x86_64/extra/extra.db", CACHYOS_MIRROR)),
            Self::Multilib => Some(format!("{}/x86_64/multilib/multilib.db", CACHYOS_MIRROR)),
            Self::Aur => None,
        }
    }

    /// Get the repository name for tagging.
    pub fn name(&self) -> &'static str {
        match self {
            Self::CachyOs => "cachyos",
            Self::CachyOsV4 => "cachyos-v4",
            Self::CachyOsCore => "cachyos-core",
            Self::CachyOsExtra => "cachyos-extra",
            Self::Core => "core",
            Self::Extra => "extra",
            Self::Multilib => "multilib",
            Self::Aur => "aur",
        }
    }

    /// All available repositories.
    pub fn all() -> &'static [CachyOsRepo] {
        &[
            Self::CachyOs,
            Self::CachyOsV4,
            Self::CachyOsCore,
            Self::CachyOsExtra,
            Self::Core,
            Self::Extra,
            Self::Multilib,
            Self::Aur,
        ]
    }

    /// CachyOS-specific repos only.
    pub fn cachyos_only() -> &'static [CachyOsRepo] {
        &[
            Self::CachyOs,
            Self::CachyOsV4,
            Self::CachyOsCore,
            Self::CachyOsExtra,
        ]
    }

    /// Arch-compatible repos via CachyOS mirrors.
    pub fn arch() -> &'static [CachyOsRepo] {
        &[Self::Core, Self::Extra, Self::Multilib, Self::Aur]
    }

    /// Stable repos (CachyOS main + Arch).
    pub fn stable() -> &'static [CachyOsRepo] {
        &[
            Self::CachyOs,
            Self::CachyOsCore,
            Self::CachyOsExtra,
            Self::Core,
            Self::Extra,
            Self::Multilib,
            Self::Aur,
        ]
    }
}

/// CachyOS package index fetcher with configurable repositories.
pub struct CachyOs {
    repos: Vec<CachyOsRepo>,
}

impl CachyOs {
    const ARCH_API: &'static str = "https://archlinux.org/packages/search/json/";
    const AUR_RPC: &'static str = "https://aur.archlinux.org/rpc/";

    /// Create a fetcher with all repositories.
    pub fn all() -> Self {
        Self {
            repos: CachyOsRepo::all().to_vec(),
        }
    }

    /// Create a fetcher with CachyOS repos only.
    pub fn cachyos_only() -> Self {
        Self {
            repos: CachyOsRepo::cachyos_only().to_vec(),
        }
    }

    /// Create a fetcher with Arch repos.
    pub fn arch() -> Self {
        Self {
            repos: CachyOsRepo::arch().to_vec(),
        }
    }

    /// Create a fetcher with stable repos.
    pub fn stable() -> Self {
        Self {
            repos: CachyOsRepo::stable().to_vec(),
        }
    }

    /// Create a fetcher with custom repository selection.
    pub fn with_repos(repos: &[CachyOsRepo]) -> Self {
        Self {
            repos: repos.to_vec(),
        }
    }

    /// Load packages from a single repository database.
    fn load_repo(repo: CachyOsRepo) -> Result<Vec<PackageMeta>, IndexError> {
        if repo == CachyOsRepo::Aur {
            return arch_common::fetch_all_aur();
        }

        let url = repo
            .db_url()
            .ok_or_else(|| IndexError::NotFound(format!("no db URL for {}", repo.name())))?;

        let (data, _was_cached) =
            cache::fetch_with_cache("cachyos", &format!("{}-db", repo.name()), &url, CACHE_TTL)
                .map_err(IndexError::Network)?;

        Self::parse_db(&data, repo)
    }

    /// Parse an Arch-style .db file.
    fn parse_db(data: &[u8], repo: CachyOsRepo) -> Result<Vec<PackageMeta>, IndexError> {
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

    /// Parse a desc file from a database.
    fn parse_desc(content: &str, repo: CachyOsRepo) -> Option<PackageMeta> {
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
        extra.insert(
            "source_repo".to_string(),
            serde_json::Value::String(repo.name().to_string()),
        );

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

        if let Some(size) = fields.get("CSIZE").and_then(|s| s.parse::<u64>().ok()) {
            extra.insert("size".to_string(), serde_json::Value::Number(size.into()));
        }

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
            archive_url: None,
            checksum: fields
                .get("SHA256SUM")
                .map(|s| format!("sha256:{}", s))
                .or_else(|| fields.get("MD5SUM").map(|s| format!("md5:{}", s))),
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
                    eprintln!("Warning: failed to load CachyOS repo: {}", e);
                }
            }
        }

        Ok(packages)
    }
}

impl PackageIndex for CachyOs {
    fn ecosystem(&self) -> &'static str {
        "cachyos"
    }

    fn display_name(&self) -> &'static str {
        "CachyOS"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Try CachyOS repos first, then fall back to Arch API + AUR
        let packages = self.load_packages()?;
        if let Some(pkg) = packages.into_iter().find(|p| p.name == name) {
            return Ok(pkg);
        }

        arch_common::fetch_official(Self::ARCH_API, name)
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
        let packages = self.load_packages()?;
        let query_lower = query.to_lowercase();

        let mut results: Vec<_> = packages
            .into_iter()
            .filter(|p| {
                p.name.to_lowercase().contains(&query_lower)
                    || p.description
                        .as_ref()
                        .map(|d| d.to_lowercase().contains(&query_lower))
                        .unwrap_or(false)
            })
            .collect();

        // Also search Arch API if AUR is included
        if self.repos.contains(&CachyOsRepo::Aur) {
            if let Ok(arch_packages) = arch_common::search_official(Self::ARCH_API, query) {
                results.extend(arch_packages);
            }
            if let Ok(aur_packages) = arch_common::search_aur(Self::AUR_RPC, query) {
                results.extend(aur_packages);
            }
        }

        Ok(results)
    }
}
