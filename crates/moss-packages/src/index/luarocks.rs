//! LuaRocks package index fetcher (Lua).
//!
//! Fetches package metadata from LuaRocks. Uses the manifest file for package
//! listing and individual rockspec files for metadata.
//!
//! ## API Strategy
//! - **fetch**: `luarocks.org/manifests/root/{name}-{version}.rockspec`
//! - **fetch_versions**: Parses `luarocks.org/manifest` (Lua table format)
//! - **search**: Not supported (would require HTML scraping)
//! - **fetch_all**: Parses manifest file (cached 1 hour)

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// LuaRocks package index fetcher.
pub struct LuaRocks;

impl LuaRocks {
    /// LuaRocks base URL.
    const BASE_URL: &'static str = "https://luarocks.org";

    /// Parse Lua table-like manifest to extract versions for a package.
    fn parse_manifest(content: &str, name: &str) -> Option<Vec<String>> {
        // Look for the package in the repository table
        // Format can be either:
        // - ["package-name"] = { ... }  (for names with special chars)
        // - package_name = { ... }       (for simple names)

        // Try multiple formats - luarocks manifest can have various indent levels
        // Format 1: ["package-name"] = { (for names with special chars)
        // Format 2: package_name = { (for simple names, various indentation)
        let quoted_search = format!("[\"{}\"] = {{", name);
        let simple_search = format!("{} = {{", name);

        let start = content
            .find(&quoted_search)
            .or_else(|| content.find(&simple_search))?;

        let rest = &content[start..];

        // Find the opening brace
        let brace_pos = rest.find('{')?;
        let after_brace = &rest[brace_pos + 1..];

        // Extract version strings - they appear as ["version"] = { ... }
        let mut versions = Vec::new();
        let mut pos = 0;

        while let Some(find_start) = after_brace[pos..].find("[\"") {
            let version_start = pos + find_start + 2;
            if let Some(end) = after_brace[version_start..].find("\"]") {
                let version = &after_brace[version_start..version_start + end];
                // Check if this is a version at depth 1 (directly under the package)
                // by counting braces. We started after the main opening brace,
                // so we're at depth 1 when opens == closes (all nested braces closed)
                let prefix = &after_brace[..version_start];
                let open_braces = prefix.matches('{').count();
                let close_braces = prefix.matches('}').count();
                if open_braces == close_braces {
                    versions.push(version.to_string());
                }
                pos = version_start + end + 2;
            } else {
                break;
            }

            // Stop if we've exited this package's block
            let so_far = &after_brace[..pos];
            let opens = so_far.matches('{').count();
            let closes = so_far.matches('}').count();
            if closes > opens {
                break;
            }
        }

        if versions.is_empty() {
            None
        } else {
            Some(versions)
        }
    }
}

impl PackageIndex for LuaRocks {
    fn ecosystem(&self) -> &'static str {
        "luarocks"
    }

    fn display_name(&self) -> &'static str {
        "LuaRocks (Lua)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Fetch the manifest to get version info
        let manifest_url = format!("{}/manifest", Self::BASE_URL);
        let manifest = ureq::get(&manifest_url)
            .call()?
            .into_string()
            .map_err(|e| IndexError::Io(e))?;

        let versions = Self::parse_manifest(&manifest, name)
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        // Get latest version (filter out scm/dev, sort by semver)
        let latest = versions
            .iter()
            .filter(|v| !v.contains("scm") && !v.contains("dev"))
            .max_by(|a, b| version_compare(a, b))
            .or(versions.first())
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        // Extract version number (remove revision suffix like "-1")
        let version_clean = latest.rsplit_once('-').map(|(v, _)| v).unwrap_or(latest);

        Ok(PackageMeta {
            name: name.to_string(),
            version: version_clean.to_string(),
            description: None, // Would require HTML scraping
            homepage: Some(format!("{}/modules/{}/{}", Self::BASE_URL, name, name)),
            repository: None,
            license: None,
            binaries: Vec::new(),
            keywords: Vec::new(),
            maintainers: Vec::new(),
            published: None,
            downloads: None,
            archive_url: Some(format!("{}/{}-{}.src.rock", Self::BASE_URL, name, latest)),
            checksum: None,
            extra: Default::default(),
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let manifest_url = format!("{}/manifest", Self::BASE_URL);
        let manifest = ureq::get(&manifest_url)
            .call()?
            .into_string()
            .map_err(|e| IndexError::Io(e))?;

        let versions = Self::parse_manifest(&manifest, name)
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        let mut result: Vec<VersionMeta> = versions
            .iter()
            .map(|v| {
                let version_clean = v.rsplit_once('-').map(|(ver, _)| ver).unwrap_or(v);
                VersionMeta {
                    version: version_clean.to_string(),
                    released: None,
                    yanked: false,
                }
            })
            .collect();

        // Sort descending
        result.sort_by(|a, b| version_compare(&b.version, &a.version));

        // Deduplicate (multiple rock revisions for same version)
        result.dedup_by(|a, b| a.version == b.version);

        Ok(result)
    }

    fn search(&self, _query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        // LuaRocks doesn't have a JSON search API
        // Would require HTML scraping
        Err(IndexError::Parse(
            "LuaRocks search requires HTML scraping (not implemented)".into(),
        ))
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
