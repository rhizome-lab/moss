//! F-Droid package index fetcher (Android FOSS).
//!
//! Fetches package metadata from the F-Droid repository API.

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// F-Droid package index fetcher.
pub struct FDroid;

impl FDroid {
    /// F-Droid API base URL.
    const API_BASE: &'static str = "https://f-droid.org/api/v1";
    /// F-Droid search API.
    const SEARCH_API: &'static str = "https://search.f-droid.org/api";
}

impl PackageIndex for FDroid {
    fn ecosystem(&self) -> &'static str {
        "fdroid"
    }

    fn display_name(&self) -> &'static str {
        "F-Droid (Android FOSS)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let url = format!("{}/packages/{}", Self::API_BASE, name);
        let response: serde_json::Value = ureq::get(&url)
            .call()
            .map_err(|_| IndexError::NotFound(name.to_string()))?
            .into_json()?;

        // Get the suggested/latest version
        let packages = response["packages"]
            .as_array()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        let suggested_code = response["suggestedVersionCode"].as_u64();
        let latest = packages
            .iter()
            .find(|p| p["versionCode"].as_u64() == suggested_code)
            .or_else(|| packages.first())
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        Ok(PackageMeta {
            name: response["packageName"].as_str().unwrap_or(name).to_string(),
            version: latest["versionName"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            description: None, // Not available in this API endpoint
            homepage: Some(format!("https://f-droid.org/packages/{}", name)),
            repository: None,
            license: None,
            binaries: Vec::new(),
            keywords: Vec::new(),
            maintainers: Vec::new(),
            ..Default::default()
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let url = format!("{}/packages/{}", Self::API_BASE, name);
        let response: serde_json::Value = ureq::get(&url)
            .call()
            .map_err(|_| IndexError::NotFound(name.to_string()))?
            .into_json()?;

        let packages = response["packages"]
            .as_array()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        Ok(packages
            .iter()
            .filter_map(|p| {
                Some(VersionMeta {
                    version: p["versionName"].as_str()?.to_string(),
                    released: None,
                    yanked: false,
                })
            })
            .collect())
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!("{}/search_apps?q={}", Self::SEARCH_API, query);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let apps = response["apps"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("Invalid search response".into()))?;

        Ok(apps
            .iter()
            .filter_map(|app| {
                // Extract package name from URL: https://f-droid.org/en/packages/{id}
                let url = app["url"].as_str()?;
                let package_name = url.rsplit('/').next()?;

                Some(PackageMeta {
                    name: package_name.to_string(),
                    version: "latest".to_string(),
                    description: app["summary"].as_str().map(String::from),
                    homepage: Some(url.to_string()),
                    ..Default::default()
                })
            })
            .collect())
    }
}
