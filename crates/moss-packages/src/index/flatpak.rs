//! Flathub package index fetcher (Flatpak apps).
//!
//! Fetches package metadata from Flathub API.

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// Flathub package index fetcher.
pub struct Flatpak;

impl Flatpak {
    /// Flathub API base.
    const API_BASE: &'static str = "https://flathub.org/api/v2";
}

impl PackageIndex for Flatpak {
    fn ecosystem(&self) -> &'static str {
        "flatpak"
    }

    fn display_name(&self) -> &'static str {
        "Flathub (Flatpak)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let url = format!("{}/appstream/{}", Self::API_BASE, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        Ok(app_to_meta(&response, name))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let url = format!("{}/appstream/{}", Self::API_BASE, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        // Flathub typically has only the current version
        let version = response["releases"]
            .as_array()
            .and_then(|r| r.first())
            .and_then(|r| r["version"].as_str())
            .or_else(|| response["bundle"]["runtime"].as_str())
            .unwrap_or("unknown");

        Ok(vec![VersionMeta {
            version: version.to_string(),
            released: response["releases"]
                .as_array()
                .and_then(|r| r.first())
                .and_then(|r| r["timestamp"].as_str())
                .map(String::from),
            yanked: false,
        }])
    }

    fn supports_fetch_all(&self) -> bool {
        true
    }

    fn fetch_all(&self) -> Result<Vec<PackageMeta>, IndexError> {
        // Get list of all app IDs
        let url = format!("{}/appstream", Self::API_BASE);
        let app_ids: Vec<String> = ureq::get(&url).call()?.into_json()?;

        // Note: fetching details for each would be too slow
        // Return basic info from the list
        Ok(app_ids
            .into_iter()
            .map(|id| PackageMeta {
                name: id,
                version: "unknown".to_string(),
                description: None,
                homepage: None,
                repository: None,
                license: None,
                binaries: Vec::new(),
                ..Default::default()
            })
            .collect())
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!("{}/search?q={}", Self::API_BASE, urlencoding::encode(query));
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let hits = response["hits"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing hits".into()))?;

        Ok(hits
            .iter()
            .take(50)
            .filter_map(|hit| {
                Some(PackageMeta {
                    name: hit["id"].as_str()?.to_string(),
                    version: "unknown".to_string(),
                    description: hit["summary"].as_str().map(String::from),
                    homepage: hit["project_url"].as_str().map(String::from),
                    repository: None,
                    license: None,
                    binaries: Vec::new(),
                    ..Default::default()
                })
            })
            .collect())
    }
}

fn app_to_meta(app: &serde_json::Value, fallback_name: &str) -> PackageMeta {
    let version = app["releases"]
        .as_array()
        .and_then(|r| r.first())
        .and_then(|r| r["version"].as_str())
        .unwrap_or("unknown");

    PackageMeta {
        name: app["id"].as_str().unwrap_or(fallback_name).to_string(),
        version: version.to_string(),
        description: app["summary"].as_str().map(String::from),
        homepage: app["project_url"].as_str().map(String::from),
        repository: app["vcs_url"].as_str().map(String::from),
        license: app["project_license"].as_str().map(String::from),
        binaries: Vec::new(),
        ..Default::default()
    }
}
