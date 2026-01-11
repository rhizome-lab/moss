//! Homebrew package index fetcher (macOS/Linux).
//!
//! Fetches package metadata from Homebrew's formula JSON API.

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use crate::cache;
use std::collections::HashMap;
use std::time::Duration;

/// Default cache TTL for formula list (1 hour).
const INDEX_CACHE_TTL: Duration = Duration::from_secs(60 * 60);

/// Homebrew package index fetcher.
pub struct Brew;

impl Brew {
    /// Homebrew formula API.
    const BREW_API: &'static str = "https://formulae.brew.sh/api";
}

impl PackageIndex for Brew {
    fn ecosystem(&self) -> &'static str {
        "brew"
    }

    fn display_name(&self) -> &'static str {
        "Homebrew (macOS/Linux)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let url = format!("{}/formula/{}.json", Self::BREW_API, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        // Extract 365-day install count from analytics
        let downloads = response["analytics"]["install"]["365d"]
            .as_object()
            .and_then(|obj| obj.values().filter_map(|v| v.as_u64()).next());

        // Collect aliases as keywords
        let mut keywords: Vec<String> = response["aliases"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        // Add oldnames to keywords too
        if let Some(oldnames) = response["oldnames"].as_array() {
            keywords.extend(oldnames.iter().filter_map(|v| v.as_str().map(String::from)));
        }

        // Build extra metadata
        let mut extra = HashMap::new();

        // Dependencies
        if let Some(deps) = response["dependencies"].as_array() {
            if !deps.is_empty() {
                extra.insert(
                    "dependencies".to_string(),
                    serde_json::Value::Array(deps.clone()),
                );
            }
        }
        if let Some(build_deps) = response["build_dependencies"].as_array() {
            if !build_deps.is_empty() {
                extra.insert(
                    "build_dependencies".to_string(),
                    serde_json::Value::Array(build_deps.clone()),
                );
            }
        }

        // Tap info
        if let Some(tap) = response["tap"].as_str() {
            extra.insert("tap".to_string(), serde_json::json!(tap));
        }

        Ok(PackageMeta {
            name: response["name"].as_str().unwrap_or(name).to_string(),
            version: response["versions"]["stable"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            description: response["desc"].as_str().map(String::from),
            homepage: response["homepage"].as_str().map(String::from),
            repository: extract_repository(&response),
            license: response["license"].as_str().map(String::from),
            binaries: response["bin"]
                .as_array()
                .map(|bins| {
                    bins.iter()
                        .filter_map(|b| b.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            keywords,
            maintainers: Vec::new(), // Not exposed in API
            published: response["generated_date"].as_str().map(String::from),
            downloads,
            archive_url: response["urls"]["stable"]["url"].as_str().map(String::from),
            checksum: response["urls"]["stable"]["checksum"]
                .as_str()
                .map(|h| format!("sha256:{}", h)),
            extra,
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let url = format!("{}/formula/{}.json", Self::BREW_API, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let mut versions = Vec::new();

        // Current stable version
        if let Some(stable) = response["versions"]["stable"].as_str() {
            versions.push(VersionMeta {
                version: stable.to_string(),
                released: None,
                yanked: false,
            });
        }

        // HEAD version if available
        if response["versions"]["head"].as_str().is_some() {
            versions.push(VersionMeta {
                version: "HEAD".to_string(),
                released: None,
                yanked: false,
            });
        }

        // Versioned formulae (e.g., python@3.11)
        if let Some(versioned) = response["versioned_formulae"].as_array() {
            for v in versioned {
                if let Some(name) = v.as_str() {
                    // Extract version from name like "python@3.11"
                    if let Some(ver) = name.split('@').nth(1) {
                        versions.push(VersionMeta {
                            version: ver.to_string(),
                            released: None,
                            yanked: false,
                        });
                    }
                }
            }
        }

        if versions.is_empty() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        Ok(versions)
    }

    fn supports_fetch_all(&self) -> bool {
        true
    }

    fn fetch_all(&self) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!("{}/formula.json", Self::BREW_API);

        // Try cache first
        let (data, _was_cached) =
            cache::fetch_with_cache(self.ecosystem(), "formula-all", &url, INDEX_CACHE_TTL)
                .map_err(|e| IndexError::Network(e))?;

        let response: Vec<serde_json::Value> = serde_json::from_slice(&data)?;

        Ok(response
            .into_iter()
            .filter_map(|formula| {
                let downloads = formula["analytics"]["install"]["365d"]
                    .as_object()
                    .and_then(|obj| obj.values().filter_map(|v| v.as_u64()).next());

                let mut keywords: Vec<String> = formula["aliases"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                if let Some(oldnames) = formula["oldnames"].as_array() {
                    keywords.extend(oldnames.iter().filter_map(|v| v.as_str().map(String::from)));
                }

                Some(PackageMeta {
                    name: formula["name"].as_str()?.to_string(),
                    version: formula["versions"]["stable"]
                        .as_str()
                        .unwrap_or("unknown")
                        .to_string(),
                    description: formula["desc"].as_str().map(String::from),
                    homepage: formula["homepage"].as_str().map(String::from),
                    repository: extract_repository(&formula),
                    license: formula["license"].as_str().map(String::from),
                    keywords,
                    downloads,
                    archive_url: formula["urls"]["stable"]["url"].as_str().map(String::from),
                    checksum: formula["urls"]["stable"]["checksum"]
                        .as_str()
                        .map(|h| format!("sha256:{}", h)),
                    ..Default::default()
                })
            })
            .collect())
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        // Homebrew doesn't have a search API, fetch all and filter
        let all = self.fetch_all()?;
        let query_lower = query.to_lowercase();

        Ok(all
            .into_iter()
            .filter(|p| {
                p.name.to_lowercase().contains(&query_lower)
                    || p.description
                        .as_ref()
                        .is_some_and(|d| d.to_lowercase().contains(&query_lower))
            })
            .collect())
    }
}

fn extract_repository(formula: &serde_json::Value) -> Option<String> {
    // Try to get repository from urls.stable.url (often GitHub releases)
    let url = formula["urls"]["stable"]["url"].as_str()?;

    if url.contains("github.com") {
        // Extract owner/repo from GitHub URL
        // e.g., https://github.com/BurntSushi/ripgrep/archive/14.1.0.tar.gz
        let parts: Vec<&str> = url.split('/').collect();
        if let Some(github_idx) = parts.iter().position(|&p| p == "github.com") {
            if parts.len() > github_idx + 2 {
                return Some(format!(
                    "https://github.com/{}/{}",
                    parts[github_idx + 1],
                    parts[github_idx + 2]
                ));
            }
        }
    }

    // Fallback to homepage if it's a GitHub URL
    let homepage = formula["homepage"].as_str()?;
    if homepage.contains("github.com") {
        return Some(homepage.to_string());
    }

    None
}
