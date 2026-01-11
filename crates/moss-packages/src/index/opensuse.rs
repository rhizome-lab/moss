//! openSUSE package index fetcher.
//!
//! Fetches package metadata from openSUSE repositories.
//!
//! ## API Strategy
//! - **fetch**: Searches configured repos for package (returns first match)
//! - **fetch_versions**: Returns all versions across configured repos
//! - **search**: Filters configured repos
//! - **fetch_all**: All packages from configured repos, tagged with source_repo in extra
//!
//! ## Configuration
//! ```rust,ignore
//! // All repos (default)
//! let index = OpenSuse::all();
//!
//! // Specific repos
//! let index = OpenSuse::with_repos(&[
//!     OpenSuseRepo::TumbleweedOss,
//!     OpenSuseRepo::Leap156Oss,
//! ]);
//!
//! // Just Tumbleweed
//! let index = OpenSuse::tumbleweed();
//! ```

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use crate::cache;
use rayon::prelude::*;
use std::collections::HashMap;
use std::time::Duration;

/// Cache TTL for openSUSE package index (1 hour).
const CACHE_TTL: Duration = Duration::from_secs(60 * 60);

/// Available openSUSE repositories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpenSuseRepo {
    // === Official: Tumbleweed ===
    /// Tumbleweed OSS (rolling release, open source)
    TumbleweedOss,
    /// Tumbleweed Non-OSS (rolling release, proprietary)
    TumbleweedNonOss,
    /// Tumbleweed Updates
    TumbleweedUpdate,

    // === Official: Leap 16.0 ===
    /// Leap 16.0 OSS
    Leap160Oss,
    /// Leap 16.0 Non-OSS
    Leap160NonOss,

    // === Official: Leap 15.6 ===
    /// Leap 15.6 OSS
    Leap156Oss,
    /// Leap 15.6 Non-OSS
    Leap156NonOss,
    /// Leap 15.6 Updates OSS
    Leap156UpdateOss,
    /// Leap 15.6 Updates Non-OSS
    Leap156UpdateNonOss,
    /// Leap 15.6 Backports
    Leap156Backports,
    /// Leap 15.6 SLE Updates
    Leap156Sle,

    // === Official: Source RPMs ===
    /// Tumbleweed Source OSS
    TumbleweedSrcOss,
    /// Tumbleweed Source Non-OSS
    TumbleweedSrcNonOss,
    /// Leap 15.6 Source OSS
    Leap156SrcOss,
    /// Leap 15.6 Source Non-OSS
    Leap156SrcNonOss,

    // === Official: Debug ===
    /// Tumbleweed Debug
    TumbleweedDebug,
    /// Leap 16.0 Debug
    Leap160Debug,
    /// Leap 15.6 Debug
    Leap156Debug,
    /// Leap 15.6 Debug Updates
    Leap156DebugUpdate,

    // === Factory (bleeding edge) ===
    /// Factory OSS (development, becomes Tumbleweed)
    FactoryOss,

    // === Community: Games ===
    /// Games for Tumbleweed
    GamesTumbleweed,
    /// Games for Leap 15.6
    GamesLeap156,

    // === Community: KDE ===
    /// KDE Extra for Tumbleweed
    KdeExtraTumbleweed,
    /// KDE Frameworks for Tumbleweed
    KdeFrameworksTumbleweed,
    /// KDE Qt6 for Tumbleweed
    KdeQt6Tumbleweed,

    // === Community: GNOME ===
    /// GNOME Apps for Leap 16.0
    GnomeAppsLeap160,
    /// GNOME Apps for Leap 15.6
    GnomeAppsLeap156,
    /// GNOME Factory
    GnomeFactory,

    // === Community: Desktop Environments ===
    /// Xfce for Tumbleweed
    XfceTumbleweed,
    /// Xfce for Leap 16.0
    XfceLeap160,
    /// Xfce for Leap 15.6
    XfceLeap156,

    // === Community: Server/Tools ===
    /// Mozilla (Firefox, Thunderbird) for Tumbleweed
    MozillaTumbleweed,
    /// Science packages for Tumbleweed
    ScienceTumbleweed,
    /// Wine for Tumbleweed
    WineTumbleweed,
    /// HTTP servers for Tumbleweed
    ServerHttpTumbleweed,
    /// Database servers for Tumbleweed
    ServerDatabaseTumbleweed,
}

