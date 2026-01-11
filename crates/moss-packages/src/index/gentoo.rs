//! Gentoo package index fetcher (Portage).
//!
//! Fetches package metadata from packages.gentoo.org and overlay repos.
//!
//! ## API Strategy
//! - **fetch**: `packages.gentoo.org/packages/{category}/{name}.json` - Official JSON API
//! - **fetch_versions**: Same as fetch, extracts versions array
//! - **search**: Not supported (API returns HTML, not JSON)
//! - **fetch_all**: Not supported (no bulk endpoint)
//!
//! ## Multi-repo Support
//! ```rust,ignore
//! use moss_packages::index::gentoo::{Gentoo, GentooRepo};
//!
//! // All repos (default)
//! let all = Gentoo::all();
//!
//! // Main tree only
//! let main = Gentoo::main_only();
//!
//! // With overlays
//! let with_overlays = Gentoo::with_overlays();
//! ```

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use std::collections::HashMap;

/// Available Gentoo repositories/overlays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GentooRepo {
    // === Main Tree ===
    /// Official Gentoo repository (::gentoo)
    Gentoo,

    // === Official Overlays ===
    /// GURU - Gentoo's User Repository (community maintained)
    Guru,
    /// Gentoo Science overlay
    Science,
    /// Gentoo Haskell overlay
    Haskell,
    /// Gentoo Games overlay
    Games,
}

impl GentooRepo {
    /// Get the repository name for tagging.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Gentoo => "gentoo",
            Self::Guru => "guru",
            Self::Science => "science",
            Self::Haskell => "haskell",
            Self::Games => "games",
        }
    }

    /// All available repositories.
    pub fn all() -> &'static [GentooRepo] {
        &[
            Self::Gentoo,
            Self::Guru,
            Self::Science,
            Self::Haskell,
            Self::Games,
        ]
    }

    /// Main tree only.
    pub fn main() -> &'static [GentooRepo] {
        &[Self::Gentoo]
    }

    /// Popular overlays only (GURU + Science).
    pub fn overlays() -> &'static [GentooRepo] {
        &[Self::Guru, Self::Science, Self::Haskell, Self::Games]
    }

    /// Main tree with GURU overlay.
    pub fn with_guru() -> &'static [GentooRepo] {
        &[Self::Gentoo, Self::Guru]
    }
}

/// Gentoo package index fetcher with configurable overlays.
pub struct Gentoo {
    repos: Vec<GentooRepo>,
}

impl Gentoo {
    /// Gentoo packages API.
    const GENTOO_API: &'static str = "https://packages.gentoo.org";

    /// Zugaina overlay API (for GURU and other overlays).
    const ZUGAINA_API: &'static str = "https://gpo.zugaina.org";

    /// Create a fetcher with all repositories.
    pub fn all() -> Self {
        Self {
            repos: GentooRepo::all().to_vec(),
        }
    }

    /// Create a fetcher with main tree only.
    pub fn main_only() -> Self {
        Self {
            repos: GentooRepo::main().to_vec(),
        }
    }

    /// Create a fetcher with overlays only.
    pub fn overlays() -> Self {
        Self {
            repos: GentooRepo::overlays().to_vec(),
        }
    }

    /// Create a fetcher with main tree and GURU.
    pub fn with_guru() -> Self {
        Self {
            repos: GentooRepo::with_guru().to_vec(),
        }
    }

    /// Create a fetcher with custom repository selection.
    pub fn with_repos(repos: &[GentooRepo]) -> Self {
        Self {
            repos: repos.to_vec(),
        }
    }

