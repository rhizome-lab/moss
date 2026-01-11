//! Scoop package index fetcher (Windows).
//!
//! Fetches package metadata from Scoop buckets (main, extras, versions).
//!
//! ## API Strategy
//! - **fetch**: GitHub raw bucket manifests
//! - **fetch_versions**: Same API, single version
//! - **search**: `scoop.sh/api/apps?q=`
//! - **fetch_all**: Not supported (no bulk API)
//!
//! ## Multi-bucket Support
//! ```rust,ignore
//! use moss_packages::index::scoop::{Scoop, ScoopBucket};
//!
//! // All buckets (default)
//! let all = Scoop::all();
//!
//! // Main + Extras only
//! let core = Scoop::core();
//!
//! // Main bucket only
//! let main = Scoop::main_only();
//! ```

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use std::collections::HashMap;

/// Available Scoop buckets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScoopBucket {
    /// Main bucket - essential apps
    Main,
    /// Extras - additional apps including GUI apps
    Extras,
    /// Versions - old versions of apps
    Versions,
    /// Games
    Games,
    /// Nerd Fonts
    NerdFonts,
    /// Java - JDKs and Java tools
    Java,
    /// PHP versions
    Php,
    /// Nonportable - apps requiring special install
    Nonportable,
}

impl ScoopBucket {
    /// Get the GitHub repo path for this bucket.
    fn repo_path(&self) -> &'static str {
        match self {
            Self::Main => "ScoopInstaller/Main/master/bucket",
            Self::Extras => "ScoopInstaller/Extras/master/bucket",
            Self::Versions => "ScoopInstaller/Versions/master/bucket",
            Self::Games => "Calinou/scoop-games/master/bucket",
            Self::NerdFonts => "matthewjberger/scoop-nerd-fonts/master/bucket",
            Self::Java => "ScoopInstaller/Java/master/bucket",
            Self::Php => "ScoopInstaller/PHP/master/bucket",
            Self::Nonportable => "ScoopInstaller/Nonportable/master/bucket",
        }
    }

    /// Get the bucket name for tagging.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Extras => "extras",
            Self::Versions => "versions",
            Self::Games => "games",
            Self::NerdFonts => "nerd-fonts",
            Self::Java => "java",
            Self::Php => "php",
            Self::Nonportable => "nonportable",
        }
    }

    /// All available buckets.
    pub fn all() -> &'static [ScoopBucket] {
        &[
            Self::Main,
            Self::Extras,
            Self::Versions,
            Self::Games,
            Self::NerdFonts,
            Self::Java,
            Self::Php,
            Self::Nonportable,
        ]
    }

    /// Core buckets (Main + Extras).
    pub fn core() -> &'static [ScoopBucket] {
        &[Self::Main, Self::Extras]
    }

    /// Main bucket only.
    pub fn main() -> &'static [ScoopBucket] {
        &[Self::Main]
    }

    /// Development-focused buckets.
    pub fn dev() -> &'static [ScoopBucket] {
        &[
            Self::Main,
            Self::Extras,
            Self::Versions,
            Self::Java,
            Self::Php,
        ]
    }
}

/// Scoop package index fetcher with configurable buckets.
pub struct Scoop {
    buckets: Vec<ScoopBucket>,
}

impl Scoop {
    /// Scoop search API.
    const SCOOP_API: &'static str = "https://scoop.sh/api";

    /// GitHub raw content for bucket manifests.
    const GITHUB_RAW: &'static str = "https://raw.githubusercontent.com";

    /// Create a fetcher with all buckets.
    pub fn all() -> Self {
        Self {
            buckets: ScoopBucket::all().to_vec(),
        }
    }

    /// Create a fetcher with core buckets (Main + Extras).
    pub fn core() -> Self {
        Self {
            buckets: ScoopBucket::core().to_vec(),
        }
    }

    /// Create a fetcher with main bucket only.
    pub fn main_only() -> Self {
        Self {
            buckets: ScoopBucket::main().to_vec(),
        }
    }

    /// Create a fetcher with development-focused buckets.
    pub fn dev() -> Self {
        Self {
            buckets: ScoopBucket::dev().to_vec(),
        }
    }

