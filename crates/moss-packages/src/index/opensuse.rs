//! openSUSE package index fetcher.
//!
//! Fetches package metadata from openSUSE Tumbleweed repositories.
//!
//! ## API Strategy
//! - **fetch**: Searches cached `download.opensuse.org/.../repodata/primary.xml.zst` (zstd XML)
//! - **fetch_versions**: Same, single version per package
//! - **search**: Filters cached primary.xml data
//! - **fetch_all**: Full primary.xml (cached 1 hour, ~170MB uncompressed)

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use crate::cache;
use std::time::Duration;

/// Cache TTL for openSUSE package index (1 hour).
const CACHE_TTL: Duration = Duration::from_secs(60 * 60);

/// openSUSE package index fetcher.
pub struct OpenSuse;

impl OpenSuse {
    /// openSUSE Tumbleweed repository base URL.
    const REPO_BASE: &'static str = "https://download.opensuse.org/tumbleweed/repo/oss/repodata";

    /// Find primary.xml.zst URL from repomd.xml.
    fn find_primary_url() -> Result<String, IndexError> {
        let repomd_url = format!("{}/repomd.xml", Self::REPO_BASE);
        let (data, _) = cache::fetch_with_cache("opensuse", "repomd", &repomd_url, CACHE_TTL)
            .map_err(IndexError::Network)?;

        let xml = String::from_utf8_lossy(&data);

        // Parse repomd.xml to find primary.xml.zst location
        // Looking for: <location href="repodata/...-primary.xml.zst"/>
        for line in xml.lines() {
            if line.contains("primary.xml.zst") {
                if let Some(start) = line.find("href=\"") {
                    let rest = &line[start + 6..];
                    if let Some(end) = rest.find('"') {
                        let href = &rest[..end];
                        // href is like "repodata/xxx-primary.xml.zst"
                        // We need full URL
                        return Ok(format!(
                            "https://download.opensuse.org/tumbleweed/repo/oss/{}",
                            href
                        ));
                    }
                }
            }
        }

        Err(IndexError::Parse(
            "primary.xml.zst not found in repomd.xml".into(),
        ))
    }

    /// Parse primary.xml to extract packages.
    fn parse_primary(xml: &str) -> Vec<PackageMeta> {
        let mut packages = Vec::new();
        let mut in_package = false;
        let mut name = String::new();
        let mut version = String::new();
        let mut summary = String::new();
        let mut description = String::new();
        let mut url = String::new();
        let mut license = String::new();

        for line in xml.lines() {
            let line = line.trim();

            if line.starts_with("<package type=\"rpm\">") {
                in_package = true;
                name.clear();
                version.clear();
                summary.clear();
                description.clear();
                url.clear();
                license.clear();
            } else if line == "</package>" && in_package {
                if !name.is_empty() {
                    packages.push(PackageMeta {
                        name: name.clone(),
                        version: version.clone(),
                        description: if summary.is_empty() {
                            None
                        } else {
                            Some(summary.clone())
                        },
                        homepage: if url.is_empty() {
                            None
                        } else {
                            Some(url.clone())
                        },
                        repository: Some(
                            "https://build.opensuse.org/project/show/openSUSE:Factory".to_string(),
                        ),
                        license: if license.is_empty() {
                            None
                        } else {
                            Some(license.clone())
                        },
                        ..Default::default()
                    });
                }
                in_package = false;
            } else if in_package {
                // Extract simple tags
                if line.starts_with("<name>") && line.ends_with("</name>") {
                    name = line[6..line.len() - 7].to_string();
                } else if line.starts_with("<summary>") && line.ends_with("</summary>") {
                    summary = line[9..line.len() - 10].to_string();
                } else if line.starts_with("<url>") && line.ends_with("</url>") {
                    url = line[5..line.len() - 6].to_string();
                } else if line.starts_with("<rpm:license>") && line.ends_with("</rpm:license>") {
                    license = line[13..line.len() - 14].to_string();
                } else if line.starts_with("<version ") {
                    // <version epoch="0" ver="0.27.1" rel="2.6"/>
                    if let Some(ver_start) = line.find("ver=\"") {
                        let rest = &line[ver_start + 5..];
                        if let Some(ver_end) = rest.find('"') {
                            version = rest[..ver_end].to_string();
                        }
                    }
                }
            }
        }

        packages
    }

    /// Load and parse the package index.
    fn load_packages() -> Result<Vec<PackageMeta>, IndexError> {
        let primary_url = Self::find_primary_url()?;

        let (data, _was_cached) =
            cache::fetch_with_cache("opensuse", "primary", &primary_url, CACHE_TTL)
                .map_err(IndexError::Network)?;

        // Decompress zstd
        let decompressed = zstd::decode_all(std::io::Cursor::new(&data))
            .map_err(|e| IndexError::Decompress(e.to_string()))?;

        let xml = String::from_utf8_lossy(&decompressed);
        Ok(Self::parse_primary(&xml))
    }
}

impl PackageIndex for OpenSuse {
    fn ecosystem(&self) -> &'static str {
        "opensuse"
    }

    fn display_name(&self) -> &'static str {
        "openSUSE (zypper)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let packages = Self::load_packages()?;

        packages
            .into_iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| IndexError::NotFound(name.to_string()))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let pkg = self.fetch(name)?;
        Ok(vec![VersionMeta {
            version: pkg.version,
            released: None,
            yanked: false,
        }])
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let packages = Self::load_packages()?;
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

    fn fetch_all(&self) -> Result<Vec<PackageMeta>, IndexError> {
        Self::load_packages()
    }
}
