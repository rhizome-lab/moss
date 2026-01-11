//! PyPI package index fetcher (Python).
//!
//! Fetches package metadata from the Python Package Index.
//!
//! ## API Strategy
//! - **fetch**: `pypi.org/pypi/{name}/json` - Official PyPI JSON API
//! - **fetch_versions**: Same API, extracts releases object keys
//! - **search**: Not supported (PyPI deprecated search API)
//! - **fetch_all**: Not supported (too large)

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// PyPI package index fetcher.
pub struct PipIndex;

impl PipIndex {
    /// PyPI JSON API.
    const PYPI_API: &'static str = "https://pypi.org/pypi";
}

impl PackageIndex for PipIndex {
    fn ecosystem(&self) -> &'static str {
        "pip"
    }

    fn display_name(&self) -> &'static str {
        "PyPI (Python)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let url = format!("{}/{}/json", Self::PYPI_API, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let info = &response["info"];
        let version = info["version"].as_str().unwrap_or("unknown");
        let latest_release = response["releases"][version]
            .as_array()
            .and_then(|files| files.iter().find(|f| f["packagetype"] == "sdist"))
            .or_else(|| {
                response["releases"][version]
                    .as_array()
                    .and_then(|f| f.first())
            });

        Ok(PackageMeta {
            name: info["name"].as_str().unwrap_or(name).to_string(),
            version: version.to_string(),
            description: info["summary"].as_str().map(String::from),
            homepage: info["home_page"]
                .as_str()
                .or_else(|| info["project_url"].as_str())
                .map(String::from),
            repository: extract_repo_url(info),
            license: info["license"].as_str().map(String::from),
            binaries: Vec::new(), // PyPI doesn't expose this directly
            keywords: info["keywords"]
                .as_str()
                .map(|s| s.split(',').map(|k| k.trim().to_string()).collect())
                .unwrap_or_default(),
            maintainers: {
                let mut m = Vec::new();
                if let Some(author) = info["author"].as_str() {
                    if !author.is_empty() {
                        m.push(author.to_string());
                    }
                }
                if let Some(maintainer) = info["maintainer"].as_str() {
                    if !maintainer.is_empty() && !m.contains(&maintainer.to_string()) {
                        m.push(maintainer.to_string());
                    }
                }
                m
            },
            published: latest_release
                .and_then(|r| r["upload_time"].as_str())
                .map(String::from),
            downloads: None, // Requires separate API (pypistats.org)
            archive_url: latest_release
                .and_then(|r| r["url"].as_str())
                .map(String::from),
            checksum: latest_release
                .and_then(|r| r["digests"]["sha256"].as_str())
                .map(|h| format!("sha256:{}", h)),
            extra: Default::default(),
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let url = format!("{}/{}/json", Self::PYPI_API, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let releases = response["releases"]
            .as_object()
            .ok_or_else(|| IndexError::Parse("missing releases".into()))?;

        Ok(releases
            .iter()
            .filter_map(|(version, files)| {
                let files = files.as_array()?;
                let released = files
                    .first()
                    .and_then(|f| f["upload_time"].as_str())
                    .map(String::from);
                let yanked = files
                    .first()
                    .and_then(|f| f["yanked"].as_bool())
                    .unwrap_or(false);

                Some(VersionMeta {
                    version: version.clone(),
                    released,
                    yanked,
                })
            })
            .collect())
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        // PyPI doesn't have a JSON search API, use the XML-RPC or simple search
        // For now, use the simple search page and parse
        let url = format!("https://pypi.org/search/?q={}", query);

        // This would require HTML parsing, so we'll use a simpler approach
        // Just return an error suggesting to use fetch() directly
        Err(IndexError::Network(format!(
            "PyPI search not implemented via API. Use fetch() with exact package name, or visit: {}",
            url
        )))
    }
}

fn extract_repo_url(info: &serde_json::Value) -> Option<String> {
    // Try project_urls first
    if let Some(urls) = info["project_urls"].as_object() {
        for key in ["Repository", "Source", "Source Code", "GitHub", "Code"] {
            if let Some(url) = urls.get(key).and_then(|u| u.as_str()) {
                return Some(url.to_string());
            }
        }
    }

    // Fall back to home_page if it looks like a repo
    if let Some(home) = info["home_page"].as_str() {
        if home.contains("github.com") || home.contains("gitlab.com") {
            return Some(home.to_string());
        }
    }

    None
}