    /// Create a fetcher with custom bucket selection.
    pub fn with_buckets(buckets: &[ScoopBucket]) -> Self {
        Self {
            buckets: buckets.to_vec(),
        }
    }

    /// Fetch a package from a specific bucket.
    fn fetch_from_bucket(name: &str, bucket: ScoopBucket) -> Result<PackageMeta, IndexError> {
        let url = format!("{}/{}/{}.json", Self::GITHUB_RAW, bucket.repo_path(), name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let mut extra = HashMap::new();
        extra.insert(
            "source_repo".to_string(),
            serde_json::Value::String(bucket.name().to_string()),
        );

        Ok(PackageMeta {
            name: name.to_string(),
            version: response["version"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            description: response["description"].as_str().map(String::from),
            homepage: response["homepage"].as_str().map(String::from),
            repository: response["repository"]
                .as_str()
                .or_else(|| response["checkver"]["github"].as_str())
                .map(|s| {
                    if s.starts_with("http") {
                        s.to_string()
                    } else {
                        format!("https://github.com/{}", s)
                    }
                }),
            license: response["license"]
                .as_str()
                .or_else(|| response["license"]["identifier"].as_str())
                .map(String::from),
            binaries: response["bin"]
                .as_array()
                .map(|bins| {
                    bins.iter()
                        .filter_map(|b| {
                            b.as_str()
                                .or_else(|| b.as_array().and_then(|a| a.first()?.as_str()))
                                .map(|s| {
                                    // Extract just the binary name from path
                                    s.rsplit(['/', '\\']).next().unwrap_or(s).to_string()
                                })
                        })
                        .collect()
                })
                .unwrap_or_default(),
            keywords: Vec::new(),
            maintainers: Vec::new(),
            published: None,
            downloads: None,
            archive_url: None,
            checksum: None,
            extra,
        })
    }
}

impl PackageIndex for Scoop {
    fn ecosystem(&self) -> &'static str {
        "scoop"
    }

    fn display_name(&self) -> &'static str {
        "Scoop (Windows)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Try each configured bucket until we find the package
        for &bucket in &self.buckets {
            match Self::fetch_from_bucket(name, bucket) {
                Ok(pkg) => return Ok(pkg),
                Err(IndexError::Network(_)) => continue,
                Err(e) => return Err(e),
            }
        }

        Err(IndexError::NotFound(name.to_string()))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // Scoop manifests only contain current version
        // Check all configured buckets for available versions
        let mut versions = Vec::new();

        for &bucket in &self.buckets {
            if let Ok(pkg) = Self::fetch_from_bucket(name, bucket) {
                versions.push(VersionMeta {
                    version: format!("{} ({})", pkg.version, bucket.name()),
                    released: None,
                    yanked: false,
                });
            }
        }

        if versions.is_empty() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        Ok(versions)
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        // Use the scoop.sh search API
        let url = format!("{}/apps?q={}", Self::SCOOP_API, query);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let apps = response
            .as_array()
            .ok_or_else(|| IndexError::Parse("expected array".into()))?;

        // Filter to configured buckets
        let bucket_names: Vec<_> = self.buckets.iter().map(|b| b.name()).collect();

        Ok(apps
            .iter()
            .filter(|app| {
                app["bucket"]
                    .as_str()
                    .map(|b| bucket_names.contains(&b))
                    .unwrap_or(true) // Include if no bucket specified
            })
            .filter_map(|app| {
                let mut extra = HashMap::new();
                if let Some(bucket) = app["bucket"].as_str() {
                    extra.insert(
                        "source_repo".to_string(),
                        serde_json::Value::String(bucket.to_string()),
                    );
                }

                Some(PackageMeta {
                    name: app["name"].as_str()?.to_string(),
                    version: app["version"].as_str().unwrap_or("unknown").to_string(),
                    description: app["description"].as_str().map(String::from),
                    homepage: app["homepage"].as_str().map(String::from),
                    repository: None,
                    license: app["license"].as_str().map(String::from),
                    binaries: Vec::new(),
                    keywords: Vec::new(),
                    maintainers: Vec::new(),
                    published: None,
                    downloads: None,
                    archive_url: None,
                    checksum: None,
                    extra,
                })
            })
            .collect())
    }
}
