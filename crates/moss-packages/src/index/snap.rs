//! Snap package index fetcher (Ubuntu/Linux).
//!
//! Fetches package metadata from the Snapcraft.io API.

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// Snap package index fetcher.
pub struct Snap;

impl Snap {
    /// Snapcraft API base URL.
    const API_BASE: &'static str = "https://api.snapcraft.io/v2/snaps";
}

impl PackageIndex for Snap {
    fn ecosystem(&self) -> &'static str {
        "snap"
    }

    fn display_name(&self) -> &'static str {
        "Snap (Snapcraft)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let url = format!("{}/info/{}", Self::API_BASE, name);
        let response: serde_json::Value = ureq::get(&url)
            .set("Snap-Device-Series", "16")
            .call()
            .map_err(|_| IndexError::NotFound(name.to_string()))?
            .into_json()?;

        let snap = &response["snap"];

        // Get latest stable version from channel-map
        let version = response["channel-map"]
            .as_array()
            .and_then(|channels| {
                channels
                    .iter()
                    .find(|ch| ch["channel"]["risk"].as_str() == Some("stable"))
            })
            .and_then(|ch| ch["version"].as_str())
            .unwrap_or_else(|| snap["version"].as_str().unwrap_or("unknown"));

        // Extract categories as keywords
        let keywords: Vec<String> = snap["categories"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| c["name"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        // Get publisher info
        let maintainers: Vec<String> = snap["publisher"]["display-name"]
            .as_str()
            .or(snap["publisher"]["username"].as_str())
            .map(|p| vec![p.to_string()])
            .unwrap_or_default();

        Ok(PackageMeta {
            name: snap["name"].as_str().unwrap_or(name).to_string(),
            version: version.to_string(),
            description: snap["summary"]
                .as_str()
                .or(snap["description"].as_str())
                .map(String::from),
            homepage: snap["website"].as_str().map(String::from),
            repository: snap["contact"].as_str().and_then(|c| {
                if c.contains("github.com") || c.contains("gitlab.com") {
                    Some(c.to_string())
                } else {
                    None
                }
            }),
            license: snap["license"].as_str().map(String::from),
            binaries: Vec::new(),
            keywords,
            maintainers,
            ..Default::default()
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let url = format!("{}/info/{}", Self::API_BASE, name);
        let response: serde_json::Value = ureq::get(&url)
            .set("Snap-Device-Series", "16")
            .call()
            .map_err(|_| IndexError::NotFound(name.to_string()))?
            .into_json()?;

        let channels = response["channel-map"]
            .as_array()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        // Collect unique versions from channel map
        let mut seen = std::collections::HashSet::new();
        let versions: Vec<VersionMeta> = channels
            .iter()
            .filter_map(|ch| {
                let version = ch["version"].as_str()?;
                if seen.insert(version.to_string()) {
                    Some(VersionMeta {
                        version: version.to_string(),
                        released: ch["released-at"].as_str().map(String::from),
                        yanked: false,
                    })
                } else {
                    None
                }
            })
            .collect();

        if versions.is_empty() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        Ok(versions)
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!("{}/find?q={}", Self::API_BASE, query);
        let response: serde_json::Value = ureq::get(&url)
            .set("Snap-Device-Series", "16")
            .call()?
            .into_json()?;

        let results = response["results"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("Invalid search response".into()))?;

        Ok(results
            .iter()
            .filter_map(|result| {
                let snap = &result["snap"];
                Some(PackageMeta {
                    name: snap["name"].as_str()?.to_string(),
                    version: result["version"].as_str().unwrap_or("unknown").to_string(),
                    description: snap["summary"].as_str().map(String::from),
                    homepage: snap["website"].as_str().map(String::from),
                    license: snap["license"].as_str().map(String::from),
                    ..Default::default()
                })
            })
            .collect())
    }
}
