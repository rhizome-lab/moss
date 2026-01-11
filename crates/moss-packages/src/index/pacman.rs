//! Pacman package index fetcher (Arch Linux).
//!
//! Fetches package metadata from Arch Linux repositories and AUR.

use super::arch_common;
use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// Pacman package index fetcher.
pub struct Pacman;

impl Pacman {
    /// AUR RPC endpoint.
    const AUR_RPC: &'static str = "https://aur.archlinux.org/rpc/";

    /// Official package search API.
    const ARCH_API: &'static str = "https://archlinux.org/packages/search/json/";
}

impl PackageIndex for Pacman {
    fn ecosystem(&self) -> &'static str {
        "pacman"
    }

    fn display_name(&self) -> &'static str {
        "Pacman (Arch Linux)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Try official repos first, then AUR
        arch_common::fetch_official(Self::ARCH_API, name)
            .or_else(|_| arch_common::fetch_aur(Self::AUR_RPC, name))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // Arch doesn't maintain version history easily accessible via API
        // Return current version only
        let pkg = self.fetch(name)?;
        Ok(vec![VersionMeta {
            version: pkg.version,
            released: None,
            yanked: false,
        }])
    }

    fn supports_fetch_all(&self) -> bool {
        true
    }

    fn fetch_all(&self) -> Result<Vec<PackageMeta>, IndexError> {
        // Fetch all AUR packages using the compressed archive
        arch_common::fetch_all_aur()
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        // Search official repos
        let mut packages = arch_common::search_official(Self::ARCH_API, query)?;

        // Also search AUR
        if let Ok(aur_packages) = arch_common::search_aur(Self::AUR_RPC, query) {
            packages.extend(aur_packages);
        }

        Ok(packages)
    }
}
