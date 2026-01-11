//! APK package index fetcher (Alpine Linux).
//!
//! Fetches package metadata from Alpine Linux repositories by parsing
//! APKINDEX.tar.gz files from mirrors.

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use crate::cache;
use flate2::read::MultiGzDecoder;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Cursor, Read};
use std::time::Duration;
use tar::Archive;

/// Cache TTL for APKINDEX (1 hour).
const INDEX_CACHE_TTL: Duration = Duration::from_secs(60 * 60);

/// APK package index fetcher.
pub struct Apk;

impl Apk {
    /// Alpine mirror URL.
    const ALPINE_MIRROR: &'static str = "https://dl-cdn.alpinelinux.org/alpine";

    /// Default branch.
    const DEFAULT_BRANCH: &'static str = "edge";

    /// Default repository.
    const DEFAULT_REPO: &'static str = "main";

    /// Default architecture.
    const DEFAULT_ARCH: &'static str = "x86_64";

    /// Parse APKINDEX format into PackageMeta entries.
    fn parse_apkindex<R: Read>(reader: R) -> Vec<PackageMeta> {
        let reader = BufReader::new(reader);
        let mut packages = Vec::new();
        let mut current = ApkPackageBuilder::new();

        for line in reader.lines().map_while(Result::ok) {
            if line.is_empty() {
                // End of stanza
                if let Some(pkg) = current.build() {
                    packages.push(pkg);
                }
                current = ApkPackageBuilder::new();
                continue;
            }

            // Single-letter field format: "X:value"
            if line.len() >= 2 && line.chars().nth(1) == Some(':') {
                let key = line.chars().next().unwrap();
                let value = &line[2..];

                match key {
                    'P' => current.name = Some(value.to_string()),
                    'V' => current.version = Some(value.to_string()),
                    'T' => current.description = Some(value.to_string()),
                    'U' => current.homepage = Some(value.to_string()),
                    'L' => current.license = Some(value.to_string()),
                    'S' => current.size = value.parse().ok(),
                    'C' => current.checksum = Some(value.to_string()),
                    'D' => current.depends = Some(value.to_string()),
                    'm' => current.maintainer = Some(value.to_string()),
                    'o' => current.origin = Some(value.to_string()),
                    'A' => current.arch = Some(value.to_string()),
                    'p' => current.provides = Some(value.to_string()),
                    _ => {}
                }
            }
        }

        // Handle last stanza
        if let Some(pkg) = current.build() {
            packages.push(pkg);
        }

        packages
    }

    /// Fetch and parse APKINDEX.tar.gz from a repository.
    fn fetch_apkindex(
        &self,
        branch: &str,
        repo: &str,
        arch: &str,
    ) -> Result<Vec<PackageMeta>, IndexError> {
        let url = format!(
            "{}/{}/{}/{}/APKINDEX.tar.gz",
            Self::ALPINE_MIRROR,
            branch,
            repo,
            arch
        );

        // Try cache first
        let (data, _was_cached) = cache::fetch_with_cache(
            self.ecosystem(),
            &format!("apkindex-{}-{}-{}", branch, repo, arch),
            &url,
            INDEX_CACHE_TTL,
        )
        .map_err(|e| IndexError::Network(e))?;

        // Decompress gzip (APKINDEX uses multi-member gzip)
        let mut decoder = MultiGzDecoder::new(Cursor::new(data));
        let mut tar_data = Vec::new();
        decoder
            .read_to_end(&mut tar_data)
            .map_err(|e| IndexError::Io(e))?;

        let mut archive = Archive::new(Cursor::new(tar_data));

        for entry in archive.entries().map_err(|e| IndexError::Io(e))? {
            let mut entry = entry.map_err(|e| IndexError::Io(e))?;
            let path = entry
                .path()
                .map_err(|e| IndexError::Io(e))?
                .to_string_lossy()
                .to_string();

            // Read entry content - must consume it to advance the iterator
            let mut content = Vec::new();
            entry
                .read_to_end(&mut content)
                .map_err(|e| IndexError::Io(e))?;

            if path == "APKINDEX" {
                return Ok(Self::parse_apkindex(Cursor::new(content)));
            }
        }

        Err(IndexError::Parse("APKINDEX not found in archive".into()))
    }
}