impl OpenSuseRepo {
    /// Get repo identifier for caching.
    fn id(&self) -> &'static str {
        match self {
            // Official
            Self::TumbleweedOss => "tumbleweed-oss",
            Self::TumbleweedNonOss => "tumbleweed-non-oss",
            Self::TumbleweedUpdate => "tumbleweed-update",
            Self::Leap160Oss => "leap-16.0-oss",
            Self::Leap160NonOss => "leap-16.0-non-oss",
            Self::Leap156Oss => "leap-15.6-oss",
            Self::Leap156NonOss => "leap-15.6-non-oss",
            Self::Leap156UpdateOss => "leap-15.6-update-oss",
            Self::Leap156UpdateNonOss => "leap-15.6-update-non-oss",
            Self::Leap156Backports => "leap-15.6-backports",
            Self::Leap156Sle => "leap-15.6-sle",
            // Source
            Self::TumbleweedSrcOss => "tumbleweed-src-oss",
            Self::TumbleweedSrcNonOss => "tumbleweed-src-non-oss",
            Self::Leap156SrcOss => "leap-15.6-src-oss",
            Self::Leap156SrcNonOss => "leap-15.6-src-non-oss",
            // Debug
            Self::TumbleweedDebug => "tumbleweed-debug",
            Self::Leap160Debug => "leap-16.0-debug",
            Self::Leap156Debug => "leap-15.6-debug",
            Self::Leap156DebugUpdate => "leap-15.6-debug-update",
            Self::FactoryOss => "factory-oss",
            // Community
            Self::GamesTumbleweed => "games-tumbleweed",
            Self::GamesLeap156 => "games-leap-15.6",
            Self::KdeExtraTumbleweed => "kde-extra-tumbleweed",
            Self::KdeFrameworksTumbleweed => "kde-frameworks-tumbleweed",
            Self::KdeQt6Tumbleweed => "kde-qt6-tumbleweed",
            Self::GnomeAppsLeap160 => "gnome-apps-leap-16.0",
            Self::GnomeAppsLeap156 => "gnome-apps-leap-15.6",
            Self::GnomeFactory => "gnome-factory",
            Self::XfceTumbleweed => "xfce-tumbleweed",
            Self::XfceLeap160 => "xfce-leap-16.0",
            Self::XfceLeap156 => "xfce-leap-15.6",
            Self::MozillaTumbleweed => "mozilla-tumbleweed",
            Self::ScienceTumbleweed => "science-tumbleweed",
            Self::WineTumbleweed => "wine-tumbleweed",
            Self::ServerHttpTumbleweed => "server-http-tumbleweed",
            Self::ServerDatabaseTumbleweed => "server-database-tumbleweed",
        }
    }

    /// Get repodata base URL.
    fn base_url(&self) -> &'static str {
        match self {
            // Official main repos
            Self::TumbleweedOss => "https://download.opensuse.org/tumbleweed/repo/oss/repodata",
            Self::TumbleweedNonOss => {
                "https://download.opensuse.org/tumbleweed/repo/non-oss/repodata"
            }
            Self::TumbleweedUpdate => "https://download.opensuse.org/update/tumbleweed/repodata",
            Self::Leap160Oss => {
                "https://download.opensuse.org/distribution/leap/16.0/repo/oss/repodata"
            }
            Self::Leap160NonOss => {
                "https://download.opensuse.org/distribution/leap/16.0/repo/non-oss/repodata"
            }
            Self::Leap156Oss => {
                "https://download.opensuse.org/distribution/leap/15.6/repo/oss/repodata"
            }
            Self::Leap156NonOss => {
                "https://download.opensuse.org/distribution/leap/15.6/repo/non-oss/repodata"
            }
            Self::Leap156UpdateOss => "https://download.opensuse.org/update/leap/15.6/oss/repodata",
            Self::Leap156UpdateNonOss => {
                "https://download.opensuse.org/update/leap/15.6/non-oss/repodata"
            }
            Self::Leap156Backports => {
                "https://download.opensuse.org/update/leap/15.6/backports/repodata"
            }
            Self::Leap156Sle => "https://download.opensuse.org/update/leap/15.6/sle/repodata",
            // Source repos
            Self::TumbleweedSrcOss => {
                "https://download.opensuse.org/tumbleweed/repo/src-oss/repodata"
            }
            Self::TumbleweedSrcNonOss => {
                "https://download.opensuse.org/tumbleweed/repo/src-non-oss/repodata"
            }
            Self::Leap156SrcOss => {
                "https://download.opensuse.org/source/distribution/leap/15.6/repo/oss/repodata"
            }
            Self::Leap156SrcNonOss => {
                "https://download.opensuse.org/source/distribution/leap/15.6/repo/non-oss/repodata"
            }
            // Debug repos
            Self::TumbleweedDebug => {
                "https://download.opensuse.org/debug/tumbleweed/repo/oss/repodata"
            }
            Self::Leap160Debug => {
                "https://download.opensuse.org/debug/distribution/leap/16.0/repo/oss/repodata"
            }
            Self::Leap156Debug => {
                "https://download.opensuse.org/debug/distribution/leap/15.6/repo/oss/repodata"
            }
            Self::Leap156DebugUpdate => {
                "https://download.opensuse.org/debug/update/leap/15.6/oss/repodata"
            }
            Self::FactoryOss => "https://download.opensuse.org/factory/repo/oss/repodata",
            // Community repos
            Self::GamesTumbleweed => {
                "https://download.opensuse.org/repositories/games/openSUSE_Tumbleweed/repodata"
            }
            Self::GamesLeap156 => "https://download.opensuse.org/repositories/games/15.6/repodata",
            Self::KdeExtraTumbleweed => {
                "https://download.opensuse.org/repositories/KDE:/Extra/openSUSE_Tumbleweed/repodata"
            }
            Self::KdeFrameworksTumbleweed => {
                "https://download.opensuse.org/repositories/KDE:/Frameworks/openSUSE_Tumbleweed/repodata"
            }
            Self::KdeQt6Tumbleweed => {
                "https://download.opensuse.org/repositories/KDE:/Qt6/openSUSE_Tumbleweed/repodata"
            }
            Self::GnomeAppsLeap160 => {
                "https://download.opensuse.org/repositories/GNOME:/Apps/16.0/repodata"
            }
            Self::GnomeAppsLeap156 => {
                "https://download.opensuse.org/repositories/GNOME:/Apps/15.6/repodata"
            }
            Self::GnomeFactory => {
                "https://download.opensuse.org/repositories/GNOME:/Factory/openSUSE_Factory/repodata"
            }
            Self::XfceTumbleweed => {
                "https://download.opensuse.org/repositories/X11:/xfce/openSUSE_Tumbleweed/repodata"
            }
            Self::XfceLeap160 => {
                "https://download.opensuse.org/repositories/X11:/xfce/16.0/repodata"
            }
            Self::XfceLeap156 => {
                "https://download.opensuse.org/repositories/X11:/xfce/15.6/repodata"
            }
            Self::MozillaTumbleweed => {
                "https://download.opensuse.org/repositories/mozilla/openSUSE_Tumbleweed/repodata"
            }
            Self::ScienceTumbleweed => {
                "https://download.opensuse.org/repositories/science/openSUSE_Tumbleweed/repodata"
            }
            Self::WineTumbleweed => {
                "https://download.opensuse.org/repositories/Emulators:/Wine/openSUSE_Tumbleweed/repodata"
            }
            Self::ServerHttpTumbleweed => {
                "https://download.opensuse.org/repositories/server:/http/openSUSE_Tumbleweed/repodata"
            }
            Self::ServerDatabaseTumbleweed => {
                "https://download.opensuse.org/repositories/server:/database/openSUSE_Tumbleweed/repodata"
            }
        }
    }

    /// All available repos (official + community).
    pub fn all() -> &'static [OpenSuseRepo] {
        &[
            // Official binary
            Self::TumbleweedOss,
            Self::TumbleweedNonOss,
            Self::TumbleweedUpdate,
            Self::Leap160Oss,
            Self::Leap160NonOss,
            Self::Leap156Oss,
            Self::Leap156NonOss,
            Self::Leap156UpdateOss,
            Self::Leap156UpdateNonOss,
            Self::Leap156Backports,
            Self::Leap156Sle,
            // Source
            Self::TumbleweedSrcOss,
            Self::TumbleweedSrcNonOss,
            Self::Leap156SrcOss,
            Self::Leap156SrcNonOss,
            // Debug
            Self::TumbleweedDebug,
            Self::Leap160Debug,
            Self::Leap156Debug,
            Self::Leap156DebugUpdate,
            // Factory
            Self::FactoryOss,
            // Community
            Self::GamesTumbleweed,
            Self::GamesLeap156,
            Self::KdeExtraTumbleweed,
            Self::KdeFrameworksTumbleweed,
            Self::KdeQt6Tumbleweed,
            Self::GnomeAppsLeap160,
            Self::GnomeAppsLeap156,
            Self::GnomeFactory,
            Self::XfceTumbleweed,
            Self::XfceLeap160,
            Self::XfceLeap156,
            Self::MozillaTumbleweed,
            Self::ScienceTumbleweed,
            Self::WineTumbleweed,
            Self::ServerHttpTumbleweed,
            Self::ServerDatabaseTumbleweed,
        ]
    }

    /// Official repos only (no community).
    pub fn official() -> &'static [OpenSuseRepo] {
        &[
            // Binary
            Self::TumbleweedOss,
            Self::TumbleweedNonOss,
            Self::TumbleweedUpdate,
            Self::Leap160Oss,
            Self::Leap160NonOss,
            Self::Leap156Oss,
            Self::Leap156NonOss,
            Self::Leap156UpdateOss,
            Self::Leap156UpdateNonOss,
            Self::Leap156Backports,
            Self::Leap156Sle,
            // Source
            Self::TumbleweedSrcOss,
            Self::TumbleweedSrcNonOss,
            Self::Leap156SrcOss,
            Self::Leap156SrcNonOss,
            // Debug
            Self::TumbleweedDebug,
            Self::Leap160Debug,
            Self::Leap156Debug,
            Self::Leap156DebugUpdate,
            // Factory
            Self::FactoryOss,
        ]
    }

    /// Binary repos only (no source or debug).
    pub fn binary_only() -> &'static [OpenSuseRepo] {
        &[
            Self::TumbleweedOss,
            Self::TumbleweedNonOss,
            Self::TumbleweedUpdate,
            Self::Leap160Oss,
            Self::Leap160NonOss,
            Self::Leap156Oss,
            Self::Leap156NonOss,
            Self::Leap156UpdateOss,
            Self::Leap156UpdateNonOss,
            Self::Leap156Backports,
            Self::Leap156Sle,
            Self::FactoryOss,
        ]
    }

    /// Just Tumbleweed repos.
    pub fn tumbleweed() -> &'static [OpenSuseRepo] {
        &[
            Self::TumbleweedOss,
            Self::TumbleweedNonOss,
            Self::TumbleweedUpdate,
        ]
    }

    /// Just Leap 15.6 repos.
    pub fn leap_15_6() -> &'static [OpenSuseRepo] {
        &[
            Self::Leap156Oss,
            Self::Leap156NonOss,
            Self::Leap156UpdateOss,
            Self::Leap156UpdateNonOss,
            Self::Leap156Backports,
            Self::Leap156Sle,
        ]
    }

    /// Just Leap 16.0 repos.
    pub fn leap_16_0() -> &'static [OpenSuseRepo] {
        &[Self::Leap160Oss, Self::Leap160NonOss]
    }
}

