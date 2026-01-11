//! Winget package index fetcher (Windows Package Manager).
//!
//! Fetches package metadata from the winget-pkgs repository.

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// Winget package index fetcher.
pub struct Winget;

impl Winget {
    /// winget.run API (community).
    const WINGET_RUN_API: &'static str = "https://api.winget.run/v2/packages";
}

impl PackageIndex for Winget {
    fn ecosystem(&self) -> &'static str {
        "winget"
    }

    fn display_name(&self) -> &'static str {
        "Winget (Windows)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Use winget.run API for easier lookups
        let url = format!("{}/{}", Self::WINGET_RUN_API, name);
        let response: serde_json::Value = ureq::get(&url)
            .set("Accept", "application/json")
            .call()?
            .into_json()?;

        if response.get("error").is_some() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        let latest = response["versions"]
            .as_array()
            .and_then(|v| v.first())
            .unwrap_or(&response);

        Ok(PackageMeta {
            name: response["id"].as_str().unwrap_or(name).to_string(),
            version: latest["version"].as_str().unwrap_or("unknown").to_string(),
            description: response["description"].as_str().map(String::from),
            homepage: response["homepage"].as_str().map(String::from),
            repository: response["repository"].as_str().map(String::from),
            license: response["license"].as_str().map(String::from),
            binaries: Vec::new(),
            ..Default::default()
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let url = format!("{}/{}", Self::WINGET_RUN_API, name);
        let response: serde_json::Value = ureq::get(&url)
            .set("Accept", "application/json")
            .call()?
            .into_json()?;

        if response.get("error").is_some() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        let versions = response["versions"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing versions".into()))?;

        Ok(versions
            .iter()
            .filter_map(|v| {
                Some(VersionMeta {
                    version: v["version"].as_str()?.to_string(),
                    released: v["date"].as_str().map(String::from),
                    yanked: false,
                })
            })
            .collect())
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!("{}?q={}", Self::WINGET_RUN_API, query);
        let response: serde_json::Value = ureq::get(&url)
            .set("Accept", "application/json")
            .call()?
            .into_json()?;

        let packages = response["packages"]
            .as_array()
            .or_else(|| response.as_array())
            .ok_or_else(|| IndexError::Parse("missing packages".into()))?;

        Ok(packages
            .iter()
            .filter_map(|pkg| {
                Some(PackageMeta {
                    name: pkg["id"].as_str()?.to_string(),
                    version: pkg["version"].as_str().unwrap_or("unknown").to_string(),
                    description: pkg["description"].as_str().map(String::from),
                    homepage: pkg["homepage"].as_str().map(String::from),
                    repository: pkg["repository"].as_str().map(String::from),
                    license: pkg["license"].as_str().map(String::from),
                    binaries: Vec::new(),
                    ..Default::default()
                })
            })
            .collect())
    }
}
