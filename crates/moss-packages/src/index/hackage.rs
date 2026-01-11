//! Hackage package index fetcher (Haskell).
//!
//! Fetches package metadata from hackage.haskell.org.

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// Hackage package index fetcher.
pub struct Hackage;

impl Hackage {
    /// Hackage API base.
    const API_BASE: &'static str = "https://hackage.haskell.org";
}

impl PackageIndex for Hackage {
    fn ecosystem(&self) -> &'static str {
        "hackage"
    }

    fn display_name(&self) -> &'static str {
        "Hackage (Haskell)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Hackage package info endpoint
        let url = format!("{}/package/{}", Self::API_BASE, name);
        let response: serde_json::Value = ureq::get(&url)
            .set("Accept", "application/json")
            .call()?
            .into_json()?;

        // Get latest version from versions endpoint
        let versions_url = format!("{}/package/{}/preferred", Self::API_BASE, name);
        let versions: serde_json::Value = ureq::get(&versions_url)
            .set("Accept", "application/json")
            .call()
            .ok()
            .and_then(|r| r.into_json().ok())
            .unwrap_or_default();

        let latest_version = versions["normal-version"]
            .as_array()
            .and_then(|v| v.first())
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        Ok(PackageMeta {
            name: response["packageName"].as_str().unwrap_or(name).to_string(),
            version: latest_version.to_string(),
            description: response["packageDescription"].as_str().map(String::from),
            homepage: response["packageHomepage"].as_str().map(String::from),
            repository: response["packageSourceRepository"]
                .as_str()
                .map(String::from),
            license: response["license"].as_str().map(String::from),
            binaries: Vec::new(),
            archive_url: Some(format!(
                "{}/package/{}-{}/{}-{}.tar.gz",
                Self::API_BASE,
                name,
                latest_version,
                name,
                latest_version
            )),
            ..Default::default()
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let url = format!("{}/package/{}/preferred", Self::API_BASE, name);
        let response: serde_json::Value = ureq::get(&url)
            .set("Accept", "application/json")
            .call()?
            .into_json()?;

        let normal = response["normal-version"]
            .as_array()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        let deprecated = response["deprecated-version"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(String::from)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        Ok(normal
            .iter()
            .filter_map(|v| {
                let version = v.as_str()?.to_string();
                Some(VersionMeta {
                    yanked: deprecated.contains(&version),
                    version,
                    released: None,
                })
            })
            .collect())
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        // Hackage search endpoint
        let url = format!(
            "{}/packages/search?terms={}",
            Self::API_BASE,
            urlencoding::encode(query)
        );
        let response: serde_json::Value = ureq::get(&url)
            .set("Accept", "application/json")
            .call()?
            .into_json()?;

        let packages = response
            .as_array()
            .ok_or_else(|| IndexError::Parse("expected array".into()))?;

        Ok(packages
            .iter()
            .take(50)
            .filter_map(|pkg| {
                Some(PackageMeta {
                    name: pkg["name"].as_str()?.to_string(),
                    version: "unknown".to_string(), // Search doesn't return version
                    description: pkg["synopsis"].as_str().map(String::from),
                    homepage: None,
                    repository: None,
                    license: None,
                    binaries: Vec::new(),
                    ..Default::default()
                })
            })
            .collect())
    }
}
