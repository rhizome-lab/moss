//! JSR package index fetcher (JavaScript Registry).
//!
//! Fetches package metadata from jsr.io.
//!
//! ## API Strategy
//! - **fetch**: `api.jsr.io/packages/@{scope}/{name}` - Official JSR JSON API
//! - **fetch_versions**: Same API, extracts versions array
//! - **search**: `api.jsr.io/packages?query=` - JSR search
//! - **fetch_all**: Not supported (use search instead)

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// JSR package index fetcher.
pub struct Jsr;

impl Jsr {
    /// JSR API.
    const JSR_API: &'static str = "https://api.jsr.io";
}

impl PackageIndex for Jsr {
    fn ecosystem(&self) -> &'static str {
        "jsr"
    }

    fn display_name(&self) -> &'static str {
        "JSR (JavaScript Registry)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // JSR uses scoped packages like @std/path
        let (scope, pkg_name) = if let Some(rest) = name.strip_prefix('@') {
            let parts: Vec<&str> = rest.splitn(2, '/').collect();
            if parts.len() == 2 {
                (parts[0], parts[1])
            } else {
                return Err(IndexError::Parse("invalid JSR package name".into()));
            }
        } else {
            return Err(IndexError::Parse(
                "JSR packages must be scoped (@scope/name)".into(),
            ));
        };

        let url = format!("{}/scopes/{}/packages/{}", Self::JSR_API, scope, pkg_name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        Ok(PackageMeta {
            name: name.to_string(),
            version: response["latestVersion"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            description: response["description"].as_str().map(String::from),
            homepage: Some(format!("https://jsr.io/{}", name)),
            repository: response["githubRepository"].as_object().map(|r| {
                format!(
                    "https://github.com/{}/{}",
                    r["owner"].as_str().unwrap_or(""),
                    r["name"].as_str().unwrap_or("")
                )
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
        let (scope, pkg_name) = if let Some(rest) = name.strip_prefix('@') {
            let parts: Vec<&str> = rest.splitn(2, '/').collect();
            if parts.len() == 2 {
                (parts[0], parts[1])
            } else {
                return Err(IndexError::Parse("invalid JSR package name".into()));
            }
        } else {
            return Err(IndexError::Parse(
                "JSR packages must be scoped (@scope/name)".into(),
            ));
        };

        let url = format!(
            "{}/scopes/{}/packages/{}/versions",
            Self::JSR_API,
            scope,
            pkg_name
        );
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let versions = response
            .as_array()
            .ok_or_else(|| IndexError::Parse("expected array".into()))?;

        Ok(versions
            .iter()
            .filter_map(|v| {
                Some(VersionMeta {
                    version: v["version"].as_str()?.to_string(),
                    released: v["createdAt"].as_str().map(String::from),
                    yanked: v["yanked"].as_bool().unwrap_or(false),
                })
            })
            .collect())
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!("{}/packages?query={}&limit=50", Self::JSR_API, query);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let items = response["items"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing items".into()))?;

        Ok(items
            .iter()
            .filter_map(|pkg| {
                let scope = pkg["scope"].as_str()?;
                let name = pkg["name"].as_str()?;
                Some(PackageMeta {
                    name: format!("@{}/{}", scope, name),
                    version: pkg["latestVersion"]
                        .as_str()
                        .unwrap_or("unknown")
                        .to_string(),
                    description: pkg["description"].as_str().map(String::from),
                    homepage: Some(format!("https://jsr.io/@{}/{}", scope, name)),
                    repository: pkg["githubRepository"].as_object().map(|r| {
                        format!(
                            "https://github.com/{}/{}",
                            r["owner"].as_str().unwrap_or(""),
                            r["name"].as_str().unwrap_or("")
                        )
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