/// openSUSE package index fetcher.
pub struct OpenSuse {
    repos: Vec<OpenSuseRepo>,
}

impl Default for OpenSuse {
    fn default() -> Self {
        Self::all()
    }
}

impl OpenSuse {
    /// Create fetcher for all repos.
    pub fn all() -> Self {
        Self {
            repos: OpenSuseRepo::all().to_vec(),
        }
    }

    /// Create fetcher for specific repos.
    pub fn with_repos(repos: &[OpenSuseRepo]) -> Self {
        Self {
            repos: repos.to_vec(),
        }
    }

    /// Create fetcher for Tumbleweed only.
    pub fn tumbleweed() -> Self {
        Self {
            repos: OpenSuseRepo::tumbleweed().to_vec(),
        }
    }

    /// Create fetcher for Leap 15.6 only.
    pub fn leap_15_6() -> Self {
        Self {
            repos: OpenSuseRepo::leap_15_6().to_vec(),
        }
    }

    /// Create fetcher for Leap 16.0 only.
    pub fn leap_16_0() -> Self {
        Self {
            repos: OpenSuseRepo::leap_16_0().to_vec(),
        }
    }

    /// Find primary.xml.zst URL from repomd.xml.
    fn find_primary_url(repo: OpenSuseRepo) -> Result<String, IndexError> {
        let repomd_url = format!("{}/repomd.xml", repo.base_url());
        let cache_key = format!("repomd-{}", repo.id());
        let (data, _) = cache::fetch_with_cache("opensuse", &cache_key, &repomd_url, CACHE_TTL)
            .map_err(IndexError::Network)?;

        let xml = String::from_utf8_lossy(&data);

        // Parse repomd.xml to find primary.xml.zst location
        for line in xml.lines() {
            if line.contains("primary.xml.zst") || line.contains("primary.xml.gz") {
                if let Some(start) = line.find("href=\"") {
                    let rest = &line[start + 6..];
                    if let Some(end) = rest.find('"') {
                        let href = &rest[..end];
                        let base = repo.base_url().trim_end_matches("/repodata");
                        return Ok(format!("{}/{}", base, href));
                    }
                }
            }
        }

        Err(IndexError::Parse(format!(
            "primary.xml not found in repomd.xml for {}",
            repo.id()
        )))
    }