impl PackageIndex for Apk {
    fn ecosystem(&self) -> &'static str {
        "apk"
    }

    fn display_name(&self) -> &'static str {
        "APK (Alpine Linux)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Fetch from APKINDEX and find the package
        let packages =
            self.fetch_apkindex(Self::DEFAULT_BRANCH, Self::DEFAULT_REPO, Self::DEFAULT_ARCH)?;

        packages
            .into_iter()
            .find(|p| p.name == name)
            .ok_or_else(|| {
                // Try community repo if not in main
                if let Ok(community) =
                    self.fetch_apkindex(Self::DEFAULT_BRANCH, "community", Self::DEFAULT_ARCH)
                {
                    if let Some(pkg) = community.into_iter().find(|p| p.name == name) {
                        return IndexError::NotFound(format!("found: {}", pkg.name));
                    }
                }
                IndexError::NotFound(name.to_string())
            })
            .or_else(|e| {
                // Handle the "found" case from the closure
                if let IndexError::NotFound(msg) = &e {
                    if msg.starts_with("found: ") {
                        // Re-fetch from community
                        let community = self.fetch_apkindex(
                            Self::DEFAULT_BRANCH,
                            "community",
                            Self::DEFAULT_ARCH,
                        )?;
                        return community
                            .into_iter()
                            .find(|p| p.name == name)
                            .ok_or_else(|| IndexError::NotFound(name.to_string()));
                    }
                }
                Err(e)
            })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // Query across multiple branches to get version history
        let branches = ["edge", "v3.21", "v3.20", "v3.19"];
        let mut versions = Vec::new();

        for branch in branches {
            for repo in ["main", "community"] {
                if let Ok(packages) = self.fetch_apkindex(branch, repo, Self::DEFAULT_ARCH) {
                    for pkg in packages {
                        if pkg.name == name
                            && !versions
                                .iter()
                                .any(|v: &VersionMeta| v.version == pkg.version)
                        {
                            versions.push(VersionMeta {
                                version: pkg.version,
                                released: None,
                                yanked: false,
                            });
                        }
                    }
                }
            }
        }

        if versions.is_empty() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        Ok(versions)
    }

    fn supports_fetch_all(&self) -> bool {
        true
    }

    fn fetch_all(&self) -> Result<Vec<PackageMeta>, IndexError> {
        // Fetch main and community repos
        let mut packages =
            self.fetch_apkindex(Self::DEFAULT_BRANCH, Self::DEFAULT_REPO, Self::DEFAULT_ARCH)?;
        if let Ok(community) =
            self.fetch_apkindex(Self::DEFAULT_BRANCH, "community", Self::DEFAULT_ARCH)
        {
            packages.extend(community);
        }
        Ok(packages)
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        // Search locally cached index
        let packages = self.fetch_all()?;
        let query_lower = query.to_lowercase();

        Ok(packages
            .into_iter()
            .filter(|p| {
                p.name.to_lowercase().contains(&query_lower)
                    || p.description
                        .as_ref()
                        .map(|d| d.to_lowercase().contains(&query_lower))
                        .unwrap_or(false)
            })
            .take(50)
            .collect())
    }
}

/// Builder for APK package metadata.
#[derive(Default)]
struct ApkPackageBuilder {
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
    homepage: Option<String>,
    license: Option<String>,
    size: Option<u64>,
    checksum: Option<String>,
    depends: Option<String>,
    maintainer: Option<String>,
    origin: Option<String>,
    arch: Option<String>,
    provides: Option<String>,
}

impl ApkPackageBuilder {
    fn new() -> Self {
        Self::default()
    }

    fn build(self) -> Option<PackageMeta> {
        let name = self.name?;
        let version = self.version?;

        let mut extra = HashMap::new();

        // Parse dependencies
        if let Some(deps) = self.depends {
            let parsed_deps: Vec<serde_json::Value> = deps
                .split_whitespace()
                .filter(|d| {
                    // Filter out so: dependencies (shared library deps)
                    !d.starts_with("so:")
                })
                .map(|d| {
                    // Strip version constraints and prefixes
                    let name = d
                        .split(|c| c == '>' || c == '<' || c == '=' || c == '~')
                        .next()
                        .unwrap_or(d);
                    serde_json::Value::String(name.to_string())
                })
                .collect();
            if !parsed_deps.is_empty() {
                extra.insert("depends".to_string(), serde_json::Value::Array(parsed_deps));
            }
        }

        // Store size
        if let Some(size) = self.size {
            extra.insert("size".to_string(), serde_json::Value::Number(size.into()));
        }

        // Store origin package
        if let Some(origin) = self.origin {
            extra.insert("origin".to_string(), serde_json::Value::String(origin));
        }

        // Build download URL
        let archive_url = Some(format!(
            "https://dl-cdn.alpinelinux.org/alpine/edge/main/x86_64/{}-{}.apk",
            name, version
        ));

        // Convert checksum (Q1... is SHA1 in base64)
        let checksum = self.checksum.map(|c| {
            if c.starts_with("Q1") {
                format!("sha1-base64:{}", &c[2..])
            } else {
                c
            }
        });

        Some(PackageMeta {
            name,
            version,
            description: self.description,
            homepage: self.homepage,
            repository: None,
            license: self.license,
            binaries: Vec::new(),
            maintainers: self.maintainer.into_iter().collect(),
            archive_url,
            checksum,
            extra,
            ..Default::default()
        })
    }
}
