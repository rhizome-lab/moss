//! Maven Central package index fetcher (Java).
//!
//! Fetches package metadata from Maven Central.

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// Maven Central package index fetcher.
pub struct Maven;

impl Maven {
    /// Maven Central search API.
    const MAVEN_API: &'static str = "https://search.maven.org/solrsearch/select";
}

impl PackageIndex for Maven {
    fn ecosystem(&self) -> &'static str {
        "maven"
    }

    fn display_name(&self) -> &'static str {
        "Maven Central (Java)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Maven uses groupId:artifactId format
        let (group_id, artifact_id) = if let Some((g, a)) = name.split_once(':') {
            (g, a)
        } else {
            // Assume it's just the artifactId, search for it
            return self
                .search(name)?
                .into_iter()
                .next()
                .ok_or_else(|| IndexError::NotFound(name.to_string()));
        };

        let url = format!(
            "{}?q=g:{}+AND+a:{}&rows=1&wt=json",
            Self::MAVEN_API,
            group_id,
            artifact_id
        );
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let docs = response["response"]["docs"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing docs".into()))?;

        let doc = docs
            .first()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        Ok(PackageMeta {
            name: format!(
                "{}:{}",
                doc["g"].as_str().unwrap_or(""),
                doc["a"].as_str().unwrap_or("")
            ),
            version: doc["latestVersion"]
                .as_str()
                .or_else(|| doc["v"].as_str())
                .unwrap_or("unknown")
                .to_string(),
            description: None, // Maven search doesn't include description
            homepage: Some(format!(
                "https://mvnrepository.com/artifact/{}/{}",
                doc["g"].as_str().unwrap_or(""),
                doc["a"].as_str().unwrap_or("")
            )),
            repository: None, // Would need to fetch POM for this
            license: None,
            binaries: Vec::new(),
            ..Default::default()
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let (group_id, artifact_id) = if let Some((g, a)) = name.split_once(':') {
            (g, a)
        } else {
            return Err(IndexError::Parse(
                "Maven package name must be groupId:artifactId".into(),
            ));
        };

        let url = format!(
            "{}?q=g:{}+AND+a:{}&core=gav&rows=100&wt=json",
            Self::MAVEN_API,
            group_id,
            artifact_id
        );
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let docs = response["response"]["docs"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing docs".into()))?;

        Ok(docs
            .iter()
            .filter_map(|doc| {
                Some(VersionMeta {
                    version: doc["v"].as_str()?.to_string(),
                    released: doc["timestamp"].as_i64().map(|ts| {
                        // Convert epoch millis to ISO date
                        let secs = ts / 1000;
                        format!("{}", secs) // Simplified, ideally use chrono
                    }),
                    yanked: false,
                })
            })
            .collect())
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!("{}?q={}&rows=50&wt=json", Self::MAVEN_API, query);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let docs = response["response"]["docs"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing docs".into()))?;

        Ok(docs
            .iter()
            .filter_map(|doc| {
                Some(PackageMeta {
                    name: format!(
                        "{}:{}",
                        doc["g"].as_str().unwrap_or(""),
                        doc["a"].as_str().unwrap_or("")
                    ),
                    version: doc["latestVersion"]
                        .as_str()
                        .or_else(|| doc["v"].as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    description: None,
                    homepage: Some(format!(
                        "https://mvnrepository.com/artifact/{}/{}",
                        doc["g"].as_str().unwrap_or(""),
                        doc["a"].as_str().unwrap_or("")
                    )),
                    repository: None,
                    license: None,
                    binaries: Vec::new(),
                    ..Default::default()
                })
            })
            .collect())
    }
}
