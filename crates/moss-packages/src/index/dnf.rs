//! DNF package index fetcher (Fedora/RHEL).
//!
//! Fetches package metadata from Fedora repositories.

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// DNF package index fetcher.
pub struct Dnf;

impl Dnf {
    /// Fedora packages API (fcomm_connector).
    const FEDORA_API: &'static str = "https://apps.fedoraproject.org/packages/fcomm_connector";

    /// mdapi for package metadata.
    const MDAPI: &'static str = "https://mdapi.fedoraproject.org";
}

impl PackageIndex for Dnf {
    fn ecosystem(&self) -> &'static str {
        "dnf"
    }

    fn display_name(&self) -> &'static str {
        "DNF (Fedora/RHEL)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Use mdapi for package info - it's simpler
        // Try Fedora rawhide first
        let url = format!("{}/rawhide/pkg/{}", Self::MDAPI, name);
        let response: serde_json::Value = ureq::get(&url)
            .set("Accept", "application/json")
            .call()?
            .into_json()?;

        if response.get("output").is_some() && response["output"].as_str() == Some("notok") {
            return Err(IndexError::NotFound(name.to_string()));
        }

        let mut extra = std::collections::HashMap::new();

        // Extract dependencies from requires
        if let Some(requires) = response["requires"].as_array() {
            let deps: Vec<serde_json::Value> = requires
                .iter()
                .filter_map(|r| r["name"].as_str())
                .filter(|name| {
                    // Filter out internal deps like libc.so, rtld, etc.
                    !name.contains("()") && !name.starts_with("rtld") && !name.contains(".so")
                })
                .map(|name| serde_json::Value::String(name.to_string()))
                .collect();
            if !deps.is_empty() {
                extra.insert("depends".to_string(), serde_json::Value::Array(deps));
            }
        }

        // Extract arch
        if let Some(arch) = response["arch"].as_str() {
            extra.insert(
                "arch".to_string(),
                serde_json::Value::String(arch.to_string()),
            );
        }

        Ok(PackageMeta {
            name: response["name"].as_str().unwrap_or(name).to_string(),
            version: format!(
                "{}-{}",
                response["version"].as_str().unwrap_or("unknown"),
                response["release"].as_str().unwrap_or("1")
            ),
            description: response["summary"].as_str().map(String::from),
            homepage: response["url"].as_str().map(String::from),
            repository: response["url"]
                .as_str()
                .filter(|u| u.contains("github.com") || u.contains("gitlab.com"))
                .map(String::from),
            license: response["license"].as_str().map(String::from),
            binaries: Vec::new(),
            extra,
            ..Default::default()
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // mdapi provides versions across different Fedora releases
        let pkg = self.fetch(name)?;

        // For now just return current version
        // Could query multiple releases (f39, f40, rawhide, etc.)
        Ok(vec![VersionMeta {
            version: pkg.version,
            released: None,
            yanked: false,
        }])
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        // Use the fcomm_connector API with JSON in URL
        let search_json = serde_json::json!({
            "filters": {"search": query},
            "rows_per_page": 50,
            "start_row": 0
        });

        let url = format!(
            "{}/xapian/query/search_packages/{}",
            Self::FEDORA_API,
            urlencoding::encode(&search_json.to_string())
        );

        let response: serde_json::Value = ureq::get(&url)
            .set("Accept", "application/json")
            .call()?
            .into_json()?;

        let rows = response["rows"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing rows".into()))?;

        Ok(rows
            .iter()
            .filter_map(|pkg| {
                Some(PackageMeta {
                    name: pkg["name"].as_str()?.to_string(),
                    version: pkg["version"].as_str().unwrap_or("unknown").to_string(),
                    description: pkg["summary"].as_str().map(String::from),
                    homepage: pkg["upstream_url"].as_str().map(String::from),
                    repository: None,
                    license: pkg["license"].as_str().map(String::from),
                    binaries: Vec::new(),
                    ..Default::default()
                })
            })
            .collect())
    }
}
