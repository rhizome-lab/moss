//! openSUSE package index fetcher (zypper).
//!
//! Fetches package metadata from software.opensuse.org.

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// openSUSE package index fetcher.
pub struct OpenSuse;

impl OpenSuse {
    /// software.opensuse.org search.
    const SEARCH_API: &'static str = "https://software.opensuse.org/search/json";
}

impl PackageIndex for OpenSuse {
    fn ecosystem(&self) -> &'static str {
        "opensuse"
    }

    fn display_name(&self) -> &'static str {
        "openSUSE (zypper)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Use software.opensuse.org search API
        let url = format!(
            "{}?q={}&baseproject=openSUSE:Tumbleweed",
            Self::SEARCH_API,
            name
        );
        let response: serde_json::Value = ureq::get(&url)
            .set("Accept", "application/json")
            .call()?
            .into_json()?;

        let packages = response["packages"]
            .as_array()
            .or_else(|| response.as_array())
            .ok_or_else(|| IndexError::Parse("missing packages".into()))?;

        // Find exact match or first result
        let pkg = packages
            .iter()
            .find(|p| p["name"].as_str() == Some(name))
            .or_else(|| packages.first())
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        Ok(PackageMeta {
            name: pkg["name"].as_str().unwrap_or(name).to_string(),
            version: pkg["version"].as_str().unwrap_or("unknown").to_string(),
            description: pkg["description"]
                .as_str()
                .or_else(|| pkg["summary"].as_str())
                .map(String::from),
            homepage: pkg["url"].as_str().map(String::from),
            repository: None,
            license: pkg["license"].as_str().map(String::from),
            binaries: Vec::new(),
            ..Default::default()
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // openSUSE doesn't expose version history via public API easily
        let pkg = self.fetch(name)?;
        Ok(vec![VersionMeta {
            version: pkg.version,
            released: None,
            yanked: false,
        }])
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!("{}?q={}", Self::SEARCH_API, query);
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
                    name: pkg["name"].as_str()?.to_string(),
                    version: pkg["version"].as_str().unwrap_or("unknown").to_string(),
                    description: pkg["description"]
                        .as_str()
                        .or_else(|| pkg["summary"].as_str())
                        .map(String::from),
                    homepage: pkg["url"].as_str().map(String::from),
                    repository: None,
                    license: pkg["license"].as_str().map(String::from),
                    binaries: Vec::new(),
                    ..Default::default()
                })
            })
            .collect())
    }
}
