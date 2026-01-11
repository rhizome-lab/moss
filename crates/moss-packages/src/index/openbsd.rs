//! OpenBSD package index fetcher (ports).
//!
//! Fetches package metadata from openports.pl.

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// OpenBSD package index fetcher.
pub struct OpenBsd;

impl OpenBsd {
    /// OpenPorts API (unofficial but reliable).
    const OPENPORTS_API: &'static str = "https://openports.pl";
}

impl PackageIndex for OpenBsd {
    fn ecosystem(&self) -> &'static str {
        "openbsd"
    }

    fn display_name(&self) -> &'static str {
        "OpenBSD (ports)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // OpenPorts.pl has a search API
        let url = format!("{}/search.json?q={}", Self::OPENPORTS_API, name);
        let response: serde_json::Value = ureq::get(&url)
            .set("Accept", "application/json")
            .call()?
            .into_json()?;

        let results = response["results"]
            .as_array()
            .or_else(|| response.as_array())
            .ok_or_else(|| IndexError::Parse("missing results".into()))?;

        // Find exact match or first result
        let pkg = results
            .iter()
            .find(|p| p["name"].as_str() == Some(name) || p["pkgname"].as_str() == Some(name))
            .or_else(|| results.first())
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        Ok(PackageMeta {
            name: pkg["pkgname"]
                .as_str()
                .or_else(|| pkg["name"].as_str())
                .unwrap_or(name)
                .to_string(),
            version: pkg["version"].as_str().unwrap_or("unknown").to_string(),
            description: pkg["comment"]
                .as_str()
                .or_else(|| pkg["description"].as_str())
                .map(String::from),
            homepage: pkg["homepage"].as_str().map(String::from),
            repository: None,
            license: None, // OpenBSD doesn't expose license in API typically
            maintainers: pkg["maintainer"]
                .as_str()
                .map(|m| vec![m.to_string()])
                .unwrap_or_default(),
            binaries: Vec::new(),
            ..Default::default()
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // OpenBSD doesn't expose version history via API
        let pkg = self.fetch(name)?;
        Ok(vec![VersionMeta {
            version: pkg.version,
            released: None,
            yanked: false,
        }])
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!("{}/search.json?q={}", Self::OPENPORTS_API, query);
        let response: serde_json::Value = ureq::get(&url)
            .set("Accept", "application/json")
            .call()?
            .into_json()?;

        let results = response["results"]
            .as_array()
            .or_else(|| response.as_array())
            .ok_or_else(|| IndexError::Parse("missing results".into()))?;

        Ok(results
            .iter()
            .filter_map(|pkg| {
                Some(PackageMeta {
                    name: pkg["pkgname"]
                        .as_str()
                        .or_else(|| pkg["name"].as_str())?
                        .to_string(),
                    version: pkg["version"].as_str().unwrap_or("unknown").to_string(),
                    description: pkg["comment"]
                        .as_str()
                        .or_else(|| pkg["description"].as_str())
                        .map(String::from),
                    homepage: pkg["homepage"].as_str().map(String::from),
                    repository: None,
                    license: None,
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
