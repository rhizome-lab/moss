//! MSYS2 package index fetcher (Windows development).
//!
//! Fetches package metadata from the MSYS2 packages API.
//!
//! ## API Strategy
//! - **fetch**: `packages.msys2.org/api/package/{name}` - Official MSYS2 JSON API
//! - **fetch_versions**: Same API, single version
//! - **search**: `packages.msys2.org/api/search?q=`
//! - **fetch_all**: `packages.msys2.org/api/packages` (all packages)
//!
//! ## Multi-environment Support
//! ```rust,ignore
//! use moss_packages::index::msys2::{Msys2, Msys2Env};
//!
//! // All environments (default)
//! let all = Msys2::all();
//!
//! // MinGW 64-bit only
//! let mingw64 = Msys2::mingw64();
//!
//! // Modern environments (UCRT + Clang)
//! let modern = Msys2::modern();
//! ```

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use std::collections::HashMap;

/// Available MSYS2 environments/subsystems.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Msys2Env {
    /// MSYS - POSIX compatibility layer
    Msys,
    /// MinGW 32-bit - Windows native (GCC)
    Mingw32,
    /// MinGW 64-bit - Windows native (GCC)
    Mingw64,
    /// UCRT 64-bit - Windows native with Universal C Runtime
    Ucrt64,
    /// Clang 32-bit - Windows native (Clang/LLVM)
    Clang32,
    /// Clang 64-bit - Windows native (Clang/LLVM)
    Clang64,
}

impl Msys2Env {
    /// Get the repository name used in the API.
    fn repo_name(&self) -> &'static str {
        match self {
            Self::Msys => "msys",
            Self::Mingw32 => "mingw32",
            Self::Mingw64 => "mingw64",
            Self::Ucrt64 => "ucrt64",
            Self::Clang32 => "clang32",
            Self::Clang64 => "clang64",
        }
    }

    /// Get the environment name for tagging.
    pub fn name(&self) -> &'static str {
        self.repo_name()
    }

    /// All available environments.
    pub fn all() -> &'static [Msys2Env] {
        &[
            Self::Msys,
            Self::Mingw32,
            Self::Mingw64,
            Self::Ucrt64,
            Self::Clang32,
            Self::Clang64,
        ]
    }

    /// MinGW environments (traditional GCC toolchains).
    pub fn mingw() -> &'static [Msys2Env] {
        &[Self::Mingw32, Self::Mingw64]
    }

    /// MinGW 64-bit only.
    pub fn mingw64() -> &'static [Msys2Env] {
        &[Self::Mingw64]
    }

    /// UCRT environments (modern Windows runtime).
    pub fn ucrt() -> &'static [Msys2Env] {
        &[Self::Ucrt64]
    }

    /// Clang/LLVM environments.
    pub fn clang() -> &'static [Msys2Env] {
        &[Self::Clang32, Self::Clang64]
    }

    /// Modern environments (UCRT + Clang).
    pub fn modern() -> &'static [Msys2Env] {
        &[Self::Ucrt64, Self::Clang32, Self::Clang64]
    }

    /// 64-bit environments only.
    pub fn x64() -> &'static [Msys2Env] {
        &[Self::Mingw64, Self::Ucrt64, Self::Clang64]
    }
}

/// MSYS2 package index fetcher with configurable environments.
pub struct Msys2 {
    envs: Vec<Msys2Env>,
}

impl Msys2 {
    /// MSYS2 packages API base URL.
    const API_BASE: &'static str = "https://packages.msys2.org/api";

    /// Create a fetcher with all environments.
    pub fn all() -> Self {
        Self {
            envs: Msys2Env::all().to_vec(),
        }
    }

    /// Create a fetcher with MinGW 64-bit only.
    pub fn mingw64() -> Self {
        Self {
            envs: Msys2Env::mingw64().to_vec(),
        }
    }

    /// Create a fetcher with MinGW environments.
    pub fn mingw() -> Self {
        Self {
            envs: Msys2Env::mingw().to_vec(),
        }
    }

    /// Create a fetcher with UCRT environment.
    pub fn ucrt() -> Self {
        Self {
            envs: Msys2Env::ucrt().to_vec(),
        }
    }

    /// Create a fetcher with modern environments.
    pub fn modern() -> Self {
        Self {
            envs: Msys2Env::modern().to_vec(),
        }
    }

    /// Create a fetcher with 64-bit environments.
    pub fn x64() -> Self {
        Self {
            envs: Msys2Env::x64().to_vec(),
        }
    }

    /// Create a fetcher with custom environment selection.
    pub fn with_envs(envs: &[Msys2Env]) -> Self {
        Self {
            envs: envs.to_vec(),
        }
    }

    /// Check if a package's repo matches any configured environment.
    fn matches_env(&self, repo: Option<&str>) -> bool {
        match repo {
            Some(r) => self.envs.iter().any(|e| e.repo_name() == r),
            None => true, // If no repo info, include it
        }
    }
}

