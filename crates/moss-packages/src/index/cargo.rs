//! Crates.io package index fetcher (Rust).
//!
//! Fetches package metadata from crates.io API.
//!
//! ## API Strategy
//! - **fetch**: `crates.io/api/v1/crates/{name}` - Official JSON API
//! - **fetch_versions**: Same API, extracts versions array
//! - **search**: `crates.io/api/v1/crates?q=` - Official search endpoint
//! - **fetch_all**: Not supported (too large, use search instead)

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use std::collections::HashMap;

/// Crates.io package index fetcher.
pub struct CargoIndex;

impl CargoIndex {
    /// Crates.io API.
    const CRATES_API: &'static str = "https://crates.io/api/v1";
}

impl PackageIndex for CargoIndex {
    fn ecosystem(&self) -> &'static str {
        "cargo"
    }

    fn display_name(&self) -> &'static str {
        "Crates.io (Rust)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let url = format!("{}/crates/{}", Self::CRATES_API, name);
        let response: serde_json::Value = ureq::get(&url)
            .set("User-Agent", "moss-packages/0.1")
            .call()?
            .into_json()?;

        let crate_data = &response["crate"];
        let latest_version = response["versions"].as_array().and_then(|v| v.first());

        let mut extra = HashMap::new();

        // Categories
        if let Some(categories) = crate_data["categories"].as_array() {
            let cats: Vec<serde_json::Value> = categories
                .iter()
                .filter_map(|c| c.as_str().map(|s| serde_json::Value::String(s.to_string())))
                .collect();
            if !cats.is_empty() {
                extra.insert("categories".to_string(), serde_json::Value::Array(cats));
            }
        }

        // Documentation URL
        if let Some(docs) = crate_data["documentation"].as_str() {
            extra.insert(
                "documentation".to_string(),
                serde_json::Value::String(docs.to_string()),
            );
        }

        // Recent downloads (last 90 days)
        if let Some(recent) = crate_data["recent_downloads"].as_u64() {
            extra.insert(
                "recent_downloads".to_string(),
                serde_json::Value::Number(recent.into()),
            );
        }

        // Rust version requirement
        if let Some(msrv) = latest_version.and_then(|v| v["rust_version"].as_str()) {
            extra.insert(
                "rust_version".to_string(),
                serde_json::Value::String(msrv.to_string()),
            );
        }

        // Features
        if let Some(features) = latest_version.and_then(|v| v["features"].as_object()) {
            let features_map: serde_json::Map<String, serde_json::Value> = features
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            if !features_map.is_empty() {
                extra.insert(
                    "features".to_string(),
                    serde_json::Value::Object(features_map),
                );
            }
        }

        Ok(PackageMeta {
            name: crate_data["id"].as_str().unwrap_or(name).to_string(),
            version: crate_data["max_stable_version"]
                .as_str()
                .or_else(|| crate_data["max_version"].as_str())
                .unwrap_or("unknown")
                .to_string(),
            description: crate_data["description"].as_str().map(String::from),
            homepage: crate_data["homepage"].as_str().map(String::from),
            repository: crate_data["repository"].as_str().map(String::from),
            license: latest_version
                .and_then(|v| v["license"].as_str())
                .map(String::from),
            binaries: latest_version
                .and_then(|v| v["bin_names"].as_array())
                .map(|bins| {
                    bins.iter()
                        .filter_map(|b| b.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            keywords: crate_data["keywords"]
                .as_array()
                .map(|kw| {
                    kw.iter()
                        .filter_map(|k| k.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            maintainers: Vec::new(), // Not directly exposed in API
            published: latest_version
                .and_then(|v| v["created_at"].as_str())
                .map(String::from),
            downloads: crate_data["downloads"].as_u64(),
            archive_url: latest_version.and_then(|v| {
                v["dl_path"]
                    .as_str()
                    .map(|p| format!("https://crates.io{}", p))
            }),
            checksum: latest_version
                .and_then(|v| v["checksum"].as_str())
                .map(|h| format!("sha256:{}", h)),
            extra,
        })
    }

    fn fetch_all_versions(&self, name: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!("{}/crates/{}", Self::CRATES_API, name);
        let response: serde_json::Value = ureq::get(&url)
            .set("User-Agent", "moss-packages/0.1")
            .call()?
            .into_json()?;

        let crate_data = &response["crate"];
        let versions = response["versions"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing versions".into()))?;

        let crate_name = crate_data["id"].as_str().unwrap_or(name);

        Ok(versions
            .iter()
            .filter_map(|v| {
                let version = v["num"].as_str()?;
                let mut extra = HashMap::new();

                // Categories (crate-level, same for all versions)
                if let Some(categories) = crate_data["categories"].as_array() {
                    let cats: Vec<serde_json::Value> = categories
                        .iter()
                        .filter_map(|c| {
                            c.as_str().map(|s| serde_json::Value::String(s.to_string()))
                        })
                        .collect();
                    if !cats.is_empty() {
                        extra.insert("categories".to_string(), serde_json::Value::Array(cats));
                    }
                }

                // Rust version requirement (per-version)
                if let Some(msrv) = v["rust_version"].as_str() {
                    extra.insert(
                        "rust_version".to_string(),
                        serde_json::Value::String(msrv.to_string()),
                    );
                }

                // Features (per-version)
                if let Some(features) = v["features"].as_object() {
                    let features_map: serde_json::Map<String, serde_json::Value> = features
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    if !features_map.is_empty() {
                        extra.insert(
                            "features".to_string(),
                            serde_json::Value::Object(features_map),
                        );
                    }
                }

                // Yanked status
                if v["yanked"].as_bool().unwrap_or(false) {
                    extra.insert("yanked".to_string(), serde_json::Value::Bool(true));
                }

                Some(PackageMeta {
                    name: crate_name.to_string(),
                    version: version.to_string(),
                    description: crate_data["description"].as_str().map(String::from),
                    homepage: crate_data["homepage"].as_str().map(String::from),
                    repository: crate_data["repository"].as_str().map(String::from),
                    license: v["license"].as_str().map(String::from),
                    binaries: v["bin_names"]
                        .as_array()
                        .map(|bins| {
                            bins.iter()
                                .filter_map(|b| b.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default(),
                    keywords: crate_data["keywords"]
                        .as_array()
                        .map(|kw| {
                            kw.iter()
                                .filter_map(|k| k.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default(),
                    maintainers: Vec::new(),
                    published: v["created_at"].as_str().map(String::from),
                    downloads: v["downloads"].as_u64(),
                    archive_url: v["dl_path"]
                        .as_str()
                        .map(|p| format!("https://crates.io{}", p)),
                    checksum: v["checksum"].as_str().map(|h| format!("sha256:{}", h)),
                    extra,
                })
            })
            .collect())
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let url = format!("{}/crates/{}/versions", Self::CRATES_API, name);
        let response: serde_json::Value = ureq::get(&url)
            .set("User-Agent", "moss-packages/0.1")
            .call()?
            .into_json()?;

        let versions = response["versions"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing versions".into()))?;

        Ok(versions
            .iter()
            .filter_map(|v| {
                Some(VersionMeta {
                    version: v["num"].as_str()?.to_string(),
                    released: v["created_at"].as_str().map(String::from),
                    yanked: v["yanked"].as_bool().unwrap_or(false),
                })
            })
            .collect())
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!("{}/crates?q={}&per_page=50", Self::CRATES_API, query);
        let response: serde_json::Value = ureq::get(&url)
            .set("User-Agent", "moss-packages/0.1")
            .call()?
            .into_json()?;

        let crates = response["crates"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing crates".into()))?;

        Ok(crates
            .iter()
            .filter_map(|c| {
                Some(PackageMeta {
                    name: c["id"].as_str()?.to_string(),
                    version: c["max_stable_version"]
                        .as_str()
                        .or_else(|| c["max_version"].as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    description: c["description"].as_str().map(String::from),
                    homepage: c["homepage"].as_str().map(String::from),
                    repository: c["repository"].as_str().map(String::from),
                    license: None,        // Not in search results
                    binaries: Vec::new(), // Not in search results
                    keywords: c["keywords"]
                        .as_array()
                        .map(|kw| {
                            kw.iter()
                                .filter_map(|k| k.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default(),
                    maintainers: Vec::new(),
                    published: c["created_at"].as_str().map(String::from),
                    downloads: c["downloads"].as_u64(),
                    archive_url: None, // Not in search results
                    checksum: None,    // Not in search results
                    extra: Default::default(),
                })
            })
            .collect())
    }
}
