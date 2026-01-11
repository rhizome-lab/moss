//! FreeBSD package index fetcher (pkg).
//!
//! Fetches package metadata from freshports.org and pkg.freebsd.org.

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// FreeBSD package index fetcher.
pub struct FreeBsd;

impl FreeBsd {
    /// FreshPorts API.
    const FRESHPORTS_API: &'static str = "https://www.freshports.org";
}

impl PackageIndex for FreeBsd {
    fn ecosystem(&self) -> &'static str {
        "freebsd"
    }

    fn display_name(&self) -> &'static str {
        "FreeBSD (pkg)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // FreshPorts search with JSON output
        let search_url = format!(
            "{}/search.php?query={}&format=json",
            Self::FRESHPORTS_API,
            name
        );
        let response: serde_json::Value = ureq::get(&search_url)
            .set("Accept", "application/json")
            .call()?
            .into_json()?;

        let results = response["results"]
            .as_array()
            .or_else(|| response.as_array())
            .ok_or_else(|| IndexError::Parse("missing results".into()))?;

        let pkg = results
            .iter()
            .find(|p| p["name"].as_str() == Some(name) || p["package-name"].as_str() == Some(name))
            .or_else(|| results.first())
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        Ok(PackageMeta {
            name: pkg["package-name"]
                .as_str()
                .or_else(|| pkg["name"].as_str())
                .unwrap_or(name)
                .to_string(),
            version: pkg["version"].as_str().unwrap_or("unknown").to_string(),
            description: pkg["short_description"]
                .as_str()
                .or_else(|| pkg["comment"].as_str())
                .map(String::from),
            homepage: pkg["homepage"].as_str().map(String::from),
            repository: None,
            license: pkg["license"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|l| l.as_str())
                .or_else(|| pkg["license"].as_str())
                .map(String::from),
            maintainers: pkg["maintainer"]
                .as_str()
                .map(|m| vec![m.to_string()])
                .unwrap_or_default(),
            binaries: Vec::new(),
            ..Default::default()
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // FreeBSD doesn't expose version history easily via API
        let pkg = self.fetch(name)?;
        Ok(vec![VersionMeta {
            version: pkg.version,
            released: None,
            yanked: false,
        }])
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!(
            "{}/search.php?query={}&format=json",
            Self::FRESHPORTS_API,
            query
        );
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
                    name: pkg["package-name"]
                        .as_str()
                        .or_else(|| pkg["name"].as_str())?
                        .to_string(),
                    version: pkg["version"].as_str().unwrap_or("unknown").to_string(),
                    description: pkg["short_description"]
                        .as_str()
                        .or_else(|| pkg["comment"].as_str())
                        .map(String::from),
                    homepage: pkg["homepage"].as_str().map(String::from),
                    repository: None,
                    license: pkg["license"]
                        .as_array()
                        .and_then(|a| a.first())
                        .and_then(|l| l.as_str())
                        .or_else(|| pkg["license"].as_str())
                        .map(String::from),
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
