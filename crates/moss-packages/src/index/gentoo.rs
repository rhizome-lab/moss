//! Gentoo package index fetcher (Portage).
//!
//! Fetches package metadata from packages.gentoo.org.

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// Gentoo package index fetcher.
pub struct Gentoo;

impl Gentoo {
    /// Gentoo packages API.
    const GENTOO_API: &'static str = "https://packages.gentoo.org";
}

impl PackageIndex for Gentoo {
    fn ecosystem(&self) -> &'static str {
        "gentoo"
    }

    fn display_name(&self) -> &'static str {
        "Gentoo (Portage)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Gentoo uses category/package format (e.g., "sys-apps/ripgrep")
        // If no category provided, search for it
        let package_path = if name.contains('/') {
            name.to_string()
        } else {
            // Search and use first result
            let search_url = format!("{}/packages/search?q={}", Self::GENTOO_API, name);
            let search_response: serde_json::Value = ureq::get(&search_url)
                .set("Accept", "application/json")
                .call()?
                .into_json()?;

            let packages = search_response["packages"]
                .as_array()
                .or_else(|| search_response.as_array())
                .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

            let pkg = packages
                .iter()
                .find(|p| p["name"].as_str() == Some(name))
                .or_else(|| packages.first())
                .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

            format!(
                "{}/{}",
                pkg["category"].as_str().unwrap_or("unknown"),
                pkg["name"].as_str().unwrap_or(name)
            )
        };

        let url = format!("{}/packages/{}.json", Self::GENTOO_API, package_path);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        // Get latest stable version
        let versions = response["versions"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing versions".into()))?;

        let latest = versions
            .iter()
            .filter(|v| {
                v["keywords"]
                    .as_array()
                    .map(|kw| {
                        kw.iter()
                            .any(|k| !k.as_str().unwrap_or("").starts_with('~'))
                    })
                    .unwrap_or(false)
            })
            .last()
            .or_else(|| versions.last())
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        Ok(PackageMeta {
            name: response["name"].as_str().unwrap_or(name).to_string(),
            version: latest["version"].as_str().unwrap_or("unknown").to_string(),
            description: response["description"].as_str().map(String::from),
            homepage: response["homepage"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|u| u.as_str())
                .map(String::from),
            repository: extract_repo(&response["homepage"]),
            license: response["licenses"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|l| l.as_str())
                .map(String::from),
            maintainers: response["maintainers"]
                .as_array()
                .map(|m| {
                    m.iter()
                        .filter_map(|p| p["email"].as_str().or_else(|| p["name"].as_str()))
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default(),
            binaries: Vec::new(),
            ..Default::default()
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let package_path = if name.contains('/') {
            name.to_string()
        } else {
            // Need to find the category first
            let pkg = self.fetch(name)?;
            // Assume we got the right package, try common categories
            return Ok(vec![VersionMeta {
                version: pkg.version,
                released: None,
                yanked: false,
            }]);
        };

        let url = format!("{}/packages/{}.json", Self::GENTOO_API, package_path);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let versions = response["versions"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing versions".into()))?;

        Ok(versions
            .iter()
            .filter_map(|v| {
                Some(VersionMeta {
                    version: v["version"].as_str()?.to_string(),
                    released: None,
                    // Masked versions could be considered "yanked"
                    yanked: v["masks"]
                        .as_array()
                        .map(|m| !m.is_empty())
                        .unwrap_or(false),
                })
            })
            .collect())
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!("{}/packages/search?q={}", Self::GENTOO_API, query);
        let response: serde_json::Value = ureq::get(&url)
            .set("Accept", "application/json")
            .call()?
            .into_json()?;

        let packages = response["packages"]
            .as_array()
            .or_else(|| response.as_array())
            .ok_or_else(|| IndexError::Parse("expected packages array".into()))?;

        Ok(packages
            .iter()
            .filter_map(|pkg| {
                Some(PackageMeta {
                    name: format!(
                        "{}/{}",
                        pkg["category"].as_str().unwrap_or("unknown"),
                        pkg["name"].as_str()?
                    ),
                    version: pkg["version"].as_str().unwrap_or("unknown").to_string(),
                    description: pkg["description"].as_str().map(String::from),
                    homepage: None,
                    repository: None,
                    license: None,
                    maintainers: Vec::new(),
                    binaries: Vec::new(),
                    ..Default::default()
                })
            })
            .collect())
    }
}

fn extract_repo(homepage: &serde_json::Value) -> Option<String> {
    homepage.as_array().and_then(|urls| {
        urls.iter()
            .filter_map(|u| u.as_str())
            .find(|u| u.contains("github.com") || u.contains("gitlab.com"))
            .map(String::from)
    })
}
