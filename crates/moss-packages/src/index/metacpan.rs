//! MetaCPAN package index fetcher (Perl).
//!
//! Fetches package metadata from metacpan.org API.
//!
//! ## API Strategy
//! - **fetch**: `fastapi.metacpan.org/v1/release/{distribution}` - MetaCPAN JSON API
//! - **fetch_versions**: `fastapi.metacpan.org/v1/release/_search` with distribution filter
//! - **search**: `fastapi.metacpan.org/v1/release/_search?q=`
//! - **fetch_all**: Not supported (too large)

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// MetaCPAN package index fetcher.
pub struct MetaCpan;

impl MetaCpan {
    /// MetaCPAN API base.
    const API_BASE: &'static str = "https://fastapi.metacpan.org/v1";
}

impl PackageIndex for MetaCpan {
    fn ecosystem(&self) -> &'static str {
        "cpan"
    }

    fn display_name(&self) -> &'static str {
        "MetaCPAN (Perl)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // MetaCPAN uses distribution names (with hyphens) or module names (with ::)
        let dist_name = name.replace("::", "-");
        let url = format!("{}/release/{}", Self::API_BASE, dist_name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        Ok(PackageMeta {
            name: response["distribution"]
                .as_str()
                .unwrap_or(name)
                .to_string(),
            version: response["version"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            description: response["abstract"].as_str().map(String::from),
            homepage: response["resources"]["homepage"].as_str().map(String::from),
            repository: response["resources"]["repository"]["url"]
                .as_str()
                .or_else(|| response["resources"]["repository"]["web"].as_str())
                .map(String::from),
            license: response["license"]
                .as_array()
                .and_then(|l| l.first())
                .and_then(|l| l.as_str())
                .map(String::from),
            binaries: Vec::new(),
            archive_url: response["download_url"].as_str().map(String::from),
            keywords: Vec::new(),
            maintainers: response["author"]
                .as_str()
                .map(|a| vec![a.to_string()])
                .unwrap_or_default(),
            published: None,
            downloads: None,
            checksum: None,
            extra: Default::default(),
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let dist_name = name.replace("::", "-");
        let url = format!(
            "{}/release/_search?q=distribution:{}&size=100&sort=date:desc",
            Self::API_BASE,
            dist_name
        );
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let hits = response["hits"]["hits"]
            .as_array()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        if hits.is_empty() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        Ok(hits
            .iter()
            .filter_map(|hit| {
                let source = &hit["_source"];
                Some(VersionMeta {
                    version: source["version"].as_str()?.to_string(),
                    released: source["date"].as_str().map(String::from),
                    yanked: source["status"].as_str() == Some("backpan"),
                })
            })
            .collect())
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!(
            "{}/release/_search?q={}&size=50",
            Self::API_BASE,
            urlencoding::encode(query)
        );
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let hits = response["hits"]["hits"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing hits".into()))?;

        Ok(hits
            .iter()
            .filter_map(|hit| {
                let source = &hit["_source"];
                Some(PackageMeta {
                    name: source["distribution"].as_str()?.to_string(),
                    version: source["version"].as_str().unwrap_or("unknown").to_string(),
                    description: source["abstract"].as_str().map(String::from),
                    homepage: None,
                    repository: source["resources"]["repository"]["url"]
                        .as_str()
                        .map(String::from),
                    license: source["license"]
                        .as_array()
                        .and_then(|l| l.first())
                        .and_then(|l| l.as_str())
                        .map(String::from),
                    binaries: Vec::new(),
                    keywords: Vec::new(),
                    maintainers: source["author"]
                        .as_str()
                        .map(|a| vec![a.to_string()])
                        .unwrap_or_default(),
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
