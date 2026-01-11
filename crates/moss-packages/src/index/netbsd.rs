//! NetBSD package index fetcher (pkgsrc).
//!
//! Fetches package metadata from pkgsrc.se.

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// NetBSD package index fetcher.
pub struct NetBsd;

impl NetBsd {
    /// pkgsrc.se API.
    const PKGSRC_API: &'static str = "https://pkgsrc.se";
}

impl PackageIndex for NetBsd {
    fn ecosystem(&self) -> &'static str {
        "netbsd"
    }

    fn display_name(&self) -> &'static str {
        "NetBSD (pkgsrc)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // pkgsrc.se has a search API
        let url = format!("{}/search.json?q={}", Self::PKGSRC_API, name);
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

        // pkgsrc uses category/name format
        let full_name = if let Some(cat) = pkg["category"].as_str() {
            format!("{}/{}", cat, pkg["name"].as_str().unwrap_or(name))
        } else {
            pkg["name"].as_str().unwrap_or(name).to_string()
        };

        Ok(PackageMeta {
            name: full_name,
            version: pkg["version"].as_str().unwrap_or("unknown").to_string(),
            description: pkg["comment"]
                .as_str()
                .or_else(|| pkg["description"].as_str())
                .map(String::from),
            homepage: pkg["homepage"].as_str().map(String::from),
            repository: None,
            license: pkg["license"].as_str().map(String::from),
            maintainers: pkg["maintainer"]
                .as_str()
                .map(|m| vec![m.to_string()])
                .unwrap_or_default(),
            binaries: Vec::new(),
            ..Default::default()
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // pkgsrc doesn't expose version history via API
        let pkg = self.fetch(name)?;
        Ok(vec![VersionMeta {
            version: pkg.version,
            released: None,
            yanked: false,
        }])
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!("{}/search.json?q={}", Self::PKGSRC_API, query);
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
                let name = pkg["name"].as_str()?;
                let full_name = if let Some(cat) = pkg["category"].as_str() {
                    format!("{}/{}", cat, name)
                } else {
                    name.to_string()
                };

                Some(PackageMeta {
                    name: full_name,
                    version: pkg["version"].as_str().unwrap_or("unknown").to_string(),
                    description: pkg["comment"]
                        .as_str()
                        .or_else(|| pkg["description"].as_str())
                        .map(String::from),
                    homepage: pkg["homepage"].as_str().map(String::from),
                    repository: None,
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
