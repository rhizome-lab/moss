//! MSYS2 package index fetcher (Windows development).
//!
//! Fetches package metadata from the MSYS2 packages API.

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// MSYS2 package index fetcher.
pub struct Msys2;

impl Msys2 {
    /// MSYS2 packages API base URL.
    const API_BASE: &'static str = "https://packages.msys2.org/api";
}

impl PackageIndex for Msys2 {
    fn ecosystem(&self) -> &'static str {
        "msys2"
    }

    fn display_name(&self) -> &'static str {
        "MSYS2 (Windows)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let url = format!("{}/search?query={}", Self::API_BASE, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        // Check for exact match first
        let pkg = if let Some(exact) = response["results"]["exact"].as_object() {
            exact.clone()
        } else if let Some(others) = response["results"]["other"].as_array() {
            // Find first match in other results
            others
                .iter()
                .find(|p| p["name"].as_str() == Some(name) || p["realname"].as_str() == Some(name))
                .cloned()
                .ok_or_else(|| IndexError::NotFound(name.to_string()))?
                .as_object()
                .cloned()
                .ok_or_else(|| IndexError::NotFound(name.to_string()))?
        } else {
            return Err(IndexError::NotFound(name.to_string()));
        };

        // Extract license from nested array
        let license = pkg
            .get("licenses")
            .and_then(|l| l.as_array())
            .and_then(|arr| arr.first())
            .and_then(|inner| inner.as_array())
            .and_then(|arr| arr.first())
            .and_then(|l| l.as_str())
            .map(String::from);

        // Collect keywords from groups
        let keywords: Vec<String> = pkg
            .get("groups")
            .and_then(|g| g.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        Ok(PackageMeta {
            name: pkg
                .get("realname")
                .or(pkg.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or(name)
                .to_string(),
            version: pkg
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            description: pkg
                .get("descriptions")
                .and_then(|d| d.as_str())
                .map(String::from),
            homepage: pkg.get("url").and_then(|u| u.as_str()).map(String::from),
            repository: pkg
                .get("source_url")
                .and_then(|u| u.as_str())
                .map(String::from),
            license,
            binaries: Vec::new(),
            keywords,
            ..Default::default()
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // MSYS2 API only provides current version
        let meta = self.fetch(name)?;
        Ok(vec![VersionMeta {
            version: meta.version,
            released: None,
            yanked: false,
        }])
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!("{}/search?query={}", Self::API_BASE, query);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let mut packages = Vec::new();

        // Add exact match if present
        if let Some(exact) = response["results"]["exact"].as_object() {
            if let Some(pkg) = parse_msys2_package(exact) {
                packages.push(pkg);
            }
        }

        // Add other matches
        if let Some(others) = response["results"]["other"].as_array() {
            for other in others {
                if let Some(obj) = other.as_object() {
                    if let Some(pkg) = parse_msys2_package(obj) {
                        packages.push(pkg);
                    }
                }
            }
        }

        Ok(packages)
    }
}

fn parse_msys2_package(pkg: &serde_json::Map<String, serde_json::Value>) -> Option<PackageMeta> {
    let license = pkg
        .get("licenses")
        .and_then(|l| l.as_array())
        .and_then(|arr| arr.first())
        .and_then(|inner| inner.as_array())
        .and_then(|arr| arr.first())
        .and_then(|l| l.as_str())
        .map(String::from);

    let keywords: Vec<String> = pkg
        .get("groups")
        .and_then(|g| g.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    Some(PackageMeta {
        name: pkg
            .get("realname")
            .or(pkg.get("name"))
            .and_then(|n| n.as_str())?
            .to_string(),
        version: pkg
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        description: pkg
            .get("descriptions")
            .and_then(|d| d.as_str())
            .map(String::from),
        homepage: pkg.get("url").and_then(|u| u.as_str()).map(String::from),
        repository: pkg
            .get("source_url")
            .and_then(|u| u.as_str())
            .map(String::from),
        license,
        keywords,
        ..Default::default()
    })
}
