//! Scoop package index fetcher (Windows).
//!
//! Fetches package metadata from Scoop buckets (main, extras, versions).

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// Scoop package index fetcher.
pub struct Scoop;

impl Scoop {
    /// Scoop search API.
    const SCOOP_API: &'static str = "https://scoop.sh/api";

    /// GitHub raw content for bucket manifests.
    const GITHUB_RAW: &'static str = "https://raw.githubusercontent.com";

    /// Main bucket.
    const MAIN_BUCKET: &'static str = "ScoopInstaller/Main/master/bucket";

    /// Extras bucket.
    const EXTRAS_BUCKET: &'static str = "ScoopInstaller/Extras/master/bucket";

    fn fetch_from_bucket(&self, name: &str, bucket: &str) -> Result<PackageMeta, IndexError> {
        let url = format!("{}/{}/{}.json", Self::GITHUB_RAW, bucket, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        Ok(PackageMeta {
            name: name.to_string(),
            version: response["version"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            description: response["description"].as_str().map(String::from),
            homepage: response["homepage"].as_str().map(String::from),
            repository: response["repository"]
                .as_str()
                .or_else(|| response["checkver"]["github"].as_str())
                .map(|s| {
                    if s.starts_with("http") {
                        s.to_string()
                    } else {
                        format!("https://github.com/{}", s)
                    }
                }),
            license: response["license"]
                .as_str()
                .or_else(|| response["license"]["identifier"].as_str())
                .map(String::from),
            binaries: response["bin"]
                .as_array()
                .map(|bins| {
                    bins.iter()
                        .filter_map(|b| {
                            b.as_str()
                                .or_else(|| b.as_array().and_then(|a| a.first()?.as_str()))
                                .map(|s| {
                                    // Extract just the binary name from path
                                    s.rsplit(['/', '\\']).next().unwrap_or(s).to_string()
                                })
                        })
                        .collect()
                })
                .unwrap_or_default(),
            ..Default::default()
        })
    }
}

impl PackageIndex for Scoop {
    fn ecosystem(&self) -> &'static str {
        "scoop"
    }

    fn display_name(&self) -> &'static str {
        "Scoop (Windows)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Try main bucket first, then extras
        self.fetch_from_bucket(name, Self::MAIN_BUCKET)
            .or_else(|_| self.fetch_from_bucket(name, Self::EXTRAS_BUCKET))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // Scoop manifests only contain current version
        // Could check versions bucket for historical versions
        let pkg = self.fetch(name)?;
        Ok(vec![VersionMeta {
            version: pkg.version,
            released: None,
            yanked: false,
        }])
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        // Use the scoop.sh search API
        let url = format!("{}/apps?q={}", Self::SCOOP_API, query);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let apps = response
            .as_array()
            .ok_or_else(|| IndexError::Parse("expected array".into()))?;

        Ok(apps
            .iter()
            .filter_map(|app| {
                Some(PackageMeta {
                    name: app["name"].as_str()?.to_string(),
                    version: app["version"].as_str().unwrap_or("unknown").to_string(),
                    description: app["description"].as_str().map(String::from),
                    homepage: app["homepage"].as_str().map(String::from),
                    repository: None,
                    license: app["license"].as_str().map(String::from),
                    binaries: Vec::new(),
                    ..Default::default()
                })
            })
            .collect())
    }
}
