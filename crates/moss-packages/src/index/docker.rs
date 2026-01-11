//! Docker Hub package index fetcher.
//!
//! Fetches image metadata from Docker Hub API.

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// Docker Hub package index fetcher.
pub struct Docker;

impl Docker {
    /// Docker Hub API base URL.
    const API_BASE: &'static str = "https://hub.docker.com/v2";
}

impl PackageIndex for Docker {
    fn ecosystem(&self) -> &'static str {
        "docker"
    }

    fn display_name(&self) -> &'static str {
        "Docker Hub"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Handle both "library/nginx" (official) and "user/repo" formats
        let (namespace, repo) = if name.contains('/') {
            let parts: Vec<&str> = name.splitn(2, '/').collect();
            (parts[0], parts[1])
        } else {
            ("library", name)
        };

        let url = format!("{}/repositories/{}/{}/", Self::API_BASE, namespace, repo);
        let response: serde_json::Value = ureq::get(&url)
            .call()
            .map_err(|_| IndexError::NotFound(name.to_string()))?
            .into_json()?;

        // Get latest tag info
        let tags_url = format!(
            "{}/repositories/{}/{}/tags?page_size=1&ordering=-last_updated",
            Self::API_BASE,
            namespace,
            repo
        );
        let tags: serde_json::Value = ureq::get(&tags_url)
            .call()
            .map_err(|_| IndexError::NotFound(name.to_string()))?
            .into_json()?;

        let latest_tag = tags["results"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|t| t["name"].as_str())
            .unwrap_or("latest");

        // Extract categories as keywords
        let keywords: Vec<String> = response["categories"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| c["slug"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        Ok(PackageMeta {
            name: format!(
                "{}/{}",
                namespace,
                response["name"].as_str().unwrap_or(repo)
            ),
            version: latest_tag.to_string(),
            description: response["description"].as_str().map(String::from),
            homepage: None,
            repository: None,
            license: None,
            binaries: Vec::new(),
            keywords,
            maintainers: vec![
                response["namespace"]
                    .as_str()
                    .unwrap_or(namespace)
                    .to_string(),
            ],
            downloads: response["pull_count"].as_u64(),
            published: response["last_updated"].as_str().map(String::from),
            ..Default::default()
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let (namespace, repo) = if name.contains('/') {
            let parts: Vec<&str> = name.splitn(2, '/').collect();
            (parts[0], parts[1])
        } else {
            ("library", name)
        };

        let url = format!(
            "{}/repositories/{}/{}/tags?page_size=50&ordering=-last_updated",
            Self::API_BASE,
            namespace,
            repo
        );
        let response: serde_json::Value = ureq::get(&url)
            .call()
            .map_err(|_| IndexError::NotFound(name.to_string()))?
            .into_json()?;

        let tags = response["results"]
            .as_array()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        Ok(tags
            .iter()
            .filter_map(|t| {
                Some(VersionMeta {
                    version: t["name"].as_str()?.to_string(),
                    released: t["last_updated"].as_str().map(String::from),
                    yanked: false,
                })
            })
            .collect())
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!(
            "{}/search/repositories?query={}&page_size=25",
            Self::API_BASE,
            query
        );
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let results = response["results"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("Invalid search response".into()))?;

        Ok(results
            .iter()
            .filter_map(|img| {
                let name = if img["is_official"].as_bool().unwrap_or(false) {
                    format!("library/{}", img["repo_name"].as_str()?)
                } else {
                    img["repo_name"].as_str()?.to_string()
                };

                Some(PackageMeta {
                    name,
                    version: "latest".to_string(),
                    description: img["short_description"].as_str().map(String::from),
                    downloads: img["pull_count"].as_u64(),
                    ..Default::default()
                })
            })
            .collect())
    }
}
