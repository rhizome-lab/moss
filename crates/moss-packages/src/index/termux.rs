//! Termux package index fetcher (Android terminal).
//!
//! Fetches package metadata from the termux-packages GitHub repository.
//!
//! ## API Strategy
//! - **fetch**: `github.com/termux/termux-packages/.../build.sh` - GitHub raw files
//! - **fetch_versions**: Same, single version per package
//! - **search**: Filters GitHub directory listing via API
//! - **fetch_all**: GitHub API to list all package directories

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// Termux package index fetcher.
pub struct Termux;

impl Termux {
    /// Termux packages GitHub raw URL template.
    const BUILD_SH_URL: &'static str =
        "https://raw.githubusercontent.com/termux/termux-packages/master/packages/{}/build.sh";

    /// Parse a build.sh file and extract package metadata.
    fn parse_build_sh(name: &str, content: &str) -> PackageMeta {
        let extract_var = |var: &str| -> Option<String> {
            for line in content.lines() {
                let line = line.trim();
                if let Some(value) = line.strip_prefix(&format!("{}=", var)) {
                    // Remove quotes
                    return Some(
                        value
                            .trim_matches('"')
                            .trim_matches('\'')
                            .trim()
                            .to_string(),
                    );
                }
            }
            None
        };

        PackageMeta {
            name: name.to_string(),
            version: extract_var("TERMUX_PKG_VERSION").unwrap_or_else(|| "unknown".to_string()),
            description: extract_var("TERMUX_PKG_DESCRIPTION"),
            homepage: extract_var("TERMUX_PKG_HOMEPAGE"),
            repository: extract_var("TERMUX_PKG_SRCURL"),
            license: extract_var("TERMUX_PKG_LICENSE"),
            binaries: Vec::new(),
            keywords: Vec::new(),
            maintainers: extract_var("TERMUX_PKG_MAINTAINER")
                .map(|m| vec![m])
                .unwrap_or_default(),
            published: None,
            downloads: None,
            archive_url: None,
            checksum: None,
            extra: Default::default(),
        }
    }
}

impl PackageIndex for Termux {
    fn ecosystem(&self) -> &'static str {
        "termux"
    }

    fn display_name(&self) -> &'static str {
        "Termux (Android)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        let url = Self::BUILD_SH_URL.replace("{}", name);
        let response = ureq::get(&url)
            .call()
            .map_err(|_| IndexError::NotFound(name.to_string()))?;

        let content = response
            .into_string()
            .map_err(|e| IndexError::Parse(e.to_string()))?;

        Ok(Self::parse_build_sh(name, &content))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // Termux only keeps the latest version in the repo
        let meta = self.fetch(name)?;
        Ok(vec![VersionMeta {
            version: meta.version,
            released: None,
            yanked: false,
        }])
    }

    fn search(&self, _query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        // Search requires listing the packages directory via GitHub API
        // which has rate limits, so we don't implement bulk search
        Err(IndexError::Network(
            "Termux search requires GitHub API (rate limited)".into(),
        ))
    }
}
