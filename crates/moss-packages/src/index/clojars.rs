//! Clojars package index fetcher (Clojure/Java).
//!
//! Fetches package metadata from the Clojars repository API.
//!
//! ## API Strategy
//! - **fetch**: `clojars.org/api/artifacts/{group}/{name}` - Official JSON API
//! - **fetch_versions**: Same API, extracts recent_versions array
//! - **search**: `clojars.org/api/search?q=` - Official search endpoint
//! - **fetch_all**: Not supported (no bulk endpoint)

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// Clojars package index fetcher.
pub struct Clojars;

impl Clojars {
    /// Clojars API base URL.
    const API_BASE: &'static str = "https://clojars.org/api";
}

impl PackageIndex for Clojars {
    fn ecosystem(&self) -> &'static str {
        "clojars"
    }

    fn display_name(&self) -> &'static str {
        "Clojars (Clojure)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Clojars uses group/artifact format, e.g., "ring/ring-core" or just "ring"
        let url = format!("{}/artifacts/{}", Self::API_BASE, name);
        let response: serde_json::Value = ureq::get(&url)
            .call()
            .map_err(|_| IndexError::NotFound(name.to_string()))?
            .into_json()?;

        // Extract license info
        let license = response["licenses"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|l| l["name"].as_str())
            .map(String::from);

        // Extract maintainer/user
        let maintainers: Vec<String> = response["user"]
            .as_str()
            .map(|u| vec![u.to_string()])
            .unwrap_or_default();

        Ok(PackageMeta {
            name: format!(
                "{}/{}",
                response["group_name"].as_str().unwrap_or(""),
                response["jar_name"].as_str().unwrap_or(name)
            ),
            version: response["latest_version"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            description: response["description"].as_str().map(String::from),
            homepage: response["homepage"].as_str().map(String::from),
            repository: response["scm"]
                .get("url")
                .and_then(|u| u.as_str())
                .map(String::from),
            license,
            binaries: Vec::new(),
            keywords: Vec::new(),
            maintainers,
            published: None,
            downloads: response["downloads"].as_u64(),
            archive_url: None,
            checksum: None,
            extra: Default::default(),
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let url = format!("{}/artifacts/{}", Self::API_BASE, name);
        let response: serde_json::Value = ureq::get(&url)
            .call()
            .map_err(|_| IndexError::NotFound(name.to_string()))?
            .into_json()?;

        let versions = response["recent_versions"]
            .as_array()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        Ok(versions
            .iter()
            .filter_map(|v| {
                Some(VersionMeta {
                    version: v["version"].as_str()?.to_string(),
                    released: None,
                    yanked: false,
                })
            })
            .collect())
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        // Clojars search uses a different endpoint format
        let url = format!("https://clojars.org/search?q={}&format=json", query);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let results = response["results"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("Invalid search response".into()))?;

        Ok(results
            .iter()
            .filter_map(|pkg| {
                Some(PackageMeta {
                    name: format!(
                        "{}/{}",
                        pkg["group_name"].as_str()?,
                        pkg["jar_name"].as_str()?
                    ),
                    version: pkg["version"].as_str().unwrap_or("unknown").to_string(),
                    description: pkg["description"].as_str().map(String::from),
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
