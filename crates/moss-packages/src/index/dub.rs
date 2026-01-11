//! DUB package index fetcher (D language).
//!
//! Fetches package metadata from the DUB package registry at code.dlang.org.
//!
//! ## API Strategy
//! - **fetch**: `code.dlang.org/api/packages/{name}` - Official DUB JSON API
//! - **fetch_versions**: Same API, extracts versions array
//! - **search**: `code.dlang.org/api/packages/search?q=`
//! - **fetch_all**: `code.dlang.org/api/packages` (all packages)

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// DUB package index fetcher.
pub struct Dub;

impl Dub {
    /// DUB API base URL.
    const API_BASE: &'static str = "https://code.dlang.org/api/packages";
}

impl PackageIndex for Dub {
    fn ecosystem(&self) -> &'static str {
        "dub"
    }

    fn display_name(&self) -> &'static str {
        "DUB (D packages)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let url = format!("{}/{}/info", Self::API_BASE, name);
        let response: serde_json::Value = ureq::get(&url)
            .call()
            .map_err(|_| IndexError::NotFound(name.to_string()))?
            .into_json()?;

        // Get the latest version (first in array)
        let latest = response["versions"]
            .as_array()
            .and_then(|v| v.first())
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        // Extract authors
        let authors: Vec<String> = latest["authors"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| a.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        Ok(PackageMeta {
            name: latest["name"].as_str().unwrap_or(name).to_string(),
            version: latest["version"].as_str().unwrap_or("unknown").to_string(),
            description: latest["description"].as_str().map(String::from),
            homepage: None, // Not in API response
            repository: extract_repository(latest),
            license: latest["license"].as_str().map(String::from),
            binaries: Vec::new(),
            keywords: Vec::new(),
            maintainers: authors,
            published: latest["date"].as_str().map(String::from),
            downloads: None,
            archive_url: None,
            checksum: None,
            extra: Default::default(),
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let url = format!("{}/{}/info", Self::API_BASE, name);
        let response: serde_json::Value = ureq::get(&url)
            .call()
            .map_err(|_| IndexError::NotFound(name.to_string()))?
            .into_json()?;

        let versions = response["versions"]
            .as_array()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        Ok(versions
            .iter()
            .filter_map(|v| {
                let version = v["version"].as_str()?.to_string();
                // Skip branch versions like ~master
                if version.starts_with('~') {
                    return None;
                }
                Some(VersionMeta {
                    version,
                    released: v["date"].as_str().map(String::from),
                    yanked: false,
                })
            })
            .collect())
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!("{}/search?q={}", Self::API_BASE, query);
        let response: Vec<serde_json::Value> = ureq::get(&url).call()?.into_json()?;

        Ok(response
            .into_iter()
            .filter_map(|pkg| {
                Some(PackageMeta {
                    name: pkg["name"].as_str()?.to_string(),
                    version: pkg["version"].as_str().unwrap_or("unknown").to_string(),
                    description: pkg["description"].as_str().map(String::from),
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
            })
            .collect())
    }
}

fn extract_repository(pkg: &serde_json::Value) -> Option<String> {
    // Try to extract from homepage or source URL in the readme
    // DUB packages typically link to GitHub in their repo
    let readme = pkg["readme"].as_str()?;

    // Look for GitHub URL in readme
    if let Some(start) = readme.find("github.com/") {
        let url_start = readme[..start]
            .rfind("https://")
            .or_else(|| readme[..start].rfind("http://"))?;
        let url_end = readme[start..]
            .find(|c: char| c.is_whitespace() || c == ')' || c == ']')
            .map(|e| start + e)
            .unwrap_or(readme.len());
        let url = &readme[url_start..url_end];
        // Clean up the URL (remove trailing punctuation)
        let url = url.trim_end_matches(|c: char| !c.is_alphanumeric() && c != '/');
        return Some(url.to_string());
    }

    None
}
