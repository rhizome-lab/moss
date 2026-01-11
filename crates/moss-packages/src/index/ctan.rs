//! CTAN (Comprehensive TeX Archive Network) package index fetcher.
//!
//! Fetches package metadata from the CTAN JSON API.
//!
//! ## API Strategy
//! - **fetch**: `ctan.org/json/1.1/pkg/{name}` - Official JSON API
//! - **fetch_versions**: Same API, single version (CTAN doesn't track versions)
//! - **search**: Filters fetch_all results (CTAN search API returns HTML)
//! - **fetch_all**: `ctan.org/json/1.2/packages` (cached 1 hour)
//!
//! See: https://ctan.org/help/json

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use std::collections::HashMap;

/// CTAN package index fetcher.
pub struct Ctan;

impl Ctan {
    /// CTAN JSON API base URL.
    const API_BASE: &'static str = "https://ctan.org/json";

    /// Parse a package from CTAN JSON response.
    fn parse_package(pkg: &serde_json::Value) -> Option<PackageMeta> {
        let id = pkg["id"].as_str()?;
        let name = pkg["name"].as_str().unwrap_or(id);

        let mut extra = HashMap::new();

        // Extract topics
        if let Some(topics) = pkg["topics"].as_array() {
            let topic_list: Vec<serde_json::Value> = topics
                .iter()
                .filter_map(|t| t.as_str().map(|s| serde_json::Value::String(s.to_string())))
                .collect();
            if !topic_list.is_empty() {
                extra.insert("topics".to_string(), serde_json::Value::Array(topic_list));
            }
        }

        // Extract TeX Live and MiKTeX info
        if let Some(texlive) = pkg["texlive"].as_str() {
            extra.insert(
                "texlive".to_string(),
                serde_json::Value::String(texlive.to_string()),
            );
        }
        if let Some(miktex) = pkg["miktex"].as_str() {
            extra.insert(
                "miktex".to_string(),
                serde_json::Value::String(miktex.to_string()),
            );
        }

        // Build archive URL from CTAN path
        let archive_url = pkg["ctan"]["path"].as_str().map(|path| {
            // Remove leading slash if present
            let clean_path = path.strip_prefix('/').unwrap_or(path);
            format!("https://mirrors.ctan.org/{}", clean_path)
        });

        // Or use install path if available (TDS zip)
        let archive_url = archive_url.or_else(|| {
            pkg["install"].as_str().map(|path| {
                let clean_path = path.strip_prefix('/').unwrap_or(path);
                format!("https://mirrors.ctan.org/{}", clean_path)
            })
        });

        // Extract description (first description entry)
        let description = pkg["descriptions"]
            .as_array()
            .and_then(|descs| descs.first())
            .and_then(|d| d["text"].as_str())
            .map(|text| {
                // Strip HTML tags for plain text
                text.replace("<p>", "")
                    .replace("</p>", " ")
                    .replace("<ref refid=", "")
                    .replace("</ref>", "")
                    .replace(">", "")
                    .replace("\"", "")
                    .trim()
                    .to_string()
            })
            .or_else(|| pkg["caption"].as_str().map(String::from));

        // Extract version
        let version = pkg["version"]["number"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();

        // Extract license
        let license = pkg["license"]
            .as_array()
            .and_then(|l| l.first())
            .and_then(|l| l.as_str())
            .map(String::from);

        // Extract authors as maintainers
        let maintainers: Vec<String> = pkg["authors"]
            .as_array()
            .map(|authors| {
                authors
                    .iter()
                    .filter_map(|a| a["id"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        Some(PackageMeta {
            name: name.to_string(),
            version,
            description,
            homepage: Some(format!("https://ctan.org/pkg/{}", id)),
            repository: None,
            license,
            binaries: Vec::new(),
            keywords: Vec::new(),
            maintainers,
            published: None,
            downloads: None,
            archive_url,
            checksum: None,
            extra,
        })
    }
}

impl PackageIndex for Ctan {
    fn ecosystem(&self) -> &'static str {
        "ctan"
    }

    fn display_name(&self) -> &'static str {
        "CTAN"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let url = format!("{}/1.1/pkg/{}", Self::API_BASE, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        // Check for errors
        if response["errors"].is_array() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        Self::parse_package(&response).ok_or_else(|| IndexError::NotFound(name.to_string()))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // CTAN only provides the current version
        let pkg = self.fetch(name)?;
        Ok(vec![VersionMeta {
            version: pkg.version,
            released: None,
            yanked: false,
        }])
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        // CTAN search API is not easily accessible via JSON, so we filter fetch_all
        let all = self.fetch_all()?;
        let query_lower = query.to_lowercase();

        let results: Vec<PackageMeta> = all
            .into_iter()
            .filter(|pkg| {
                pkg.name.to_lowercase().contains(&query_lower)
                    || pkg
                        .description
                        .as_ref()
                        .map(|d| d.to_lowercase().contains(&query_lower))
                        .unwrap_or(false)
            })
            .take(50)
            .collect();

        Ok(results)
    }

    fn fetch_all(&self) -> Result<Vec<PackageMeta>, IndexError> {
        // Get list of all packages
        let url = format!("{}/1.2/packages", Self::API_BASE);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let packages: Vec<PackageMeta> = response
            .as_array()
            .ok_or_else(|| IndexError::Parse("expected array".into()))?
            .iter()
            .filter_map(|pkg| {
                let key = pkg["key"].as_str()?;
                let name = pkg["name"].as_str().unwrap_or(key);
                let caption = pkg["caption"].as_str();

                Some(PackageMeta {
                    name: name.to_string(),
                    version: "unknown".to_string(), // List doesn't include versions
                    description: caption.map(String::from),
                    homepage: Some(format!("https://ctan.org/pkg/{}", key)),
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
            .collect();

        Ok(packages)
    }
}
