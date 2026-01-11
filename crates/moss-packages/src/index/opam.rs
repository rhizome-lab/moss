//! OPAM package index fetcher (OCaml).
//!
//! Fetches package metadata from the opam-repository on GitHub.
//! OPAM doesn't have a public JSON API, so we parse opam files from GitHub.
//!
//! ## API Strategy
//! - **fetch**: `github.com/ocaml/opam-repository/.../opam` - GitHub raw files
//! - **fetch_versions**: GitHub API to list version directories
//! - **search**: Not supported (would need GitHub API pagination)
//! - **fetch_all**: Not supported (too many packages)

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// OPAM package index fetcher.
pub struct Opam;

impl Opam {
    /// GitHub raw content base for opam-repository.
    const GITHUB_RAW: &'static str =
        "https://raw.githubusercontent.com/ocaml/opam-repository/master/packages";

    /// GitHub API base for listing.
    const GITHUB_API: &'static str =
        "https://api.github.com/repos/ocaml/opam-repository/contents/packages";

    /// Parse an opam file to extract metadata.
    fn parse_opam_file(content: &str) -> OpamMeta {
        let mut meta = OpamMeta::default();

        for line in content.lines() {
            let line = line.trim();

            if line.starts_with("synopsis:") {
                meta.synopsis = extract_string(line, "synopsis:");
            } else if line.starts_with("description:") {
                meta.description = extract_string(line, "description:");
            } else if line.starts_with("homepage:") {
                meta.homepage = extract_string(line, "homepage:");
            } else if line.starts_with("bug-reports:") {
                meta.bug_reports = extract_string(line, "bug-reports:");
            } else if line.starts_with("dev-repo:") {
                meta.dev_repo = extract_string(line, "dev-repo:");
            } else if line.starts_with("license:") {
                meta.license = extract_string(line, "license:");
            } else if line.starts_with("maintainer:") {
                meta.maintainer = extract_string(line, "maintainer:");
            } else if line.starts_with("authors:") {
                meta.authors = extract_string(line, "authors:");
            }
        }

        meta
    }
}

#[derive(Default)]
struct OpamMeta {
    synopsis: Option<String>,
    description: Option<String>,
    homepage: Option<String>,
    bug_reports: Option<String>,
    dev_repo: Option<String>,
    license: Option<String>,
    maintainer: Option<String>,
    authors: Option<String>,
}

/// Extract a quoted string from an opam field.
fn extract_string(line: &str, prefix: &str) -> Option<String> {
    let value = line.strip_prefix(prefix)?.trim();
    if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        Some(value[1..value.len() - 1].to_string())
    } else if value.starts_with('"') {
        // Multi-line or unclosed - just take what's after the quote
        Some(value[1..].trim_end_matches('"').to_string())
    } else {
        Some(value.to_string())
    }
}

impl PackageIndex for Opam {
    fn ecosystem(&self) -> &'static str {
        "opam"
    }

    fn display_name(&self) -> &'static str {
        "OPAM (OCaml)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // First, list versions from GitHub API
        let api_url = format!("{}/{}", Self::GITHUB_API, name);
        let response: serde_json::Value = ureq::get(&api_url)
            .set("User-Agent", "moss-packages/0.1")
            .set("Accept", "application/vnd.github.v3+json")
            .call()?
            .into_json()?;

        let versions: Vec<&str> = response
            .as_array()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?
            .iter()
            .filter_map(|entry| {
                let dir_name = entry["name"].as_str()?;
                // Directory format: package.version
                dir_name.strip_prefix(&format!("{}.", name))
            })
            .collect();

        if versions.is_empty() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        // Get the latest version (sort semver-style)
        let latest = versions
            .iter()
            .max_by(|a, b| version_compare(a, b))
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        // Fetch the opam file for the latest version
        let opam_url = format!("{}/{}/{}.{}/opam", Self::GITHUB_RAW, name, name, latest);
        let opam_content = ureq::get(&opam_url)
            .call()?
            .into_string()
            .map_err(|e| IndexError::Io(e))?;

        let meta = Self::parse_opam_file(&opam_content);

        Ok(PackageMeta {
            name: name.to_string(),
            version: latest.to_string(),
            description: meta.synopsis.or(meta.description),
            homepage: meta.homepage,
            repository: meta.dev_repo.map(|r| r.replace("git+", "")),
            license: meta.license,
            binaries: Vec::new(),
            keywords: Vec::new(),
            maintainers: meta.maintainer.into_iter().chain(meta.authors).collect(),
            published: None,
            downloads: None,
            archive_url: None,
            checksum: None,
            extra: Default::default(),
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let api_url = format!("{}/{}", Self::GITHUB_API, name);
        let response: serde_json::Value = ureq::get(&api_url)
            .set("User-Agent", "moss-packages/0.1")
            .set("Accept", "application/vnd.github.v3+json")
            .call()?
            .into_json()?;

        let mut versions: Vec<VersionMeta> = response
            .as_array()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?
            .iter()
            .filter_map(|entry| {
                let dir_name = entry["name"].as_str()?;
                let version = dir_name.strip_prefix(&format!("{}.", name))?;
                Some(VersionMeta {
                    version: version.to_string(),
                    released: None,
                    yanked: false,
                })
            })
            .collect();

        if versions.is_empty() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        // Sort descending
        versions.sort_by(|a, b| version_compare(&b.version, &a.version));

        Ok(versions)
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        // GitHub API code search
        let search_url = format!(
            "https://api.github.com/search/code?q={}+in:path+path:packages&per_page=50",
            urlencoding::encode(query)
        );

        let response: serde_json::Value = ureq::get(&search_url)
            .set("User-Agent", "moss-packages/0.1")
            .set("Accept", "application/vnd.github.v3+json")
            .call()?
            .into_json()?;

        let items = response["items"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing items".into()))?;

        // Extract unique package names from paths
        let mut seen = std::collections::HashSet::new();
        let packages: Vec<PackageMeta> = items
            .iter()
            .filter_map(|item| {
                let path = item["path"].as_str()?;
                // Path format: packages/name/name.version/opam
                let parts: Vec<&str> = path.split('/').collect();
                if parts.len() >= 2 && parts[0] == "packages" {
                    let name = parts[1];
                    if seen.insert(name.to_string()) {
                        return Some(PackageMeta {
                            name: name.to_string(),
                            version: "unknown".to_string(),
                            description: None,
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
                        });
                    }
                }
                None
            })
            .take(50)
            .collect();

        Ok(packages)
    }
}

/// Simple version comparison.
fn version_compare(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| -> Vec<u32> {
        s.split(|c: char| !c.is_ascii_digit())
            .filter_map(|p| p.parse().ok())
            .collect()
    };
    parse(a).cmp(&parse(b))
}
