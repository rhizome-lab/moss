//! Void Linux package index fetcher (xbps).
//!
//! Fetches package metadata from Void Linux repositories.
//!
//! ## API Strategy
//! - **fetch**: Searches cached `repo-default.voidlinux.org/.../x86_64-repodata` (zstd tar + XML plist)
//! - **fetch_versions**: Loads from all configured repos
//! - **search**: Filters cached repodata
//! - **fetch_all**: Full repodata (cached 1 hour, ~20MB uncompressed per repo)
//!
//! ## Multi-repo Support
//! ```rust,ignore
//! use moss_packages::index::void::{Void, VoidRepo};
//!
//! // All repos (default)
//! let all = Void::all();
//!
//! // x86_64 glibc only
//! let x64 = Void::with_repos(&[VoidRepo::X86_64, VoidRepo::X86_64Nonfree]);
//!
//! // musl variants
//! let musl = Void::musl();
//! ```

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use crate::cache;
use rayon::prelude::*;
use std::collections::HashMap;
use std::io::Read;
use std::time::Duration;

/// Cache TTL for Void package index (1 hour).
const CACHE_TTL: Duration = Duration::from_secs(60 * 60);

/// Void Linux repository base URL.
const VOID_MIRROR: &str = "https://repo-default.voidlinux.org/current";

/// Available Void Linux repositories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VoidRepo {
    // === x86_64 glibc ===
    /// x86_64 glibc main repository
    X86_64,
    /// x86_64 glibc nonfree repository
    X86_64Nonfree,

    // === x86_64 musl ===
    /// x86_64 musl main repository
    X86_64Musl,
    /// x86_64 musl nonfree repository
    X86_64MuslNonfree,

    // === aarch64 glibc ===
    /// aarch64 glibc main repository
    Aarch64,
    /// aarch64 glibc nonfree repository
    Aarch64Nonfree,

    // === aarch64 musl ===
    /// aarch64 musl main repository
    Aarch64Musl,
    /// aarch64 musl nonfree repository
    Aarch64MuslNonfree,
}

impl VoidRepo {
    /// Get the repository URL.
    fn url(&self) -> String {
        match self {
            Self::X86_64 => format!("{}/x86_64-repodata", VOID_MIRROR),
            Self::X86_64Nonfree => format!("{}/nonfree/x86_64-repodata", VOID_MIRROR),
            Self::X86_64Musl => format!("{}/musl/x86_64-repodata", VOID_MIRROR),
            Self::X86_64MuslNonfree => format!("{}/musl/nonfree/x86_64-repodata", VOID_MIRROR),
            Self::Aarch64 => format!("{}/aarch64-repodata", VOID_MIRROR),
            Self::Aarch64Nonfree => format!("{}/nonfree/aarch64-repodata", VOID_MIRROR),
            Self::Aarch64Musl => format!("{}/musl/aarch64-repodata", VOID_MIRROR),
            Self::Aarch64MuslNonfree => format!("{}/musl/nonfree/aarch64-repodata", VOID_MIRROR),
        }
    }

    /// Get the repository name for tagging.
    pub fn name(&self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64",
            Self::X86_64Nonfree => "x86_64-nonfree",
            Self::X86_64Musl => "x86_64-musl",
            Self::X86_64MuslNonfree => "x86_64-musl-nonfree",
            Self::Aarch64 => "aarch64",
            Self::Aarch64Nonfree => "aarch64-nonfree",
            Self::Aarch64Musl => "aarch64-musl",
            Self::Aarch64MuslNonfree => "aarch64-musl-nonfree",
        }
    }

    /// All available repositories.
    pub fn all() -> &'static [VoidRepo] {
        &[
            Self::X86_64,
            Self::X86_64Nonfree,
            Self::X86_64Musl,
            Self::X86_64MuslNonfree,
            Self::Aarch64,
            Self::Aarch64Nonfree,
            Self::Aarch64Musl,
            Self::Aarch64MuslNonfree,
        ]
    }

    /// x86_64 glibc repositories.
    pub fn x86_64() -> &'static [VoidRepo] {
        &[Self::X86_64, Self::X86_64Nonfree]
    }

    /// x86_64 musl repositories.
    pub fn x86_64_musl() -> &'static [VoidRepo] {
        &[Self::X86_64Musl, Self::X86_64MuslNonfree]
    }

    /// All musl repositories.
    pub fn musl() -> &'static [VoidRepo] {
        &[
            Self::X86_64Musl,
            Self::X86_64MuslNonfree,
            Self::Aarch64Musl,
            Self::Aarch64MuslNonfree,
        ]
    }

    /// All glibc repositories.
    pub fn glibc() -> &'static [VoidRepo] {
        &[
            Self::X86_64,
            Self::X86_64Nonfree,
            Self::Aarch64,
            Self::Aarch64Nonfree,
        ]
    }

    /// Free (non-proprietary) repositories only.
    pub fn free() -> &'static [VoidRepo] {
        &[
            Self::X86_64,
            Self::X86_64Musl,
            Self::Aarch64,
            Self::Aarch64Musl,
        ]
    }
}

/// Void Linux package index fetcher with configurable repositories.
pub struct Void {
    repos: Vec<VoidRepo>,
}

impl Void {
    /// Create a fetcher with all repositories.
    pub fn all() -> Self {
        Self {
            repos: VoidRepo::all().to_vec(),
        }
    }

