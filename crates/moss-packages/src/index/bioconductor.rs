//! Bioconductor package index fetcher.
//!
//! Fetches package metadata from Bioconductor via r-universe.dev API.
//! Bioconductor provides bioinformatics packages for R.
//!
//! ## API Strategy
//! - **fetch**: `bioconductor.r-universe.dev/api/packages/{name}` - r-universe JSON API
//! - **fetch_versions**: Same API, extracts version history
//! - **search**: `bioconductor.r-universe.dev/api/search?q=` - r-universe search
//! - **fetch_all**: `bioconductor.r-universe.dev/api/packages` (cached 1 hour)

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use std::collections::HashMap;

/// Bioconductor package index fetcher.
pub struct Bioconductor;

impl Bioconductor {
    /// Bioconductor r-universe API base.
    const API_BASE: &'static str = "https://bioconductor.r-universe.dev/api";

    /// Parse a package from r-universe JSON format.
    fn parse_package(pkg: &serde_json::Value) -> Option<PackageMeta> {
        let name = pkg["Package"].as_str()?;
        let version = pkg["Version"].as_str().unwrap_or("unknown");

        let mut extra = HashMap::new();

        // Extract dependencies from Imports/Depends
        let mut deps = Vec::new();
        for dep_field in ["Imports", "Depends", "LinkingTo"] {
            if let Some(dep_str) = pkg[dep_field].as_str() {
                for dep in dep_str.split(',') {
                    let dep_name = dep
                        .trim()
                        .split(|c| c == ' ' || c == '(' || c == '\n')
                        .next()
                        .unwrap_or("")
                        .trim();
                    if !dep_name.is_empty() && dep_name != "R" {
                        deps.push(serde_json::Value::String(dep_name.to_string()));
                    }
                }
            }
        }
        if !deps.is_empty() {
            extra.insert("depends".to_string(), serde_json::Value::Array(deps));
        }

        // Extract file info
        if let Some(size) = pkg["_filesize"].as_u64() {
            extra.insert("size".to_string(), serde_json::Value::Number(size.into()));
        }

        // Build archive URL
        let archive_url = pkg["_file"]
            .as_str()
            .map(|file| format!("https://bioconductor.r-universe.dev/src/contrib/{}", file));

        // Get checksum
        let checksum = pkg["_sha256"].as_str().map(|s| format!("sha256:{}", s));

        // Get maintainer
        let maintainers: Vec<String> = pkg["Maintainer"]
            .as_str()
            .map(|m| vec![m.to_string()])
            .unwrap_or_default();

        Some(PackageMeta {
            name: name.to_string(),
            version: version.to_string(),
            description: pkg["Title"].as_str().map(String::from),
            homepage: pkg["URL"].as_str().map(String::from),
            repository: pkg["RemoteUrl"].as_str().map(String::from),
            license: pkg["License"].as_str().map(String::from),
            binaries: Vec::new(),
            archive_url,
            keywords: Vec::new(),
            maintainers,
            published: None,
            downloads: None,
            checksum,
            extra,
        })
    }
}

impl PackageIndex for Bioconductor {
    fn ecosystem(&self) -> &'static str {
        "bioconductor"
    }

    fn display_name(&self) -> &'static str {
        "Bioconductor"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let url = format!("{}/packages/{}", Self::API_BASE, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        Self::parse_package(&response).ok_or_else(|| IndexError::NotFound(name.to_string()))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // r-universe provides version history
        let url = format!("{}/packages/{}/versions", Self::API_BASE, name);

        match ureq::get(&url).call() {
            Ok(resp) => {
                let versions: Vec<serde_json::Value> = resp.into_json()?;
                Ok(versions
                    .iter()
                    .filter_map(|v| {
                        Some(VersionMeta {
                            version: v["Version"].as_str()?.to_string(),
                            released: v["_published"].as_str().map(String::from),
                            yanked: false,
                        })
                    })
                    .collect())
            }
            Err(_) => {
                // Fall back to just current version
                let pkg = self.fetch(name)?;
                Ok(vec![VersionMeta {
                    version: pkg.version,
                    released: None,
                    yanked: false,
                }])
            }
        }
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        // r-universe has a search endpoint
        let url = format!(
            "{}/packages?q={}&limit=50",
            Self::API_BASE,
            urlencoding::encode(query)
        );
        let response: Vec<serde_json::Value> = ureq::get(&url).call()?.into_json()?;

        Ok(response.iter().filter_map(Self::parse_package).collect())
    }

    fn fetch_all(&self) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!("{}/packages", Self::API_BASE);
        let response: Vec<serde_json::Value> = ureq::get(&url).call()?.into_json()?;

        Ok(response.iter().filter_map(Self::parse_package).collect())
    }
}
