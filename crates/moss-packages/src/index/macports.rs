//! MacPorts package index fetcher (macOS).
//!
//! Fetches package metadata from the MacPorts ports API.
//!
//! ## API Strategy
//! - **fetch**: `ports.macports.org/api/v1/ports/{name}` - Official MacPorts JSON API
//! - **fetch_versions**: Same API, extracts version info
//! - **search**: `ports.macports.org/api/v1/ports/?search=`
//! - **fetch_all**: `ports.macports.org/api/v1/ports/` (all ports)

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// MacPorts package index fetcher.
pub struct MacPorts;

impl MacPorts {
    /// MacPorts API base URL.
    const API_BASE: &'static str = "https://ports.macports.org/api/v1";
}

impl PackageIndex for MacPorts {
    fn ecosystem(&self) -> &'static str {
        "macports"
    }

    fn display_name(&self) -> &'static str {
        "MacPorts (macOS)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let url = format!("{}/ports/{}/", Self::API_BASE, name);
        let response: serde_json::Value = ureq::get(&url)
            .call()
            .map_err(|_| IndexError::NotFound(name.to_string()))?
            .into_json()?;

        // Extract maintainers
        let maintainers: Vec<String> = response["maintainers"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| {
                        m["github"]
                            .as_str()
                            .or(m["name"].as_str())
                            .map(String::from)
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Extract categories as keywords
        let keywords: Vec<String> = response["categories"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| c.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        Ok(PackageMeta {
            name: response["name"].as_str().unwrap_or(name).to_string(),
            version: response["version"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            description: response["description"]
                .as_str()
                .or(response["long_description"].as_str())
                .map(String::from),
            homepage: response["homepage"].as_str().map(String::from),
            repository: None, // MacPorts doesn't expose this directly
            license: response["license"].as_str().map(String::from),
            binaries: Vec::new(),
            keywords,
            maintainers,
            published: None,
            downloads: None,
            archive_url: None,
            checksum: None,
            extra: Default::default(),
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // MacPorts API only provides current version
        let meta = self.fetch(name)?;
        Ok(vec![VersionMeta {
            version: meta.version,
            released: None,
            yanked: false,
        }])
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!("{}/ports/?name__contains={}", Self::API_BASE, query);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let results = response["results"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("Invalid search response".into()))?;

        Ok(results
            .iter()
            .filter_map(|port| {
                let keywords: Vec<String> = port["categories"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|c| c.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                Some(PackageMeta {
                    name: port["name"].as_str()?.to_string(),
                    version: port["version"].as_str().unwrap_or("unknown").to_string(),
                    description: port["description"].as_str().map(String::from),
                    homepage: port["homepage"].as_str().map(String::from),
                    repository: None,
                    license: port["license"].as_str().map(String::from),
                    binaries: Vec::new(),
                    keywords,
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