    /// Create a fetcher with x86_64 glibc repositories.
    pub fn x86_64() -> Self {
        Self {
            repos: VoidRepo::x86_64().to_vec(),
        }
    }

    /// Create a fetcher with x86_64 musl repositories.
    pub fn x86_64_musl() -> Self {
        Self {
            repos: VoidRepo::x86_64_musl().to_vec(),
        }
    }

    /// Create a fetcher with all musl repositories.
    pub fn musl() -> Self {
        Self {
            repos: VoidRepo::musl().to_vec(),
        }
    }

    /// Create a fetcher with all glibc repositories.
    pub fn glibc() -> Self {
        Self {
            repos: VoidRepo::glibc().to_vec(),
        }
    }

    /// Create a fetcher with free repositories only.
    pub fn free() -> Self {
        Self {
            repos: VoidRepo::free().to_vec(),
        }
    }

    /// Create a fetcher with custom repository selection.
    pub fn with_repos(repos: &[VoidRepo]) -> Self {
        Self {
            repos: repos.to_vec(),
        }
    }

    /// Parse plist XML into packages.
    fn parse_plist(xml: &str, repo: VoidRepo) -> Result<Vec<PackageMeta>, IndexError> {
        let mut packages = Vec::new();
        let mut current_name: Option<String> = None;
        let mut in_package = false;
        let mut current_field: Option<String> = None;

        let mut version = String::new();
        let mut homepage = String::new();
        let mut description = String::new();
        let mut license = String::new();
        let mut maintainer = String::new();

        for line in xml.lines() {
            let line = line.trim();

            if line.starts_with("<key>") && line.ends_with("</key>") {
                let key = &line[5..line.len() - 6];
                if !in_package {
                    current_name = Some(key.to_string());
                    in_package = false;
                    version.clear();
                    homepage.clear();
                    description.clear();
                    license.clear();
                    maintainer.clear();
                } else {
                    current_field = Some(key.to_string());
                }
            } else if line == "<dict>" && current_name.is_some() && !in_package {
                in_package = true;
            } else if line == "</dict>" && in_package {
                if let Some(name) = current_name.take() {
                    let (pkg_name, ver) = if version.contains('-') {
                        let parts: Vec<&str> = version.rsplitn(2, '-').collect();
                        if parts.len() == 2 {
                            (parts[1].to_string(), parts[0].to_string())
                        } else {
                            (name.clone(), version.clone())
                        }
                    } else {
                        (name.clone(), version.clone())
                    };

                    let mut extra = HashMap::new();
                    extra.insert(
                        "source_repo".to_string(),
                        serde_json::Value::String(repo.name().to_string()),
                    );

                    packages.push(PackageMeta {
                        name: pkg_name,
                        version: ver,
                        description: if description.is_empty() {
                            None
                        } else {
                            Some(description.clone())
                        },
                        homepage: if homepage.is_empty() {
                            None
                        } else {
                            Some(homepage.clone())
                        },
                        repository: Some("https://github.com/void-linux/void-packages".to_string()),
                        license: if license.is_empty() {
                            None
                        } else {
                            Some(license.clone())
                        },
                        maintainers: if maintainer.is_empty() {
                            Vec::new()
                        } else {
                            vec![maintainer.clone()]
                        },
                        binaries: Vec::new(),
                        keywords: Vec::new(),
                        published: None,
                        downloads: None,
                        archive_url: None,
                        checksum: None,
                        extra,
                    });
                }
                in_package = false;
            } else if line.starts_with("<string>") && line.ends_with("</string>") {
                let value = &line[8..line.len() - 9];
                if let Some(field) = &current_field {
                    match field.as_str() {
                        "pkgver" => version = value.to_string(),
                        "homepage" => homepage = value.to_string(),
                        "short_desc" => description = value.to_string(),
                        "license" => license = value.to_string(),
                        "maintainer" => maintainer = value.to_string(),
                        _ => {}
                    }
                }
                current_field = None;
            }
        }

        Ok(packages)
    }

    /// Load packages from a single repository.
    fn load_repo(repo: VoidRepo) -> Result<Vec<PackageMeta>, IndexError> {
        let url = repo.url();

        let (data, _was_cached) = cache::fetch_with_cache(
            "void",
            &format!("repodata-{}", repo.name()),
            &url,
            CACHE_TTL,
        )
        .map_err(IndexError::Network)?;

        // Decompress zstd
        let decompressed = zstd::decode_all(std::io::Cursor::new(&data))
            .map_err(|e| IndexError::Decompress(e.to_string()))?;

        // Extract tar
        let mut archive = tar::Archive::new(std::io::Cursor::new(decompressed));

        for entry in archive.entries().map_err(|e| IndexError::Io(e))? {
            let mut entry = entry.map_err(|e| IndexError::Io(e))?;
            let path = entry.path().map_err(|e| IndexError::Io(e))?;

            if path.to_string_lossy() == "index.plist" {
                let mut xml = String::new();
                entry
                    .read_to_string(&mut xml)
                    .map_err(|e| IndexError::Io(e))?;
                return Self::parse_plist(&xml, repo);
            }
        }

        Err(IndexError::Parse("index.plist not found in archive".into()))
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
                    eprintln!("Warning: failed to load Void repo: {}", e);
                }
            }
        }

        Ok(packages)
    }
}

impl PackageIndex for Void {
    fn ecosystem(&self) -> &'static str {
        "void"
    }

    fn display_name(&self) -> &'static str {
        "Void Linux (xbps)"
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
