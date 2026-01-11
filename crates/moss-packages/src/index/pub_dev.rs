//! pub.dev package index fetcher (Dart/Flutter).
//!
//! Fetches package metadata from pub.dev API.
//!
//! ## API Strategy
//! - **fetch**: `pub.dev/api/packages/{name}` - Official pub.dev JSON API
//! - **fetch_versions**: Same API, extracts versions array
//! - **search**: `pub.dev/api/search?q=`
//! - **fetch_all**: Not supported (too large)

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// pub.dev package index fetcher.
pub struct Pub;

impl Pub {
    /// pub.dev API base.
    const API_BASE: &'static str = "https://pub.dev/api";
}

impl PackageIndex for Pub {
    fn ecosystem(&self) -> &'static str {
        "pub"
    }

    fn display_name(&self) -> &'static str {
        "pub.dev (Dart/Flutter)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let url = format!("{}/packages/{}", Self::API_BASE, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let latest = &response["latest"];
        let pubspec = &latest["pubspec"];

        Ok(PackageMeta {
            name: response["name"].as_str().unwrap_or(name).to_string(),
            version: latest["version"].as_str().unwrap_or("unknown").to_string(),
            description: pubspec["description"].as_str().map(String::from),
            homepage: pubspec["homepage"].as_str().map(String::from),
            repository: pubspec["repository"].as_str().map(String::from),
            license: None, // pub.dev doesn't expose license in API
            binaries: Vec::new(),
            keywords: Vec::new(), // pub.dev doesn't have keywords
            maintainers: pubspec["authors"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|author| author.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            published: latest["published"].as_str().map(String::from),
            downloads: None, // pub.dev doesn't expose download counts
            archive_url: latest["archive_url"].as_str().map(String::from),
            checksum: latest["archive_sha256"]
                .as_str()
                .map(|h| format!("sha256:{}", h)),
            extra: Default::default(),
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let url = format!("{}/packages/{}", Self::API_BASE, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let versions = response["versions"]
            .as_array()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        Ok(versions
            .iter()
            .filter_map(|v| {
                Some(VersionMeta {
                    version: v["version"].as_str()?.to_string(),
                    released: v["published"].as_str().map(String::from),
                    yanked: v["retracted"].as_bool().unwrap_or(false),
                })
            })
            .collect())
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!("{}/search?q={}", Self::API_BASE, urlencoding::encode(query));
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let packages = response["packages"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing packages".into()))?;

        // Search only returns package names, need to fetch details for each
        // For efficiency, just return names with unknown version
        Ok(packages
            .iter()
            .take(50)
            .filter_map(|pkg| {
                Some(PackageMeta {
                    name: pkg["package"].as_str()?.to_string(),
                    version: "unknown".to_string(), // Search only returns names
                    description: None,
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
                    extra: Default::default(),
                })
            })
            .collect())
    }
}
