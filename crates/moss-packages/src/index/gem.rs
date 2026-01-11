//! RubyGems package index fetcher (Ruby).
//!
//! Fetches package metadata from rubygems.org.

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// RubyGems package index fetcher.
pub struct Gem;

impl Gem {
    /// RubyGems API.
    const RUBYGEMS_API: &'static str = "https://rubygems.org/api/v1";
}

impl PackageIndex for Gem {
    fn ecosystem(&self) -> &'static str {
        "gem"
    }

    fn display_name(&self) -> &'static str {
        "RubyGems (Ruby)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let url = format!("{}/gems/{}.json", Self::RUBYGEMS_API, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        Ok(PackageMeta {
            name: response["name"].as_str().unwrap_or(name).to_string(),
            version: response["version"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            description: response["info"].as_str().map(String::from),
            homepage: response["homepage_uri"].as_str().map(String::from),
            repository: response["source_code_uri"]
                .as_str()
                .or_else(|| {
                    response["homepage_uri"]
                        .as_str()
                        .filter(|u| u.contains("github.com"))
                })
                .map(String::from),
            license: response["licenses"]
                .as_array()
                .and_then(|l| l.first())
                .and_then(|l| l.as_str())
                .map(String::from),
            binaries: response["executables"]
                .as_array()
                .map(|exes| {
                    exes.iter()
                        .filter_map(|e| e.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            ..Default::default()
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let url = format!("{}/versions/{}.json", Self::RUBYGEMS_API, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let versions = response
            .as_array()
            .ok_or_else(|| IndexError::Parse("expected array".into()))?;

        Ok(versions
            .iter()
            .filter_map(|v| {
                Some(VersionMeta {
                    version: v["number"].as_str()?.to_string(),
                    released: v["created_at"].as_str().map(String::from),
                    yanked: v["yanked"].as_bool().unwrap_or(false),
                })
            })
            .collect())
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!("{}/search.json?query={}", Self::RUBYGEMS_API, query);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let gems = response
            .as_array()
            .ok_or_else(|| IndexError::Parse("expected array".into()))?;

        Ok(gems
            .iter()
            .filter_map(|gem| {
                Some(PackageMeta {
                    name: gem["name"].as_str()?.to_string(),
                    version: gem["version"].as_str().unwrap_or("unknown").to_string(),
                    description: gem["info"].as_str().map(String::from),
                    homepage: gem["homepage_uri"].as_str().map(String::from),
                    repository: gem["source_code_uri"]
                        .as_str()
                        .or_else(|| {
                            gem["homepage_uri"]
                                .as_str()
                                .filter(|u| u.contains("github.com"))
                        })
                        .map(String::from),
                    license: gem["licenses"]
                        .as_array()
                        .and_then(|l| l.first())
                        .and_then(|l| l.as_str())
                        .map(String::from),
                    binaries: Vec::new(),
                    ..Default::default()
                })
            })
            .collect())
    }
}
