//! Packagist package index fetcher (PHP/Composer).
//!
//! Fetches package metadata from packagist.org.
//!
//! ## API Strategy
//! - **fetch**: `packagist.org/packages/{vendor}/{name}.json` - Official JSON API
//! - **fetch_versions**: Same API, extracts versions object
//! - **search**: `packagist.org/search.json?q=` - Official search endpoint
//! - **fetch_all**: Not supported (too large)

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// Packagist package index fetcher.
pub struct Composer;

impl Composer {
    /// Packagist API.
    const PACKAGIST_API: &'static str = "https://packagist.org";
}

impl PackageIndex for Composer {
    fn ecosystem(&self) -> &'static str {
        "composer"
    }

    fn display_name(&self) -> &'static str {
        "Packagist (PHP)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let url = format!("{}/packages/{}.json", Self::PACKAGIST_API, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let package = &response["package"];
        let versions = package["versions"].as_object();

        // Find latest stable version (not dev-*)
        let latest_version = versions
            .and_then(|v| {
                v.keys().filter(|k| !k.starts_with("dev-")).max_by(|a, b| {
                    // Simple version comparison
                    a.cmp(b)
                })
            })
            .cloned();

        let version_str = latest_version.as_deref().unwrap_or("unknown");
        let version_info = versions.and_then(|v| v.get(version_str));

        Ok(PackageMeta {
            name: package["name"].as_str().unwrap_or(name).to_string(),
            version: version_str.to_string(),
            description: package["description"].as_str().map(String::from),
            homepage: version_info
                .and_then(|v| v["homepage"].as_str())
                .map(String::from),
            repository: package["repository"].as_str().map(String::from),
            license: version_info
                .and_then(|v| v["license"].as_array())
                .and_then(|l| l.first())
                .and_then(|l| l.as_str())
                .map(String::from),
            binaries: version_info
                .and_then(|v| v["bin"].as_array())
                .map(|bins| {
                    bins.iter()
                        .filter_map(|b| b.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            keywords: version_info
                .and_then(|v| v["keywords"].as_array())
                .map(|kw| {
                    kw.iter()
                        .filter_map(|k| k.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            maintainers: version_info
                .and_then(|v| v["authors"].as_array())
                .map(|authors| {
                    authors
                        .iter()
                        .filter_map(|a| a["name"].as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            published: version_info
                .and_then(|v| v["time"].as_str())
                .map(String::from),
            downloads: package["downloads"]["total"].as_u64(),
            archive_url: version_info
                .and_then(|v| v["dist"]["url"].as_str())
                .map(String::from),
            checksum: version_info
                .and_then(|v| v["dist"]["shasum"].as_str())
                .filter(|s| !s.is_empty())
                .map(|h| format!("sha1:{}", h)),
            extra: Default::default(),
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let url = format!("{}/packages/{}.json", Self::PACKAGIST_API, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let versions = response["package"]["versions"]
            .as_object()
            .ok_or_else(|| IndexError::Parse("missing versions".into()))?;

        Ok(versions
            .iter()
            .filter(|(k, _)| !k.starts_with("dev-"))
            .map(|(version, info)| VersionMeta {
                version: version.clone(),
                released: info["time"].as_str().map(String::from),
                yanked: info["abandoned"].as_bool().unwrap_or(false),
            })
            .collect())
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!("{}/search.json?q={}", Self::PACKAGIST_API, query);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let results = response["results"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing results".into()))?;

        Ok(results
            .iter()
            .filter_map(|pkg| {
                Some(PackageMeta {
                    name: pkg["name"].as_str()?.to_string(),
                    version: pkg["version"].as_str().unwrap_or("unknown").to_string(),
                    description: pkg["description"].as_str().map(String::from),
                    homepage: pkg["url"].as_str().map(String::from),
                    repository: pkg["repository"].as_str().map(String::from),
                    license: None, // Not in search results
                    binaries: Vec::new(),
                    keywords: Vec::new(),
                    maintainers: Vec::new(),
                    published: None,
                    downloads: pkg["downloads"].as_u64(),
                    archive_url: None,
                    checksum: None,
                    extra: Default::default(),
                })
            })
            .collect())
    }
}
