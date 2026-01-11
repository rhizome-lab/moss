//! Conan package index fetcher (C/C++).
//!
//! Fetches package metadata from ConanCenter.

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// Conan package index fetcher.
pub struct Conan;

impl Conan {
    /// ConanCenter API.
    const CONAN_API: &'static str = "https://center2.conan.io/api/ui";
}

impl PackageIndex for Conan {
    fn ecosystem(&self) -> &'static str {
        "conan"
    }

    fn display_name(&self) -> &'static str {
        "Conan (C/C++)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let url = format!("{}/recipes/{}", Self::CONAN_API, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let versions = response["versions"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing versions".into()))?;

        let latest = versions
            .first()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        Ok(PackageMeta {
            name: response["name"].as_str().unwrap_or(name).to_string(),
            version: latest["version"].as_str().unwrap_or("unknown").to_string(),
            description: response["description"].as_str().map(String::from),
            homepage: response["homepage"].as_str().map(String::from),
            repository: response["url"]
                .as_str()
                .filter(|u| u.contains("github.com") || u.contains("gitlab.com"))
                .map(String::from),
            license: response["license"]
                .as_str()
                .or_else(|| {
                    response["licenses"]
                        .as_array()
                        .and_then(|l| l.first())
                        .and_then(|l| l.as_str())
                })
                .map(String::from),
            binaries: Vec::new(),
            ..Default::default()
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let url = format!("{}/recipes/{}", Self::CONAN_API, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let versions = response["versions"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing versions".into()))?;

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
        let url = format!("{}/recipes?q={}", Self::CONAN_API, query);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let recipes = response["recipes"]
            .as_array()
            .or_else(|| response.as_array())
            .ok_or_else(|| IndexError::Parse("missing recipes".into()))?;

        Ok(recipes
            .iter()
            .filter_map(|recipe| {
                Some(PackageMeta {
                    name: recipe["name"].as_str()?.to_string(),
                    version: recipe["versions"]
                        .as_array()
                        .and_then(|v| v.first())
                        .and_then(|v| v["version"].as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    description: recipe["description"].as_str().map(String::from),
                    homepage: recipe["homepage"].as_str().map(String::from),
                    repository: recipe["url"]
                        .as_str()
                        .filter(|u| u.contains("github.com") || u.contains("gitlab.com"))
                        .map(String::from),
                    license: recipe["license"].as_str().map(String::from),
                    binaries: Vec::new(),
                    ..Default::default()
                })
            })
            .collect())
    }
}
