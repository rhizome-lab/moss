//! APT package index fetcher (Debian/Ubuntu).
//!
//! Fetches package metadata from Debian/Ubuntu repositories by parsing
//! Sources/Packages files from mirror indices.

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use crate::cache;
use flate2::read::GzDecoder;
use std::io::{BufRead, BufReader, Cursor, Read};
use std::time::Duration;

/// APT package index fetcher.
pub struct Apt;

/// Default cache TTL for package indices (1 hour).
const INDEX_CACHE_TTL: Duration = Duration::from_secs(60 * 60);

impl Apt {
    /// Default mirror URL (can be overridden).
    const DEFAULT_MIRROR: &'static str = "https://deb.debian.org/debian";

    /// Parse a Packages or Sources file in Debian control format.
    fn parse_control<R: Read>(reader: R) -> Vec<PackageMeta> {
        let reader = BufReader::new(reader);
        let mut packages = Vec::new();
        let mut current: Option<PackageBuilder> = None;

        for line in reader.lines().map_while(Result::ok) {
            if line.is_empty() {
                // End of stanza
                if let Some(builder) = current.take() {
                    if let Some(pkg) = builder.build() {
                        packages.push(pkg);
                    }
                }
                continue;
            }

            if line.starts_with(' ') || line.starts_with('\t') {
                // Continuation line - skip for now
                continue;
            }

            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim();
                let value = value.trim();

                let builder = current.get_or_insert_with(PackageBuilder::new);

                match key {
                    "Package" => builder.name = Some(value.to_string()),
                    "Version" => builder.version = Some(value.to_string()),
                    "Description" => builder.description = Some(value.to_string()),
                    "Homepage" => builder.homepage = Some(value.to_string()),
                    "Vcs-Git" | "Vcs-Browser" => {
                        if builder.repository.is_none() {
                            builder.repository = Some(value.to_string());
                        }
                    }
                    // New fields for nursery
                    "Filename" => builder.filename = Some(value.to_string()),
                    "SHA256" => builder.sha256 = Some(value.to_string()),
                    "Depends" => builder.depends = Some(value.to_string()),
                    "Size" => builder.size = value.parse().ok(),
                    _ => {}
                }
            }
        }

        // Handle last stanza
        if let Some(builder) = current {
            if let Some(pkg) = builder.build() {
                packages.push(pkg);
            }
        }

        packages
    }

    fn fetch_packages_gz(&self, url: &str) -> Result<Vec<PackageMeta>, IndexError> {
        // Try cache first
        let (data, _was_cached) = cache::fetch_with_cache(
            self.ecosystem(),
            "packages-stable-amd64",
            url,
            INDEX_CACHE_TTL,
        )
        .map_err(|e| IndexError::Network(e))?;

        let decoder = GzDecoder::new(Cursor::new(data));
        Ok(Self::parse_control(decoder))
    }
}

impl PackageIndex for Apt {
    fn ecosystem(&self) -> &'static str {
        "apt"
    }

    fn display_name(&self) -> &'static str {
        "APT (Debian/Ubuntu)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Use the API endpoint for single package lookup
        let url = format!(
            "https://sources.debian.org/api/src/{}/",
            urlencoding::encode(name)
        );

        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        if response.get("error").is_some() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        let versions = response["versions"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing versions".into()))?;

        let latest = versions
            .first()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        Ok(PackageMeta {
            name: name.to_string(),
            version: latest["version"].as_str().unwrap_or("unknown").to_string(),
            description: None, // Not available in this API
            homepage: response["homepage"].as_str().map(String::from),
            repository: response["vcs_url"].as_str().map(String::from),
            license: None,
            binaries: Vec::new(),
            ..Default::default()
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let url = format!(
            "https://sources.debian.org/api/src/{}/",
            urlencoding::encode(name)
        );

        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        if response.get("error").is_some() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        let versions = response["versions"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing versions".into()))?;

        Ok(versions
            .iter()
            .map(|v| VersionMeta {
                version: v["version"].as_str().unwrap_or("unknown").to_string(),
                released: None,
                yanked: false,
            })
            .collect())
    }

    fn supports_fetch_all(&self) -> bool {
        true
    }

    fn fetch_all(&self) -> Result<Vec<PackageMeta>, IndexError> {
        // Fetch the main Packages.gz from stable/main
        let url = format!(
            "{}/dists/stable/main/binary-amd64/Packages.gz",
            Self::DEFAULT_MIRROR
        );
        self.fetch_packages_gz(&url)
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        // Try local cached index first
        if let Some(data) = cache::read_index(self.ecosystem(), "packages-stable-amd64") {
            let decoder = GzDecoder::new(Cursor::new(data));
            let packages = Self::parse_control(decoder);
            let query_lower = query.to_lowercase();
            let results: Vec<PackageMeta> = packages
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
            if !results.is_empty() {
                return Ok(results);
            }
        }

        // Fall back to search API
        let url = format!(
            "https://sources.debian.org/api/search/{}/?suite=stable",
            urlencoding::encode(query)
        );

        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let results = response["results"]["exact"]
            .as_array()
            .or_else(|| response["results"]["other"].as_array())
            .ok_or_else(|| IndexError::Parse("missing results".into()))?;

        results
            .iter()
            .map(|r| {
                let name = r["name"].as_str().unwrap_or("").to_string();
                // Fetch full details for each result
                self.fetch(&name)
            })
            .collect()
    }
}

#[derive(Default)]
struct PackageBuilder {
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
    homepage: Option<String>,
    repository: Option<String>,
    // New fields for nursery
    filename: Option<String>,
    sha256: Option<String>,
    depends: Option<String>,
    size: Option<u64>,
}

impl PackageBuilder {
    fn new() -> Self {
        Self::default()
    }

    fn build(self) -> Option<PackageMeta> {
        let mut extra = std::collections::HashMap::new();

        // Store dependencies in extra
        if let Some(deps) = self.depends {
            let parsed_deps: Vec<String> = deps
                .split(',')
                .map(|d| {
                    // Strip version constraints: "libc6 (>= 2.17)" -> "libc6"
                    d.trim()
                        .split_once(' ')
                        .map(|(name, _)| name)
                        .unwrap_or(d.trim())
                        .to_string()
                })
                .filter(|d| !d.is_empty())
                .collect();
            extra.insert(
                "depends".to_string(),
                serde_json::Value::Array(
                    parsed_deps
                        .into_iter()
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            );
        }

        // Store size in extra
        if let Some(size) = self.size {
            extra.insert("size".to_string(), serde_json::Value::Number(size.into()));
        }

        Some(PackageMeta {
            name: self.name?,
            version: self.version?,
            description: self.description,
            homepage: self.homepage,
            repository: self.repository,
            license: None,
            binaries: Vec::new(),
            archive_url: self
                .filename
                .map(|f| format!("{}/{}", Apt::DEFAULT_MIRROR, f)),
            checksum: self.sha256.map(|h| format!("sha256:{}", h)),
            extra,
            ..Default::default()
        })
    }
}

mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut result = String::with_capacity(s.len() * 3);
        for c in s.chars() {
            match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
                _ => {
                    for b in c.to_string().bytes() {
                        result.push_str(&format!("%{:02X}", b));
                    }
                }
            }
        }
        result
    }
}
