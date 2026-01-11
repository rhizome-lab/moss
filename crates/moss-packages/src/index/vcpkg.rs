//! vcpkg package index fetcher (C++ packages).
//!
//! Fetches package metadata from the vcpkg registry on GitHub.
//!
//! ## API Strategy
//! - **fetch**: `github.com/microsoft/vcpkg/.../baseline.json` + port CONTROL/vcpkg.json
//! - **fetch_versions**: baseline.json (single version)
//! - **search**: Filters cached baseline.json
//! - **fetch_all**: Parses baseline.json from GitHub (cached 1 hour)

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use crate::cache;
use std::time::Duration;

/// Cache TTL for baseline data (1 hour).
const BASELINE_CACHE_TTL: Duration = Duration::from_secs(60 * 60);

/// vcpkg package index fetcher.
pub struct Vcpkg;

impl Vcpkg {
    /// vcpkg baseline.json URL (contains all package versions).
    const BASELINE_URL: &'static str =
        "https://raw.githubusercontent.com/microsoft/vcpkg/master/versions/baseline.json";

    /// vcpkg port manifest URL template.
    const PORT_URL_TEMPLATE: &'static str =
        "https://raw.githubusercontent.com/microsoft/vcpkg/master/ports/{}/vcpkg.json";

    /// Fetch and cache the baseline.json.
    fn fetch_baseline() -> Result<serde_json::Value, IndexError> {
        let (data, _was_cached) =
            cache::fetch_with_cache("vcpkg", "baseline", Self::BASELINE_URL, BASELINE_CACHE_TTL)
                .map_err(IndexError::Network)?;

        let baseline: serde_json::Value = serde_json::from_slice(&data)?;
        Ok(baseline)
    }

    /// Fetch the vcpkg.json manifest for a specific port.
    fn fetch_port_manifest(name: &str) -> Result<serde_json::Value, IndexError> {
        let url = Self::PORT_URL_TEMPLATE.replace("{}", name);
        let response: serde_json::Value = ureq::get(&url)
            .call()
            .map_err(|_| IndexError::NotFound(name.to_string()))?
            .into_json()?;
        Ok(response)
    }
}

impl PackageIndex for Vcpkg {
    fn ecosystem(&self) -> &'static str {
        "vcpkg"
    }

    fn display_name(&self) -> &'static str {
        "vcpkg (C++)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // First check if package exists in baseline
        let baseline = Self::fetch_baseline()?;
        let default_baseline = baseline
            .get("default")
            .ok_or_else(|| IndexError::Parse("Missing default baseline".into()))?;

        let pkg_baseline = default_baseline
            .get(name)
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        let version = pkg_baseline["baseline"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();

        let port_version = pkg_baseline["port-version"].as_u64().unwrap_or(0);

        // Fetch detailed metadata from port manifest
        match Self::fetch_port_manifest(name) {
            Ok(manifest) => Ok(PackageMeta {
                name: name.to_string(),
                version: if port_version > 0 {
                    format!("{}#{}", version, port_version)
                } else {
                    version
                },
                description: manifest["description"]
                    .as_str()
                    .or_else(|| {
                        manifest["description"]
                            .as_array()
                            .and_then(|a| a.first()?.as_str())
                    })
                    .map(String::from),
                homepage: manifest["homepage"].as_str().map(String::from),
                repository: manifest["repository"].as_str().map(String::from),
                license: manifest["license"].as_str().map(String::from),
                binaries: Vec::new(),
                keywords: manifest["features"]
                    .as_object()
                    .map(|f| f.keys().cloned().collect())
                    .unwrap_or_default(),
                maintainers: Vec::new(),
                published: None,
                downloads: None,
                archive_url: None,
                checksum: None,
                extra: Default::default(),
            }),
            Err(_) => {
                // Fall back to baseline-only info
                Ok(PackageMeta {
                    name: name.to_string(),
                    version: if port_version > 0 {
                        format!("{}#{}", version, port_version)
                    } else {
                        version
                    },
                    description: None,
                    homepage: None,
                    repository: None,
                    license: None,
                    binaries: Vec::new(),
                    keywords: Vec::new(),
                    maintainers: Vec::new(),
                    published: None,
                    downloads: None,
                    archive_url: None,
                    checksum: None,
                    extra: Default::default(),
                })
            }
        }
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // vcpkg baseline only provides the current version
        let baseline = Self::fetch_baseline()?;
        let default_baseline = baseline
            .get("default")
            .ok_or_else(|| IndexError::Parse("Missing default baseline".into()))?;

        let pkg_baseline = default_baseline
            .get(name)
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        let version = pkg_baseline["baseline"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();

        let port_version = pkg_baseline["port-version"].as_u64().unwrap_or(0);

        let full_version = if port_version > 0 {
            format!("{}#{}", version, port_version)
        } else {
            version
        };

        Ok(vec![VersionMeta {
            version: full_version,
            released: None,
            yanked: false,
        }])
    }

    fn supports_fetch_all(&self) -> bool {
        true
    }

    fn fetch_all(&self) -> Result<Vec<PackageMeta>, IndexError> {
        let baseline = Self::fetch_baseline()?;
        let default_baseline = baseline
            .get("default")
            .ok_or_else(|| IndexError::Parse("Missing default baseline".into()))?;

        let packages = default_baseline
            .as_object()
            .ok_or_else(|| IndexError::Parse("Invalid baseline format".into()))?;

        Ok(packages
            .iter()
            .map(|(name, pkg)| {
                let version = pkg["baseline"].as_str().unwrap_or("unknown").to_string();
                let port_version = pkg["port-version"].as_u64().unwrap_or(0);

                PackageMeta {
                    name: name.clone(),
                    version: if port_version > 0 {
                        format!("{}#{}", version, port_version)
                    } else {
                        version
                    },
                    description: None,
                    homepage: None,
                    repository: None,
                    license: None,
                    binaries: Vec::new(),
                    keywords: Vec::new(),
                    maintainers: Vec::new(),
                    published: None,
                    downloads: None,
                    archive_url: None,
                    checksum: None,
                    extra: Default::default(),
                }
            })
            .collect())
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let all = self.fetch_all()?;
        let query_lower = query.to_lowercase();
        Ok(all
            .into_iter()
            .filter(|p| p.name.to_lowercase().contains(&query_lower))
            .take(50)
            .collect())
    }
}
