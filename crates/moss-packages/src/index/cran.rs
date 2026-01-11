//! CRAN package index fetcher (R).
//!
//! Fetches package metadata from CRAN (Comprehensive R Archive Network).
//! Uses the crandb API for JSON access.
//!
//! ## API Strategy
//! - **fetch**: `crandb.r-pkg.org/{name}` - crandb JSON API
//! - **fetch_versions**: `crandb.r-pkg.org/{name}/all` - all versions
//! - **search**: `crandb.r-pkg.org/-/search?q=` - crandb search endpoint
//! - **fetch_all**: Not supported (no bulk endpoint)

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// CRAN package index fetcher.
pub struct Cran;

impl Cran {
    /// crandb API base (provides JSON API for CRAN).
    const API_BASE: &'static str = "https://crandb.r-pkg.org";

    /// CRAN mirror for downloads.
    const CRAN_MIRROR: &'static str = "https://cran.r-project.org";
}

impl PackageIndex for Cran {
    fn ecosystem(&self) -> &'static str {
        "cran"
    }

    fn display_name(&self) -> &'static str {
        "CRAN (R)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let url = format!("{}/{}", Self::API_BASE, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        // Check for error response
        if response["error"].is_string() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        let version = response["Version"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();

        Ok(PackageMeta {
            name: response["Package"].as_str().unwrap_or(name).to_string(),
            version: version.clone(),
            description: response["Title"]
                .as_str()
                .or_else(|| response["Description"].as_str())
                .map(String::from),
            homepage: response["URL"]
                .as_str()
                .and_then(|urls| urls.split(',').next())
                .map(|s| s.trim().to_string()),
            repository: response["BugReports"]
                .as_str()
                .filter(|s| s.contains("github.com") || s.contains("gitlab.com"))
                .map(String::from),
            license: response["License"].as_str().map(String::from),
            binaries: Vec::new(),
            maintainers: response["Maintainer"]
                .as_str()
                .map(|m| vec![m.to_string()])
                .unwrap_or_default(),
            keywords: Vec::new(),
            published: None,
            downloads: None,
            archive_url: Some(format!(
                "{}/src/contrib/{}_{}.tar.gz",
                Self::CRAN_MIRROR,
                name,
                version
            )),
            checksum: None,
            extra: Default::default(),
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // crandb /all endpoint returns all versions
        let url = format!("{}/{}/all", Self::API_BASE, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        // Check for error response
        if response["error"].is_string() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        let versions = response["versions"]
            .as_object()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        let mut result: Vec<VersionMeta> = versions
            .iter()
            .map(|(version, data)| VersionMeta {
                version: version.clone(),
                released: data["crandb_file_date"].as_str().map(String::from),
                yanked: false, // CRAN doesn't have yanked concept
            })
            .collect();

        // Sort by version descending
        result.sort_by(|a, b| version_compare(&b.version, &a.version));

        Ok(result)
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        // crandb search endpoint
        let url = format!(
            "{}/-/search?q={}&size=50",
            Self::API_BASE,
            urlencoding::encode(query)
        );
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let packages = response
            .as_array()
            .ok_or_else(|| IndexError::Parse("expected array".into()))?;

        Ok(packages
            .iter()
            .filter_map(|pkg| {
                Some(PackageMeta {
                    name: pkg["Package"].as_str()?.to_string(),
                    version: pkg["Version"].as_str().unwrap_or("unknown").to_string(),
                    description: pkg["Title"].as_str().map(String::from),
                    homepage: None,
                    repository: None,
                    license: pkg["License"].as_str().map(String::from),
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

/// Simple version comparison (handles R-style versions like "1.2-3").
fn version_compare(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| -> Vec<u32> {
        s.split(|c: char| !c.is_ascii_digit())
            .filter_map(|p| p.parse().ok())
            .collect()
    };
    parse(a).cmp(&parse(b))
}