    /// Fetch from main Gentoo tree.
    fn fetch_main(name: &str) -> Result<PackageMeta, IndexError> {
        // Gentoo uses category/package format (e.g., "sys-apps/ripgrep")
        // If no category provided, search for it
        let package_path = if name.contains('/') {
            name.to_string()
        } else {
            // Search and use first result
            let search_url = format!("{}/packages/search?q={}", Self::GENTOO_API, name);
            let search_response: serde_json::Value = ureq::get(&search_url)
                .set("Accept", "application/json")
                .call()?
                .into_json()?;

            let packages = search_response["packages"]
                .as_array()
                .or_else(|| search_response.as_array())
                .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

            let pkg = packages
                .iter()
                .find(|p| p["name"].as_str() == Some(name))
                .or_else(|| packages.first())
                .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

            format!(
                "{}/{}",
                pkg["category"].as_str().unwrap_or("unknown"),
                pkg["name"].as_str().unwrap_or(name)
            )
        };

        let url = format!("{}/packages/{}.json", Self::GENTOO_API, package_path);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        // Get latest stable version
        let versions = response["versions"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing versions".into()))?;

        let latest = versions
            .iter()
            .filter(|v| {
                v["keywords"]
                    .as_array()
                    .map(|kw| {
                        kw.iter()
                            .any(|k| !k.as_str().unwrap_or("").starts_with('~'))
                    })
                    .unwrap_or(false)
            })
            .last()
            .or_else(|| versions.last())
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        let mut extra = HashMap::new();
        extra.insert(
            "source_repo".to_string(),
            serde_json::Value::String("gentoo".to_string()),
        );

        if let Some(cat) = response["category"].as_str() {
            extra.insert(
                "category".to_string(),
                serde_json::Value::String(cat.to_string()),
            );
        }

        Ok(PackageMeta {
            name: response["name"].as_str().unwrap_or(name).to_string(),
            version: latest["version"].as_str().unwrap_or("unknown").to_string(),
            description: response["description"].as_str().map(String::from),
            homepage: response["homepage"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|u| u.as_str())
                .map(String::from),
            repository: extract_repo(&response["homepage"]),
            license: response["licenses"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|l| l.as_str())
                .map(String::from),
            binaries: Vec::new(),
            keywords: Vec::new(),
            maintainers: response["maintainers"]
                .as_array()
                .map(|m| {
                    m.iter()
                        .filter_map(|p| p["email"].as_str().or_else(|| p["name"].as_str()))
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default(),
            published: None,
            downloads: None,
            archive_url: None,
            checksum: None,
            extra,
        })
    }

    /// Fetch from an overlay via Zugaina.
    fn fetch_overlay(name: &str, repo: GentooRepo) -> Result<PackageMeta, IndexError> {
        // Zugaina provides overlay package info
        // Zugaina returns HTML, so we'll try a direct package path if category is known
        let package_path = if name.contains('/') {
            name.to_string()
        } else {
            // For overlays without API, we try common categories
            let common_categories = [
                "app-misc",
                "dev-util",
                "sys-apps",
                "app-shells",
                "dev-libs",
                "dev-python",
                "dev-rust",
                "games-misc",
                "sci-libs",
            ];

            for cat in common_categories {
                let test_path = format!("{}/{}", cat, name);
                let test_url = format!(
                    "{}/Overlays/{}/{}",
                    Self::ZUGAINA_API,
                    repo.name(),
                    test_path
                );
                if ureq::get(&test_url).call().is_ok() {
                    return Self::parse_zugaina_page(&test_url, name, repo);
                }
            }

            // If not found in common categories, report not found
            return Err(IndexError::NotFound(format!("{} in {}", name, repo.name())));
        };

        let url = format!(
            "{}/Overlays/{}/{}",
            Self::ZUGAINA_API,
            repo.name(),
            package_path
        );
        Self::parse_zugaina_page(&url, name, repo)
    }

    /// Parse a Zugaina package page (simplified - returns basic metadata).
    fn parse_zugaina_page(
        url: &str,
        name: &str,
        repo: GentooRepo,
    ) -> Result<PackageMeta, IndexError> {
        // Zugaina returns HTML - we'll extract basic info
        let response = ureq::get(url).call()?;
        let html = response.into_string()?;

        // Check if we got a valid package page
        if html.contains("No Packages found") || html.contains("404") {
            return Err(IndexError::NotFound(format!("{} in {}", name, repo.name())));
        }

        // Extract version from HTML (simple regex-like parsing)
        let version = extract_version_from_html(&html).unwrap_or("unknown".to_string());
        let description = extract_description_from_html(&html);

        let mut extra = HashMap::new();
        extra.insert(
            "source_repo".to_string(),
            serde_json::Value::String(repo.name().to_string()),
        );

        Ok(PackageMeta {
            name: name.split('/').last().unwrap_or(name).to_string(),
            version,
            description,
            homepage: None,
            repository: None,
            license: None,
            binaries: Vec::new(),
            keywords: Vec::new(),
            maintainers: Vec::new(),
            published: None,
            downloads: None,
            archive_url: None,
            checksum: None,
            extra,
        })
    }

    /// Fetch versions from main tree.
    fn fetch_versions_main(name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let package_path = if name.contains('/') {
            name.to_string()
        } else {
            // Need to find the category first via search
            let search_url = format!("{}/packages/search?q={}", Self::GENTOO_API, name);
            let search_response: serde_json::Value = ureq::get(&search_url)
                .set("Accept", "application/json")
                .call()?
                .into_json()?;

            let packages = search_response["packages"]
                .as_array()
                .or_else(|| search_response.as_array())
                .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

            let pkg = packages
                .iter()
                .find(|p| p["name"].as_str() == Some(name))
                .or_else(|| packages.first())
                .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

            format!(
                "{}/{}",
                pkg["category"].as_str().unwrap_or("unknown"),
                pkg["name"].as_str().unwrap_or(name)
            )
        };

        let url = format!("{}/packages/{}.json", Self::GENTOO_API, package_path);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let versions = response["versions"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing versions".into()))?;

        Ok(versions
            .iter()
            .filter_map(|v| {
                Some(VersionMeta {
                    version: v["version"].as_str()?.to_string(),
                    released: None,
                    // Masked versions could be considered "yanked"
                    yanked: v["masks"]
                        .as_array()
                        .map(|m| !m.is_empty())
                        .unwrap_or(false),
                })
            })
            .collect())
    }
}

impl PackageIndex for Gentoo {
    fn ecosystem(&self) -> &'static str {
        "gentoo"
    }

    fn display_name(&self) -> &'static str {
        "Gentoo (Portage)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Try configured repos in order
        for &repo in &self.repos {
            let result = match repo {
                GentooRepo::Gentoo => Self::fetch_main(name),
                _ => Self::fetch_overlay(name, repo),
            };

            match result {
                Ok(pkg) => return Ok(pkg),
                Err(IndexError::NotFound(_)) => continue,
                Err(e) => return Err(e),
            }
        }

        Err(IndexError::NotFound(name.to_string()))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // Versions are primarily available from main tree
        if self.repos.contains(&GentooRepo::Gentoo) {
            return Self::fetch_versions_main(name);
        }

        // For overlay-only, return single version from fetch
        let pkg = self.fetch(name)?;
        Ok(vec![VersionMeta {
            version: pkg.version,
            released: None,
            yanked: false,
        }])
    }

    fn search(&self, _query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        // Gentoo search API returns HTML, not JSON
        // Use fetch() with category/package format instead (e.g., "sys-apps/ripgrep")
        Err(IndexError::NotImplemented(
            "Gentoo search API returns HTML. Use fetch() with category/name format.".into(),
        ))
    }
}

fn extract_repo(homepage: &serde_json::Value) -> Option<String> {
    homepage.as_array().and_then(|urls| {
        urls.iter()
            .filter_map(|u| u.as_str())
            .find(|u| u.contains("github.com") || u.contains("gitlab.com"))
            .map(String::from)
    })
}

/// Extract version from Zugaina HTML page.
fn extract_version_from_html(html: &str) -> Option<String> {
    // Look for version in ebuild filenames like "package-1.2.3.ebuild"
    for line in html.lines() {
        if line.contains(".ebuild") {
            // Extract version from ebuild filename
            if let Some(start) = line.rfind('-') {
                if let Some(end) = line.find(".ebuild") {
                    if start < end {
                        let version = &line[start + 1..end];
                        // Basic validation - version should start with digit
                        if version
                            .chars()
                            .next()
                            .map(|c| c.is_ascii_digit())
                            .unwrap_or(false)
                        {
                            return Some(version.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

/// Extract description from Zugaina HTML page.
fn extract_description_from_html(html: &str) -> Option<String> {
    // Look for description in meta tag or specific div
    for line in html.lines() {
        if line.contains("class=\"description\"") || line.contains("meta name=\"description\"") {
            // Simple extraction - in practice this would need proper HTML parsing
            if let Some(start) = line.find("content=\"").or_else(|| line.find('>')) {
                let start = start + if line.contains("content=\"") { 9 } else { 1 };
                if let Some(end) = line[start..].find(|c| c == '"' || c == '<') {
                    let desc = &line[start..start + end];
                    if !desc.is_empty() {
                        return Some(desc.to_string());
                    }
                }
            }
        }
    }
    None
}
