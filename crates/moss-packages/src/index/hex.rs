//! Hex package index fetcher (Elixir/Erlang).
//!
//! Fetches package metadata from hex.pm.
//!
//! ## API Strategy
//! - **fetch**: `hex.pm/api/packages/{name}` - Official Hex JSON API
//! - **fetch_versions**: Same API, extracts releases array
//! - **search**: `hex.pm/api/packages?search=` - Hex search
//! - **fetch_all**: `hex.pm/api/packages` with pagination

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// Hex package index fetcher.
pub struct Hex;

impl Hex {
    /// Hex.pm API.
    const HEX_API: &'static str = "https://hex.pm/api";
}

impl PackageIndex for Hex {
    fn ecosystem(&self) -> &'static str {
        "hex"
    }

    fn display_name(&self) -> &'static str {
        "Hex (Elixir/Erlang)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let url = format!("{}/packages/{}", Self::HEX_API, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let meta = &response["meta"];
        let latest_release = response["releases"].as_array().and_then(|r| r.first());

        Ok(PackageMeta {
            name: response["name"].as_str().unwrap_or(name).to_string(),
            version: latest_release
                .and_then(|r| r["version"].as_str())
                .unwrap_or("unknown")
                .to_string(),
            description: meta["description"].as_str().map(String::from),
            homepage: response["html_url"].as_str().map(String::from),
            repository: meta["links"]["GitHub"]
                .as_str()
                .or_else(|| meta["links"]["Repository"].as_str())
                .or_else(|| meta["links"]["Source"].as_str())
                .map(String::from),
            license: meta["licenses"]
                .as_array()
                .and_then(|l| l.first())
                .and_then(|l| l.as_str())
                .map(String::from),
            binaries: Vec::new(),
            keywords: Vec::new(), // Hex doesn't have keywords
            maintainers: meta["maintainers"]
                .as_array()
                .map(|m| {
                    m.iter()
                        .filter_map(|maint| maint["username"].as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            published: latest_release
                .and_then(|r| r["inserted_at"].as_str())
                .map(String::from),
            downloads: response["downloads"]["all"].as_u64(),
            archive_url: latest_release
                .and_then(|r| r["url"].as_str())
                .map(String::from),
            checksum: latest_release
                .and_then(|r| r["checksum"].as_str())
                .map(|h| format!("sha256:{}", h)),
            extra: Default::default(),
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let url = format!("{}/packages/{}", Self::HEX_API, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let releases = response["releases"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing releases".into()))?;

        Ok(releases
            .iter()
            .filter_map(|r| {
                Some(VersionMeta {
                    version: r["version"].as_str()?.to_string(),
                    released: r["inserted_at"].as_str().map(String::from),
                    yanked: r["retired"].as_bool().unwrap_or(false),
                })
            })
            .collect())
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!("{}/packages?search={}", Self::HEX_API, query);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let packages = response
            .as_array()
            .ok_or_else(|| IndexError::Parse("expected array".into()))?;

        Ok(packages
            .iter()
            .filter_map(|pkg| {
                let meta = &pkg["meta"];
                let latest_release = pkg["releases"].as_array().and_then(|r| r.first());
                Some(PackageMeta {
                    name: pkg["name"].as_str()?.to_string(),
                    version: latest_release
                        .and_then(|r| r["version"].as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    description: meta["description"].as_str().map(String::from),
                    homepage: pkg["html_url"].as_str().map(String::from),
                    repository: meta["links"]["GitHub"]
                        .as_str()
                        .or_else(|| meta["links"]["Repository"].as_str())
                        .map(String::from),
                    license: meta["licenses"]
                        .as_array()
                        .and_then(|l| l.first())
                        .and_then(|l| l.as_str())
                        .map(String::from),
                    binaries: Vec::new(),
                    keywords: Vec::new(),
                    maintainers: meta["maintainers"]
                        .as_array()
                        .map(|m| {
                            m.iter()
                                .filter_map(|maint| maint["username"].as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default(),
                    published: latest_release
                        .and_then(|r| r["inserted_at"].as_str())
                        .map(String::from),
                    downloads: pkg["downloads"]["all"].as_u64(),
                    archive_url: latest_release
                        .and_then(|r| r["url"].as_str())
                        .map(String::from),
                    checksum: latest_release
                        .and_then(|r| r["checksum"].as_str())
                        .map(|h| format!("sha256:{}", h)),
                    extra: Default::default(),
                })
            })
            .collect())
    }
}
