//! Go module index fetcher.
//!
//! Fetches package metadata from pkg.go.dev and proxy.golang.org.

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// Go module index fetcher.
pub struct Go;

impl Go {
    /// Go proxy API.
    const GO_PROXY: &'static str = "https://proxy.golang.org";

    /// pkg.go.dev API (for metadata).
    const PKG_GO_DEV: &'static str = "https://pkg.go.dev";
}

impl PackageIndex for Go {
    fn ecosystem(&self) -> &'static str {
        "go"
    }

    fn display_name(&self) -> &'static str {
        "Go Modules"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Get latest version from proxy
        let versions = self.fetch_versions(name)?;
        let latest = versions
            .first()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        // Go modules are typically hosted on GitHub
        let repository = if name.starts_with("github.com/") {
            Some(format!("https://{}", name))
        } else if name.starts_with("golang.org/x/") {
            // Standard library extensions
            let pkg_name = name.strip_prefix("golang.org/x/").unwrap_or(name);
            Some(format!("https://github.com/golang/{}", pkg_name))
        } else {
            None
        };

        Ok(PackageMeta {
            name: name.to_string(),
            version: latest.version.clone(),
            description: None, // Would need to scrape pkg.go.dev
            homepage: Some(format!("{}/{}", Self::PKG_GO_DEV, name)),
            repository,
            license: None,
            binaries: Vec::new(),
            ..Default::default()
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let url = format!("{}/{}/@v/list", Self::GO_PROXY, name);
        let response = ureq::get(&url).call()?.into_string()?;

        let mut versions: Vec<VersionMeta> = response
            .lines()
            .filter(|line| !line.is_empty())
            .map(|version| VersionMeta {
                version: version.to_string(),
                released: None,
                yanked: false,
            })
            .collect();

        // Sort by semver (newest first) - simplified sorting
        versions.sort_by(|a, b| b.version.cmp(&a.version));

        if versions.is_empty() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        Ok(versions)
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        // Go doesn't have a search API in the proxy
        // Would need to scrape pkg.go.dev or use a third-party index
        Err(IndexError::Network(format!(
            "Go module search not available via API. Visit: {}/search?q={}",
            Self::PKG_GO_DEV,
            query
        )))
    }
}
