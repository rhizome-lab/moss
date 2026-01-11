//! NuGet package index fetcher (.NET).
//!
//! Fetches package metadata from nuget.org.

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// NuGet package index fetcher.
pub struct Nuget;

impl Nuget {
    /// NuGet API v3.
    const NUGET_API: &'static str = "https://api.nuget.org/v3";
}

impl PackageIndex for Nuget {
    fn ecosystem(&self) -> &'static str {
        "nuget"
    }

    fn display_name(&self) -> &'static str {
        "NuGet (.NET)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let name_lower = name.to_lowercase();
        let url = format!(
            "{}/registration5-semver1/{}/index.json",
            Self::NUGET_API,
            name_lower
        );
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        // Get the latest catalog entry
        let items = response["items"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing items".into()))?;

        let latest_page = items
            .last()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        let page_items = latest_page["items"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing page items".into()))?;

        let latest = page_items
            .last()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        let catalog = &latest["catalogEntry"];

        Ok(PackageMeta {
            name: catalog["id"].as_str().unwrap_or(name).to_string(),
            version: catalog["version"].as_str().unwrap_or("unknown").to_string(),
            description: catalog["description"].as_str().map(String::from),
            homepage: catalog["projectUrl"].as_str().map(String::from),
            repository: catalog["repository"]
                .as_str()
                .or_else(|| {
                    // Try to extract from projectUrl if it's a GitHub link
                    catalog["projectUrl"]
                        .as_str()
                        .filter(|u| u.contains("github.com"))
                })
                .map(String::from),
            license: catalog["licenseExpression"]
                .as_str()
                .or_else(|| catalog["licenseUrl"].as_str())
                .map(String::from),
            binaries: Vec::new(),
            ..Default::default()
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let name_lower = name.to_lowercase();
        let url = format!(
            "{}/registration5-semver1/{}/index.json",
            Self::NUGET_API,
            name_lower
        );
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let items = response["items"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing items".into()))?;

        let mut versions = Vec::new();

        for page in items {
            if let Some(page_items) = page["items"].as_array() {
                for item in page_items {
                    let catalog = &item["catalogEntry"];
                    if let Some(version) = catalog["version"].as_str() {
                        versions.push(VersionMeta {
                            version: version.to_string(),
                            released: catalog["published"].as_str().map(String::from),
                            yanked: catalog["listed"].as_bool().map(|l| !l).unwrap_or(false),
                        });
                    }
                }
            }
        }

        Ok(versions)
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!(
            "{}-flatcontainer/query?q={}&take=50",
            Self::NUGET_API,
            query
        );
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let data = response["data"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing data".into()))?;

        Ok(data
            .iter()
            .filter_map(|pkg| {
                Some(PackageMeta {
                    name: pkg["id"].as_str()?.to_string(),
                    version: pkg["version"].as_str().unwrap_or("unknown").to_string(),
                    description: pkg["description"].as_str().map(String::from),
                    homepage: pkg["projectUrl"].as_str().map(String::from),
                    repository: None,
                    license: pkg["licenseUrl"].as_str().map(String::from),
                    binaries: Vec::new(),
                    ..Default::default()
                })
            })
            .collect())
    }
}
