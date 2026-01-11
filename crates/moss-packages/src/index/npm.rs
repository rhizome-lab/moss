//! npm package index fetcher (JavaScript/TypeScript).
//!
//! Fetches package metadata from the npm registry.
//!
//! ## API Strategy
//! - **fetch**: `registry.npmjs.org/{name}` - Official npm JSON API
//! - **fetch_versions**: Same API, extracts versions object
//! - **search**: `registry.npmjs.org/-/v1/search?text=`
//! - **fetch_all**: Not supported (millions of packages)

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// npm package index fetcher.
pub struct NpmIndex;

impl NpmIndex {
    /// npm registry API.
    const NPM_REGISTRY: &'static str = "https://registry.npmjs.org";
}

impl PackageIndex for NpmIndex {
    fn ecosystem(&self) -> &'static str {
        "npm"
    }

    fn display_name(&self) -> &'static str {
        "npm (JavaScript)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let url = format!("{}/{}", Self::NPM_REGISTRY, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        if response.get("error").is_some() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        let latest_version = response["dist-tags"]["latest"]
            .as_str()
            .unwrap_or("unknown");
        let latest = &response["versions"][latest_version];

        Ok(PackageMeta {
            name: response["name"].as_str().unwrap_or(name).to_string(),
            version: latest_version.to_string(),
            description: response["description"].as_str().map(String::from),
            homepage: response["homepage"].as_str().map(String::from),
            repository: response["repository"]["url"]
                .as_str()
                .or_else(|| response["repository"].as_str())
                .map(|s| {
                    s.trim_start_matches("git+")
                        .trim_end_matches(".git")
                        .to_string()
                }),
            license: response["license"].as_str().map(String::from),
            binaries: latest["bin"]
                .as_object()
                .map(|bins| bins.keys().cloned().collect())
                .unwrap_or_default(),
            keywords: response["keywords"]
                .as_array()
                .map(|kw| {
                    kw.iter()
                        .filter_map(|k| k.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            maintainers: response["maintainers"]
                .as_array()
                .map(|m| {
                    m.iter()
                        .filter_map(|maint| maint["name"].as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            published: response["time"][latest_version].as_str().map(String::from),
            downloads: None, // Requires separate API call
            archive_url: latest["dist"]["tarball"].as_str().map(String::from),
            checksum: latest["dist"]["shasum"]
                .as_str()
                .map(|h| format!("sha1:{}", h)),
            extra: Default::default(),
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let url = format!("{}/{}", Self::NPM_REGISTRY, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        if response.get("error").is_some() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        let versions = response["versions"]
            .as_object()
            .ok_or_else(|| IndexError::Parse("missing versions".into()))?;

        let time = response["time"].as_object();

        Ok(versions
            .keys()
            .map(|v| VersionMeta {
                version: v.clone(),
                released: time
                    .and_then(|t| t.get(v))
                    .and_then(|t| t.as_str())
                    .map(String::from),
                yanked: response["versions"][v]["deprecated"].is_string(),
            })
            .collect())
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!(
            "https://registry.npmjs.org/-/v1/search?text={}&size=50",
            query
        );
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let objects = response["objects"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing objects".into()))?;

        Ok(objects
            .iter()
            .filter_map(|obj| {
                let pkg = &obj["package"];
                Some(PackageMeta {
                    name: pkg["name"].as_str()?.to_string(),
                    version: pkg["version"].as_str().unwrap_or("unknown").to_string(),
                    description: pkg["description"].as_str().map(String::from),
                    homepage: pkg["links"]["homepage"].as_str().map(String::from),
                    repository: pkg["links"]["repository"].as_str().map(String::from),
                    license: None, // Not in search results
                    binaries: Vec::new(),
                    keywords: pkg["keywords"]
                        .as_array()
                        .map(|kw| {
                            kw.iter()
                                .filter_map(|k| k.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default(),
                    maintainers: pkg["maintainers"]
                        .as_array()
                        .map(|m| {
                            m.iter()
                                .filter_map(|maint| maint["username"].as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default(),
                    published: pkg["date"].as_str().map(String::from),
                    downloads: None,
                    archive_url: None, // Not in search results
                    checksum: None,    // Not in search results
                    extra: Default::default(),
                })
            })
            .collect())
    }
}
