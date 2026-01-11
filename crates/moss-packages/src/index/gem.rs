//! RubyGems package index fetcher (Ruby).
//!
//! Fetches package metadata from rubygems.org.
//!
//! ## API Strategy
//! - **fetch**: `rubygems.org/api/v1/gems/{name}.json` - Official RubyGems JSON API
//! - **fetch_versions**: `rubygems.org/api/v1/versions/{name}.json`
//! - **search**: `rubygems.org/api/v1/search.json?query=`
//! - **fetch_all**: Compact Index `/versions` endpoint (streaming)
//! - **iter_all**: Streaming iterator over `/versions` file

use super::{IndexError, PackageIndex, PackageIter, PackageMeta, VersionMeta};
use std::io::{BufRead, BufReader};

/// RubyGems package index fetcher.
pub struct Gem;

impl Gem {
    /// RubyGems API.
    const RUBYGEMS_API: &'static str = "https://rubygems.org/api/v1";
    /// Compact Index base URL.
    const COMPACT_INDEX: &'static str = "https://rubygems.org";
}

impl PackageIndex for Gem {
    fn ecosystem(&self) -> &'static str {
        "gem"
    }

    fn display_name(&self) -> &'static str {
        "RubyGems (Ruby)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let url = format!("{}/gems/{}.json", Self::RUBYGEMS_API, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        Ok(PackageMeta {
            name: response["name"].as_str().unwrap_or(name).to_string(),
            version: response["version"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            description: response["info"].as_str().map(String::from),
            homepage: response["homepage_uri"].as_str().map(String::from),
            repository: response["source_code_uri"]
                .as_str()
                .or_else(|| {
                    response["homepage_uri"]
                        .as_str()
                        .filter(|u| u.contains("github.com"))
                })
                .map(String::from),
            license: response["licenses"]
                .as_array()
                .and_then(|l| l.first())
                .and_then(|l| l.as_str())
                .map(String::from),
            binaries: response["executables"]
                .as_array()
                .map(|exes| {
                    exes.iter()
                        .filter_map(|e| e.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            keywords: Vec::new(), // RubyGems doesn't expose keywords
            maintainers: response["authors"]
                .as_str()
                .map(|a| a.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_default(),
            published: response["version_created_at"]
                .as_str()
                .or_else(|| response["created_at"].as_str())
                .map(String::from),
            downloads: response["version_downloads"]
                .as_u64()
                .or_else(|| response["downloads"].as_u64()),
            archive_url: response["gem_uri"].as_str().map(String::from),
            checksum: response["sha"].as_str().map(|h| format!("sha256:{}", h)),
            extra: Default::default(),
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let url = format!("{}/versions/{}.json", Self::RUBYGEMS_API, name);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let versions = response
            .as_array()
            .ok_or_else(|| IndexError::Parse("expected array".into()))?;

        Ok(versions
            .iter()
            .filter_map(|v| {
                Some(VersionMeta {
                    version: v["number"].as_str()?.to_string(),
                    released: v["created_at"].as_str().map(String::from),
                    yanked: v["yanked"].as_bool().unwrap_or(false),
                })
            })
            .collect())
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!("{}/search.json?query={}", Self::RUBYGEMS_API, query);
        let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

        let gems = response
            .as_array()
            .ok_or_else(|| IndexError::Parse("expected array".into()))?;

        Ok(gems
            .iter()
            .filter_map(|gem| {
                Some(PackageMeta {
                    name: gem["name"].as_str()?.to_string(),
                    version: gem["version"].as_str().unwrap_or("unknown").to_string(),
                    description: gem["info"].as_str().map(String::from),
                    homepage: gem["homepage_uri"].as_str().map(String::from),
                    repository: gem["source_code_uri"]
                        .as_str()
                        .or_else(|| {
                            gem["homepage_uri"]
                                .as_str()
                                .filter(|u| u.contains("github.com"))
                        })
                        .map(String::from),
                    license: gem["licenses"]
                        .as_array()
                        .and_then(|l| l.first())
                        .and_then(|l| l.as_str())
                        .map(String::from),
                    binaries: Vec::new(), // Not in search results
                    keywords: Vec::new(),
                    maintainers: gem["authors"]
                        .as_str()
                        .map(|a| a.split(',').map(|s| s.trim().to_string()).collect())
                        .unwrap_or_default(),
                    published: None, // Not in search results
                    downloads: gem["downloads"].as_u64(),
                    archive_url: gem["gem_uri"].as_str().map(String::from),
                    checksum: gem["sha"].as_str().map(|h| format!("sha256:{}", h)),
                    extra: Default::default(),
                })
            })
            .collect())
    }

    fn supports_fetch_all(&self) -> bool {
        true
    }

    fn fetch_all(&self) -> Result<Vec<PackageMeta>, IndexError> {
        // Use iter_all and collect
        self.iter_all()?.collect()
    }

    fn iter_all(&self) -> Result<PackageIter<'_>, IndexError> {
        let url = format!("{}/versions", Self::COMPACT_INDEX);
        let response = ureq::get(&url).call()?;
        let reader = BufReader::new(response.into_reader());

        Ok(Box::new(GemVersionsIter {
            reader,
            seen: std::collections::HashSet::new(),
        }))
    }
}

/// Iterator over RubyGems Compact Index /versions file.
/// Format: gem_name version1,version2,... md5_hash
struct GemVersionsIter<R: BufRead> {
    reader: R,
    /// Track seen gem names to deduplicate (versions file is append-only, same gem can appear multiple times)
    seen: std::collections::HashSet<String>,
}

impl<R: BufRead + Send> Iterator for GemVersionsIter<R> {
    type Item = Result<PackageMeta, IndexError>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut line = String::new();

        loop {
            line.clear();
            match self.reader.read_line(&mut line) {
                Ok(0) => return None, // EOF
                Ok(_) => {
                    let line = line.trim();

                    // Skip header line (starts with "---")
                    if line.starts_with("---") || line.is_empty() {
                        continue;
                    }

                    // Format: gem_name version1,version2,... md5_hash
                    // Split from the end to handle gem names with spaces
                    let parts: Vec<&str> = line.rsplitn(3, ' ').collect();
                    if parts.len() < 2 {
                        continue;
                    }

                    // parts is [md5, versions, name] due to rsplitn
                    let name = if parts.len() == 3 {
                        parts[2].to_string()
                    } else {
                        parts[1].to_string()
                    };

                    // Skip if we've already seen this gem (return only latest entry)
                    if self.seen.contains(&name) {
                        continue;
                    }

                    let versions_str = if parts.len() == 3 { parts[1] } else { parts[0] };

                    // Parse versions (comma-separated, may include platform like "1.0.0-java")
                    let versions: Vec<&str> = versions_str.split(',').collect();
                    let latest = versions.last().unwrap_or(&"unknown");

                    // Extract version number (strip platform suffix)
                    let version = latest.split('-').next().unwrap_or(latest).to_string();

                    self.seen.insert(name.clone());

                    return Some(Ok(PackageMeta {
                        name,
                        version,
                        description: None, // Compact index doesn't include this
                        homepage: None,
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
                    }));
                }
                Err(e) => return Some(Err(IndexError::Io(e))),
            }
        }
    }
}
