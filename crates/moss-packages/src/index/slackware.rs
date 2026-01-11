//! Slackware package index fetcher (SlackBuilds).
//!
//! Fetches package metadata from slackbuilds.org.
//!
//! ## API Strategy
//! - **fetch**: `github.com/SlackBuildsOrg/slackbuilds/.../info` - GitHub raw files
//! - **fetch_versions**: Same, single version per package
//! - **search**: Not supported (no search API)
//! - **fetch_all**: Not supported (would need to enumerate all directories)
//!
//! ## Multi-version Support
//! ```rust,ignore
//! use moss_packages::index::slackware::{Slackware, SlackwareVersion};
//!
//! // All versions (default)
//! let all = Slackware::all();
//!
//! // Current (development) only
//! let current = Slackware::current();
//!
//! // Stable versions only
//! let stable = Slackware::stable();
//! ```

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use std::collections::HashMap;

/// Available Slackware versions/branches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlackwareVersion {
    /// Current development branch (master)
    Current,
    /// Slackware 15.0 (latest stable)
    Slack150,
    /// Slackware 14.2 (older stable)
    Slack142,
}

impl SlackwareVersion {
    /// Get the git branch name for this version.
    fn branch(&self) -> &'static str {
        match self {
            Self::Current => "master",
            Self::Slack150 => "15.0",
            Self::Slack142 => "14.2",
        }
    }

    /// Get the version name for tagging.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Slack150 => "15.0",
            Self::Slack142 => "14.2",
        }
    }

    /// All available versions.
    pub fn all() -> &'static [SlackwareVersion] {
        &[Self::Current, Self::Slack150, Self::Slack142]
    }

    /// Current (development) only.
    pub fn current() -> &'static [SlackwareVersion] {
        &[Self::Current]
    }

    /// Stable versions only (15.0, 14.2).
    pub fn stable() -> &'static [SlackwareVersion] {
        &[Self::Slack150, Self::Slack142]
    }

    /// Latest stable only (15.0).
    pub fn latest_stable() -> &'static [SlackwareVersion] {
        &[Self::Slack150]
    }
}

/// SlackBuilds package categories.
const CATEGORIES: &[&str] = &[
    "system",
    "development",
    "network",
    "multimedia",
    "desktop",
    "misc",
    "libraries",
    "games",
    "graphics",
    "office",
    "audio",
    "academic",
    "accessibility",
    "business",
    "gis",
    "ham",
    "haskell",
    "perl",
    "python",
    "ruby",
];

/// Slackware package index fetcher (SlackBuilds.org) with configurable versions.
pub struct Slackware {
    versions: Vec<SlackwareVersion>,
}

impl Slackware {
    /// SlackBuilds.org website.
    const SBO_API: &'static str = "https://slackbuilds.org";

    /// Create a fetcher with all versions.
    pub fn all() -> Self {
        Self {
            versions: SlackwareVersion::all().to_vec(),
        }
    }

    /// Create a fetcher with current (development) only.
    pub fn current() -> Self {
        Self {
            versions: SlackwareVersion::current().to_vec(),
        }
    }

    /// Create a fetcher with stable versions only.
    pub fn stable() -> Self {
        Self {
            versions: SlackwareVersion::stable().to_vec(),
        }
    }

    /// Create a fetcher with latest stable only.
    pub fn latest_stable() -> Self {
        Self {
            versions: SlackwareVersion::latest_stable().to_vec(),
        }
    }

    /// Create a fetcher with custom version selection.
    pub fn with_versions(versions: &[SlackwareVersion]) -> Self {
        Self {
            versions: versions.to_vec(),
        }
    }

    /// Fetch a package from a specific version.
    fn fetch_from_version(
        name: &str,
        version: SlackwareVersion,
    ) -> Result<PackageMeta, IndexError> {
        // Try each category to find the package
        for category in CATEGORIES {
            let info_url = format!(
                "https://raw.githubusercontent.com/SlackBuildsOrg/slackbuilds/{}/{}/{}/{}.info",
                version.branch(),
                category,
                name,
                name
            );

            if let Ok(response) = ureq::get(&info_url).call() {
                if let Ok(body) = response.into_string() {
                    return parse_sbo_info(&body, name, category, version);
                }
            }
        }

        Err(IndexError::NotFound(format!(
            "{} in {}",
            name,
            version.name()
        )))
    }
}

impl PackageIndex for Slackware {
    fn ecosystem(&self) -> &'static str {
        "slackware"
    }

    fn display_name(&self) -> &'static str {
        "SlackBuilds.org"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Try each configured version until we find the package
        for &version in &self.versions {
            match Self::fetch_from_version(name, version) {
                Ok(pkg) => return Ok(pkg),
                Err(IndexError::NotFound(_)) => continue,
                Err(e) => return Err(e),
            }
        }

        Err(IndexError::NotFound(name.to_string()))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let mut all_versions = Vec::new();

        // Check each configured version for this package
        for &version in &self.versions {
            if let Ok(pkg) = Self::fetch_from_version(name, version) {
                all_versions.push(VersionMeta {
                    version: format!("{} ({})", pkg.version, version.name()),
                    released: None,
                    yanked: false,
                });
            }
        }

        if all_versions.is_empty() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        Ok(all_versions)
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        // SlackBuilds doesn't have a JSON search API
        // Return an error suggesting to use fetch() directly
        Err(IndexError::Network(format!(
            "SlackBuilds search not implemented via API. Use fetch() with exact package name, or visit: {}/result/?search={}",
            Self::SBO_API,
            query
        )))
    }
}

fn parse_sbo_info(
    content: &str,
    name: &str,
    category: &str,
    version: SlackwareVersion,
) -> Result<PackageMeta, IndexError> {
    let mut pkg_version = String::new();
    let mut homepage = None;
    let mut maintainer = None;
    let mut email = None;

    for line in content.lines() {
        if let Some(val) = line.strip_prefix("VERSION=") {
            pkg_version = val.trim_matches('"').to_string();
        } else if let Some(val) = line.strip_prefix("HOMEPAGE=") {
            homepage = Some(val.trim_matches('"').to_string());
        } else if let Some(val) = line.strip_prefix("MAINTAINER=") {
            maintainer = Some(val.trim_matches('"').to_string());
        } else if let Some(val) = line.strip_prefix("EMAIL=") {
            email = Some(val.trim_matches('"').to_string());
        }
    }

    let maintainers = match (maintainer, email) {
        (Some(m), Some(e)) => vec![format!("{} <{}>", m, e)],
        (Some(m), None) => vec![m],
        _ => Vec::new(),
    };

    let mut extra = HashMap::new();
    extra.insert(
        "source_repo".to_string(),
        serde_json::Value::String(version.name().to_string()),
    );
    extra.insert(
        "category".to_string(),
        serde_json::Value::String(category.to_string()),
    );

    Ok(PackageMeta {
        name: format!("{}/{}", category, name),
        version: pkg_version,
        description: None, // Would need to parse README
        homepage,
        repository: Some(format!(
            "https://github.com/SlackBuildsOrg/slackbuilds/tree/{}/{}/{}",
            version.branch(),
            category,
            name
        )),
        license: None,
        binaries: Vec::new(),
        keywords: Vec::new(),
        maintainers,
        published: None,
        downloads: None,
        archive_url: None,
        checksum: None,
        extra,
    })
}