    /// Parse primary.xml to extract packages, tagging with source repo.
    fn parse_primary(xml: &str, repo_id: &str) -> Vec<PackageMeta> {
        let mut packages = Vec::new();
        let mut in_package = false;
        let mut name = String::new();
        let mut version = String::new();
        let mut release = String::new();
        let mut summary = String::new();
        let mut url = String::new();
        let mut license = String::new();

        for line in xml.lines() {
            let line = line.trim();

            if line.starts_with("<package type=\"rpm\">") {
                in_package = true;
                name.clear();
                version.clear();
                release.clear();
                summary.clear();
                url.clear();
                license.clear();
            } else if line == "</package>" && in_package {
                if !name.is_empty() {
                    let mut extra = HashMap::new();
                    extra.insert(
                        "source_repo".to_string(),
                        serde_json::Value::String(repo_id.to_string()),
                    );

                    // Include release in version if present
                    let full_version = if release.is_empty() {
                        version.clone()
                    } else {
                        format!("{}-{}", version, release)
                    };

                    packages.push(PackageMeta {
                        name: name.clone(),
                        version: full_version,
                        description: if summary.is_empty() {
                            None
                        } else {
                            Some(summary.clone())
                        },
                        homepage: if url.is_empty() {
                            None
                        } else {
                            Some(url.clone())
                        },
                        repository: Some(
                            "https://build.opensuse.org/project/show/openSUSE:Factory".to_string(),
                        ),
                        license: if license.is_empty() {
                            None
                        } else {
                            Some(license.clone())
                        },
                        binaries: Vec::new(),
                        keywords: Vec::new(),
                        maintainers: Vec::new(),
                        published: None,
                        downloads: None,
                        archive_url: None,
                        checksum: None,
                        extra,
                    });
                }
                in_package = false;
            } else if in_package {
                if line.starts_with("<name>") && line.ends_with("</name>") {
                    name = line[6..line.len() - 7].to_string();
                } else if line.starts_with("<summary>") && line.ends_with("</summary>") {
                    summary = line[9..line.len() - 10].to_string();
                } else if line.starts_with("<url>") && line.ends_with("</url>") {
                    url = line[5..line.len() - 6].to_string();
                } else if line.starts_with("<rpm:license>") && line.ends_with("</rpm:license>") {
                    license = line[13..line.len() - 14].to_string();
                } else if line.starts_with("<version ") {
                    if let Some(ver_start) = line.find("ver=\"") {
                        let rest = &line[ver_start + 5..];
                        if let Some(ver_end) = rest.find('"') {
                            version = rest[..ver_end].to_string();
                        }
                    }
                    if let Some(rel_start) = line.find("rel=\"") {
                        let rest = &line[rel_start + 5..];
                        if let Some(rel_end) = rest.find('"') {
                            release = rest[..rel_end].to_string();
                        }
                    }
                }
            }
        }

        packages
    }

