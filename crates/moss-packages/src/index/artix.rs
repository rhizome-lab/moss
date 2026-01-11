//! Artix Linux package index fetcher.
//!
//! Fetches package metadata from Artix Linux repositories.
//! Artix is an Arch-based distro without systemd.

use super::arch_common;
use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// Artix Linux package index fetcher.
pub struct Artix;

impl Artix {
    /// Artix package search API.
    const ARTIX_API: &'static str = "https://packages.artixlinux.org/packages/search/json/";

    /// Arch AUR (Artix users can also use AUR packages).
    const AUR_RPC: &'static str = "https://aur.archlinux.org/rpc/";
}

impl PackageIndex for Artix {
    fn ecosystem(&self) -> &'static str {
        "artix"
    }

    fn display_name(&self) -> &'static str {
        "Artix Linux"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Try Artix repos first, then fall back to AUR
        arch_common::fetch_official(Self::ARTIX_API, name)
            .or_else(|_| arch_common::fetch_aur(Self::AUR_RPC, name))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // Artix doesn't maintain version history via API
        let pkg = self.fetch(name)?;
        Ok(vec![VersionMeta {
            version: pkg.version,
            released: None,
            yanked: false,
        }])
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        // Search Artix repos
        let mut packages = arch_common::search_official(Self::ARTIX_API, query).unwrap_or_default();

        // Also search AUR
        if let Ok(aur_packages) = arch_common::search_aur(Self::AUR_RPC, query) {
            packages.extend(aur_packages);
        }

        Ok(packages)
    }
}
