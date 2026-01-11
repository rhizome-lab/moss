//! Void Linux package index fetcher (xbps).
//!
//! Fetches package metadata from Void Linux repositories.

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// Void Linux package index fetcher.
pub struct Void;

impl Void {
    /// Void Linux packages API.
    const VOID_API: &'static str = "https://voidlinux.org/packages";
}

impl PackageIndex for Void {
    fn ecosystem(&self) -> &'static str {
        "void"
    }

    fn display_name(&self) -> &'static str {
        "Void Linux (xbps)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Void uses xlocate API for package search
        let url = format!("{}/package/{}", Self::VOID_API, name);
        let response: serde_json::Value = ureq::get(&url)
            .set("Accept", "application/json")
            .call()?
            .into_json()?;

        if response.get("error").is_some() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        Ok(PackageMeta {
            name: response["name"].as_str().unwrap_or(name).to_string(),
            version: response["version"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            description: response["short_desc"].as_str().map(String::from),
            homepage: response["homepage"].as_str().map(String::from),
            repository: response["source"].as_str().map(String::from),
            license: response["license"].as_str().map(String::from),
            maintainers: response["maintainer"]
                .as_str()
                .map(|m| vec![m.to_string()])
                .unwrap_or_default(),
            binaries: Vec::new(),
            ..Default::default()
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // Void doesn't expose version history via API
        let pkg = self.fetch(name)?;
        Ok(vec![VersionMeta {
            version: pkg.version,
            released: None,
            yanked: false,
        }])
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!("{}/search?q={}", Self::VOID_API, query);
        let response: serde_json::Value = ureq::get(&url)
            .set("Accept", "application/json")
            .call()?
            .into_json()?;

        let packages = response
            .as_array()
            .or_else(|| response["packages"].as_array())
            .ok_or_else(|| IndexError::Parse("expected array".into()))?;

        Ok(packages
            .iter()
            .filter_map(|pkg| {
                Some(PackageMeta {
                    name: pkg["name"].as_str()?.to_string(),
                    version: pkg["version"].as_str().unwrap_or("unknown").to_string(),
                    description: pkg["short_desc"].as_str().map(String::from),
                    homepage: pkg["homepage"].as_str().map(String::from),
                    repository: pkg["source"].as_str().map(String::from),
                    license: pkg["license"].as_str().map(String::from),
                    maintainers: pkg["maintainer"]
                        .as_str()
                        .map(|m| vec![m.to_string()])
                        .unwrap_or_default(),
                    binaries: Vec::new(),
                    ..Default::default()
                })
            })
            .collect())
    }
}