    /// Load packages from a single repo.
    fn load_repo(repo: OpenSuseRepo) -> Result<Vec<PackageMeta>, IndexError> {
        let primary_url = Self::find_primary_url(repo)?;
        let cache_key = format!("primary-{}", repo.id());

        let (data, _was_cached) =
            cache::fetch_with_cache("opensuse", &cache_key, &primary_url, CACHE_TTL)
                .map_err(IndexError::Network)?;

        // Decompress - try zstd first, fall back to gzip
        let decompressed = if primary_url.ends_with(".zst") {
            zstd::decode_all(std::io::Cursor::new(&data))
                .map_err(|e| IndexError::Decompress(e.to_string()))?
        } else {
            use flate2::read::GzDecoder;
            use std::io::Read;
            let mut decoder = GzDecoder::new(&data[..]);
            let mut decompressed = Vec::new();
            decoder
                .read_to_end(&mut decompressed)
                .map_err(|e| IndexError::Decompress(e.to_string()))?;
            decompressed
        };

        let xml = String::from_utf8_lossy(&decompressed);
        Ok(Self::parse_primary(&xml, repo.id()))
    }

    /// Load packages from configured repos in parallel.
    fn load_packages(&self) -> Result<Vec<PackageMeta>, IndexError> {
        let results: Vec<_> = self
            .repos
            .par_iter()
            .map(|&repo| Self::load_repo(repo))
            .collect();

        let mut all_packages = Vec::new();
        for (repo, result) in self.repos.iter().zip(results) {
            match result {
                Ok(packages) => {
                    all_packages.extend(packages);
                }
                Err(e) => {
                    eprintln!("Warning: failed to load openSUSE repo {}: {}", repo.id(), e);
                }
            }
        }

        if all_packages.is_empty() {
            return Err(IndexError::Network(
                "failed to load any openSUSE repos".into(),
            ));
        }

        Ok(all_packages)
    }
}

impl PackageIndex for OpenSuse {
    fn ecosystem(&self) -> &'static str {
        "opensuse"
    }

    fn display_name(&self) -> &'static str {
        "openSUSE (zypper)"
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
        let name_lower = name.to_lowercase();

        let versions: Vec<VersionMeta> = packages
            .into_iter()
            .filter(|p| p.name.to_lowercase() == name_lower)
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

    fn fetch_all(&self) -> Result<Vec<PackageMeta>, IndexError> {
        self.load_packages()
    }
}
