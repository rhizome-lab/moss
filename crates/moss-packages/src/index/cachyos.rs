//! CachyOS package index fetcher.
//!
//! CachyOS is an Arch-based distro with its own repositories.
//! Uses the same package format as Arch but with additional CachyOS repos.

use super::arch_common;
use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// CachyOS package index fetcher.
pub struct CachyOs;

impl CachyOs {
    /// CachyOS uses Arch's API format - they don't have a separate package API.
    /// Fall back to Arch repos + AUR.
    const ARCH_API: &'static str = "https://archlinux.org/packages/search/json/";
    const AUR_RPC: &'static str = "https://aur.archlinux.org/rpc/";
}

impl PackageIndex for CachyOs {
    fn ecosystem(&self) -> &'static str {
        "cachyos"
    }

    fn display_name(&self) -> &'static str {
        "CachyOS"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // CachyOS uses Arch repos + AUR + their own repos (not exposed via API)
        arch_common::fetch_official(Self::ARCH_API, name)
            .or_else(|_| arch_common::fetch_aur(Self::AUR_RPC, name))
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
        let mut packages = arch_common::search_official(Self::ARCH_API, query)?;

        if let Ok(aur_packages) = arch_common::search_aur(Self::AUR_RPC, query) {
            packages.extend(aur_packages);
        }

        Ok(packages)
    }
}
