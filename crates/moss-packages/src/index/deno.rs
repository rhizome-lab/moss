//! Deno.land package index fetcher.
//!
//! Fetches package metadata from deno.land/x (third-party modules).
//!
//! ## API Strategy
//! - **fetch**: `apiland.deno.dev/v2/modules/{name}` - Official Deno API
//! - **fetch_versions**: Same API, extracts versions array
//! - **search**: `apiland.deno.dev/v2/modules?query=` - Official search
//! - **fetch_all**: Not supported (too large)

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// Deno.land package index fetcher.
pub struct Deno;

impl Deno {
    /// Deno.land API.
    const DENO_API: &'static str = "https://apiland.deno.dev/v2";
}

impl PackageIndex for Deno {
    fn ecosystem(&self) -> &'static str {
        "deno"
    }

    fn display_name(&self) -> &'static str {
        "deno.land/x"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let url = format!("{}/modules/{}", Self::DENO_API, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        Ok(PackageMeta {
            name: response["name"].as_str().unwrap_or(name).to_string(),
            version: response["latest_version"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            description: response["description"].as_str().map(String::from),
            homepage: Some(format!("https://deno.land/x/{}", name)),
            repository: response["repo"].as_str().map(|r| {
                if r.starts_with("http") {
                    r.to_string()
                } else {
                    format!("https://github.com/{}", r)
                }
            }),
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

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let url = format!("{}/modules/{}", Self::DENO_API, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let versions = response["versions"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing versions".into()))?;

        Ok(versions
            .iter()
            .filter_map(|v| {
                Some(VersionMeta {
                    version: v.as_str()?.to_string(),
                    released: None,
                    yanked: false,
                })
            })
            .collect())
    }

    fn supports_fetch_all(&self) -> bool {
        true
    }

    fn fetch_all(&self) -> Result<Vec<PackageMeta>, IndexError> {
        let mut all_packages = Vec::new();
        let mut next_url = Some(format!("{}/modules?limit=100", Self::DENO_API));

        while let Some(url) = next_url {
            let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

            let items = response["items"]
                .as_array()
                .ok_or_else(|| IndexError::Parse("expected items array".into()))?;

            for m in items {
                if let Some(name) = m["name"].as_str() {
                    all_packages.push(PackageMeta {
                        name: name.to_string(),
                        version: m["latest_version"]
                            .as_str()
                            .unwrap_or("unknown")
                            .to_string(),
                        description: m["description"].as_str().map(String::from),
                        homepage: Some(format!("https://deno.land/x/{}", name)),
                        repository: m["repo"].as_str().map(|r| {
                            if r.starts_with("http") {
                                r.to_string()
                            } else {
                                format!("https://github.com/{}", r)
                            }
                        }),
                        license: None,
                        binaries: Vec::new(),
                        keywords: Vec::new(),
                        maintainers: Vec::new(),
                        published: None,
                        downloads: None,
                        archive_url: None,
                        checksum: None,
                        extra: Default::default(),
                    });
                }
            }

            // Get next page URL
            next_url = response["next"]
                .as_str()
                .map(|path| format!("https://apiland.deno.dev{}", path));
        }

        Ok(all_packages)
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!("{}/modules?query={}&limit=50", Self::DENO_API, query);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let items = response["items"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("expected items array".into()))?;

        Ok(items
            .iter()
            .filter_map(|m| {
                let name = m["name"].as_str()?;
                Some(PackageMeta {
                    name: name.to_string(),
                    version: m["latest_version"]
                        .as_str()
                        .unwrap_or("unknown")
                        .to_string(),
                    description: m["description"].as_str().map(String::from),
                    homepage: Some(format!("https://deno.land/x/{}", name)),
                    repository: m["repo"].as_str().map(|r| {
                        if r.starts_with("http") {
                            r.to_string()
                        } else {
                            format!("https://github.com/{}", r)
                        }
                    }),
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