impl PackageIndex for Msys2 {
    fn ecosystem(&self) -> &'static str {
        "msys2"
    }

    fn display_name(&self) -> &'static str {
        "MSYS2 (Windows)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let url = format!("{}/search?query={}", Self::API_BASE, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        // Check for exact match first
        let pkg = if let Some(exact) = response["results"]["exact"].as_object() {
            let repo = exact.get("repo").and_then(|r| r.as_str());
            if self.matches_env(repo) {
                Some((exact.clone(), repo.map(String::from)))
            } else {
                None
            }
        } else {
            None
        };

        let (pkg, repo) = if let Some((p, r)) = pkg {
            (p, r)
        } else if let Some(others) = response["results"]["other"].as_array() {
            // Find first match in other results that matches our environments
            others
                .iter()
                .find_map(|p| {
                    let repo = p["repo"].as_str();
                    if (p["name"].as_str() == Some(name) || p["realname"].as_str() == Some(name))
                        && self.matches_env(repo)
                    {
                        p.as_object()
                            .map(|obj| (obj.clone(), repo.map(String::from)))
                    } else {
                        None
                    }
                })
                .ok_or_else(|| IndexError::NotFound(name.to_string()))?
        } else {
            return Err(IndexError::NotFound(name.to_string()));
        };

        let mut extra = HashMap::new();
        if let Some(r) = &repo {
            extra.insert(
                "source_repo".to_string(),
                serde_json::Value::String(r.clone()),
            );
        }

        // Extract license from nested array
        let license = pkg
            .get("licenses")
            .and_then(|l| l.as_array())
            .and_then(|arr| arr.first())
            .and_then(|inner| inner.as_array())
            .and_then(|arr| arr.first())
            .and_then(|l| l.as_str())
            .map(String::from);

        // Collect keywords from groups
        let keywords: Vec<String> = pkg
            .get("groups")
            .and_then(|g| g.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        Ok(PackageMeta {
            name: pkg
                .get("realname")
                .or(pkg.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or(name)
                .to_string(),
            version: pkg
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            description: pkg
                .get("descriptions")
                .and_then(|d| d.as_str())
                .map(String::from),
            homepage: pkg.get("url").and_then(|u| u.as_str()).map(String::from),
            repository: pkg
                .get("source_url")
                .and_then(|u| u.as_str())
                .map(String::from),
            license,
            binaries: Vec::new(),
            keywords,
            maintainers: Vec::new(),
            published: None,
            downloads: None,
            archive_url: None,
            checksum: None,
            extra,
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // MSYS2 API only provides current version
        let meta = self.fetch(name)?;
        let repo = meta
            .extra
            .get("source_repo")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        Ok(vec![VersionMeta {
            version: format!("{} ({})", meta.version, repo),
            released: None,
            yanked: false,
        }])
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!("{}/search?query={}", Self::API_BASE, query);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let mut packages = Vec::new();

        // Add exact match if present and matches environment
        if let Some(exact) = response["results"]["exact"].as_object() {
            let repo = exact.get("repo").and_then(|r| r.as_str());
            if self.matches_env(repo) {
                if let Some(pkg) = parse_msys2_package(exact, repo) {
                    packages.push(pkg);
                }
            }
        }

        // Add other matches that match our environments
        if let Some(others) = response["results"]["other"].as_array() {
            for other in others {
                if let Some(obj) = other.as_object() {
                    let repo = obj.get("repo").and_then(|r| r.as_str());
                    if self.matches_env(repo) {
                        if let Some(pkg) = parse_msys2_package(obj, repo) {
                            packages.push(pkg);
                        }
                    }
                }
            }
        }

        Ok(packages)
    }
}

fn parse_msys2_package(
    pkg: &serde_json::Map<String, serde_json::Value>,
    repo: Option<&str>,
) -> Option<PackageMeta> {
    let license = pkg
        .get("licenses")
        .and_then(|l| l.as_array())
        .and_then(|arr| arr.first())
        .and_then(|inner| inner.as_array())
        .and_then(|arr| arr.first())
        .and_then(|l| l.as_str())
        .map(String::from);

    let keywords: Vec<String> = pkg
        .get("groups")
        .and_then(|g| g.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let mut extra = HashMap::new();
    if let Some(r) = repo {
        extra.insert(
            "source_repo".to_string(),
            serde_json::Value::String(r.to_string()),
        );
    }

    Some(PackageMeta {
        name: pkg
            .get("realname")
            .or(pkg.get("name"))
            .and_then(|n| n.as_str())?
            .to_string(),
        version: pkg
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        description: pkg
            .get("descriptions")
            .and_then(|d| d.as_str())
            .map(String::from),
        homepage: pkg.get("url").and_then(|u| u.as_str()).map(String::from),
        repository: pkg
            .get("source_url")
            .and_then(|u| u.as_str())
            .map(String::from),
        license,
        binaries: Vec::new(),
        keywords,
        maintainers: Vec::new(),
        published: None,
        downloads: None,
        archive_url: None,
        checksum: None,
        extra,
    })
}
