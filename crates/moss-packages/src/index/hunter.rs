//! Hunter C++ package manager index fetcher.
//!
//! Fetches package metadata from Hunter's GitHub repository.
//! Hunter is a CMake-driven cross-platform package manager for C/C++.
//! Parses the cmake/configs/default.cmake file for package versions.
//!
//! ## API Strategy
//! - **fetch**: Searches cached `github.com/cpp-pm/hunter/.../default.cmake`
//! - **fetch_versions**: Same, single version per package
//! - **search**: Filters cached cmake config
//! - **fetch_all**: Parses default.cmake from GitHub (cached 1 hour)

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use crate::cache;
use std::time::Duration;

/// Cache TTL for Hunter package list (1 hour).
const CACHE_TTL: Duration = Duration::from_secs(60 * 60);

/// Hunter package index fetcher.
pub struct Hunter;

impl Hunter {
    /// Hunter default.cmake URL on GitHub.
    const DEFAULT_CMAKE_URL: &'static str =
        "https://raw.githubusercontent.com/cpp-pm/hunter/master/cmake/configs/default.cmake";

    /// Parse packages from the cmake file content.
    /// Parses lines like: hunter_default_version(Boost VERSION 1.86.0)
    fn parse_packages(content: &str) -> Vec<PackageMeta> {
        let mut packages = Vec::new();

        for line in content.lines() {
            let line = line.trim();
            if !line.starts_with("hunter_default_version(") {
                continue;
            }

            // Extract content between parentheses
            let start = match line.find('(') {
                Some(i) => i + 1,
                None => continue,
            };
            let end = match line.rfind(')') {
                Some(i) => i,
                None => continue,
            };
            let inner = &line[start..end];

            // Split by whitespace and find name and version
            let parts: Vec<&str> = inner.split_whitespace().collect();
            if parts.len() < 3 {
                continue;
            }

            let name = parts[0];
            // Find VERSION keyword and get the next part
            let version_idx = parts.iter().position(|&p| p == "VERSION");
            let version = match version_idx {
                Some(idx) if idx + 1 < parts.len() => parts[idx + 1],
                _ => continue,
            };

            packages.push(PackageMeta {
                name: name.to_string(),
                version: version.to_string(),
                description: None, // cmake file doesn't have descriptions
                homepage: Some(format!(
                    "https://hunter.readthedocs.io/en/latest/packages/pkg/{}.html",
                    name
                )),
                repository: Some("https://github.com/cpp-pm/hunter".to_string()),
                license: None,
                binaries: Vec::new(),
                keywords: Vec::new(),
                maintainers: Vec::new(),
                published: None,
                downloads: None,
                archive_url: None,
                checksum: None,
                extra: Default::default(),
            });
        }

        packages
    }

    /// Load and cache the cmake file.
    fn load_packages() -> Result<Vec<PackageMeta>, IndexError> {
        let (data, _was_cached) = cache::fetch_with_cache(
            "hunter",
            "default-cmake",
            Self::DEFAULT_CMAKE_URL,
            CACHE_TTL,
        )
        .map_err(IndexError::Network)?;

        let content = String::from_utf8(data)
            .map_err(|e| IndexError::Parse(format!("UTF-8 error: {}", e)))?;

        Ok(Self::parse_packages(&content))
    }
}

impl PackageIndex for Hunter {
    fn ecosystem(&self) -> &'static str {
        "hunter"
    }

    fn display_name(&self) -> &'static str {
        "Hunter"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let packages = Self::load_packages()?;

        packages
            .into_iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| IndexError::NotFound(name.to_string()))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // Hunter only tracks the default version in the cmake file
        let pkg = self.fetch(name)?;
        Ok(vec![VersionMeta {
            version: pkg.version,
            released: None,
            yanked: false,
        }])
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let packages = Self::load_packages()?;
        let query_lower = query.to_lowercase();

        Ok(packages
            .into_iter()
            .filter(|p| p.name.to_lowercase().contains(&query_lower))
            .take(50)
            .collect())
    }

    fn fetch_all(&self) -> Result<Vec<PackageMeta>, IndexError> {
        Self::load_packages()
    }
}
