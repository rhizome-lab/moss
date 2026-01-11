//! Julia package index fetcher.
//!
//! Fetches package metadata from the Julia General registry on GitHub.
//!
//! ## API Strategy
//! - **fetch**: `github.com/JuliaRegistries/General/.../Package.toml` - GitHub raw files
//! - **fetch_versions**: `github.com/JuliaRegistries/General/.../Versions.toml`
//! - **search**: Not supported (would need to download full registry)
//! - **fetch_all**: Not supported (too many packages)

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// Julia package index fetcher.
pub struct Julia;

impl Julia {
    /// GitHub raw content base.
    const REGISTRY_RAW: &'static str =
        "https://raw.githubusercontent.com/JuliaRegistries/General/master";
}

impl PackageIndex for Julia {
    fn ecosystem(&self) -> &'static str {
        "julia"
    }

    fn display_name(&self) -> &'static str {
        "Julia (General registry)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Julia packages are organized by first letter, then name
        let first_letter = name
            .chars()
            .next()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?
            .to_uppercase()
            .to_string();

        // Fetch Package.toml for metadata
        let pkg_url = format!(
            "{}/{}/{}/Package.toml",
            Self::REGISTRY_RAW,
            first_letter,
            name
        );
        let pkg_toml = ureq::get(&pkg_url)
            .call()
            .map_err(|_| IndexError::NotFound(name.to_string()))?
            .into_string()
            .map_err(|e| IndexError::Io(e))?;

        // Fetch Versions.toml for version info
        let versions_url = format!(
            "{}/{}/{}/Versions.toml",
            Self::REGISTRY_RAW,
            first_letter,
            name
        );
        let versions_toml = ureq::get(&versions_url)
            .call()
            .map_err(|_| IndexError::NotFound(name.to_string()))?
            .into_string()
            .map_err(|e| IndexError::Io(e))?;

        let pkg = parse_package_toml(&pkg_toml);
        let versions = parse_versions_toml(&versions_toml);

        let latest = versions
            .first()
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());

        Ok(PackageMeta {
            name: pkg.name.unwrap_or_else(|| name.to_string()),
            version: latest,
            description: None, // Package.toml doesn't include description
            homepage: None,
            repository: pkg.repo,
            license: None,
            binaries: Vec::new(),
            keywords: Vec::new(),
            maintainers: Vec::new(),
            published: None,
            downloads: None,
            archive_url: None,
            checksum: None,
            extra: Default::default(),
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let first_letter = name
            .chars()
            .next()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?
            .to_uppercase()
            .to_string();

        let versions_url = format!(
            "{}/{}/{}/Versions.toml",
            Self::REGISTRY_RAW,
            first_letter,
            name
        );
        let versions_toml = ureq::get(&versions_url)
            .call()
            .map_err(|_| IndexError::NotFound(name.to_string()))?
            .into_string()
            .map_err(|e| IndexError::Io(e))?;

        let versions = parse_versions_toml(&versions_toml);

        if versions.is_empty() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        Ok(versions
            .into_iter()
            .map(|v| VersionMeta {
                version: v,
                released: None,
                yanked: false,
            })
            .collect())
    }

    fn search(&self, _query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        // Julia doesn't have a search API; would need to list all packages
        // For now, return an error indicating search is not efficient
        Err(IndexError::Parse(
            "Julia registry search requires downloading full registry (not implemented)".into(),
        ))
    }
}

struct JuliaPackage {
    name: Option<String>,
    repo: Option<String>,
}

fn parse_package_toml(content: &str) -> JuliaPackage {
    let mut pkg = JuliaPackage {
        name: None,
        repo: None,
    };

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("name") {
            pkg.name = extract_toml_string(line);
        } else if line.starts_with("repo") {
            pkg.repo = extract_toml_string(line);
        }
    }

    pkg
}

fn parse_versions_toml(content: &str) -> Vec<String> {
    let mut versions = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        // Version lines look like: [1.2.3]
        if line.starts_with('[') && line.ends_with(']') {
            let version = &line[1..line.len() - 1];
            // Skip sha1 hashes and metadata sections
            if !version.contains('.') || version.len() > 20 {
                continue;
            }
            versions.push(version.to_string());
        }
    }

    // Sort descending
    versions.sort_by(|a, b| version_compare(b, a));
    versions
}

fn extract_toml_string(line: &str) -> Option<String> {
    let eq_pos = line.find('=')?;
    let value = line[eq_pos + 1..].trim();
    if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        Some(value[1..value.len() - 1].to_string())
    } else {
        None
    }
}

fn version_compare(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| -> Vec<u32> {
        s.split(|c: char| !c.is_ascii_digit())
            .filter_map(|p| p.parse().ok())
            .collect()
    };
    parse(a).cmp(&parse(b))
}
