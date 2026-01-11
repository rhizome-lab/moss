//! Package index ingestion for cross-platform package mapping.
//!
//! This module provides fetchers that pull metadata from package manager indices
//! (apt Sources, brew API, crates.io, etc.) to extract package information.
//!
//! Unlike the `ecosystem` feature which is project-focused (dependencies of a project),
//! this is registry-focused (what packages exist and what's their metadata).

mod types;

#[cfg(test)]
mod tests;

// Distro package managers
pub mod apk;
pub mod apt;
mod arch_common;
pub mod artix;
pub mod cachyos;
pub mod chaotic_aur;
pub mod copr;
pub mod dnf;
pub mod endeavouros;
pub mod gentoo;
pub mod guix;
pub mod manjaro;
pub mod nix;
pub mod pacman;
pub mod slackware;

// Windows package managers
pub mod choco;
pub mod msys2;
pub mod scoop;
pub mod winget;

// macOS
pub mod brew;
pub mod homebrew_casks;
pub mod macports;

// Cross-platform app stores
pub mod flatpak;
pub mod snap;

// Containers
pub mod docker;

// Mobile
pub mod fdroid;
pub mod termux;

// Language package managers
pub mod bioconductor;
pub mod cargo;
pub mod clojars;
pub mod composer;
pub mod conan;
pub mod conda;
pub mod cran;
pub mod ctan;
pub mod deno;
pub mod dub;
pub mod gem;
pub mod go;
pub mod hackage;
pub mod hex;
pub mod hunter;
pub mod jsr;
pub mod julia;
pub mod luarocks;
pub mod maven;
pub mod metacpan;
pub mod nimble;
pub mod npm;
pub mod nuget;
pub mod opam;
pub mod pip;
pub mod pub_dev;
pub mod racket;
pub mod vcpkg;

pub use types::{IndexError, PackageIndex, PackageMeta, VersionMeta};

use std::sync::OnceLock;

static INDEX_REGISTRY: OnceLock<Vec<&'static dyn PackageIndex>> = OnceLock::new();

fn init_builtin() -> Vec<&'static dyn PackageIndex> {
    vec![
        // Distro
        &apk::Apk,
        &apt::Apt,
        &artix::Artix,
        &cachyos::CachyOs,
        &chaotic_aur::ChaoticAur,
        &copr::Copr,
        &dnf::Dnf,
        &endeavouros::EndeavourOs,
        &gentoo::Gentoo,
        &guix::Guix,
        &manjaro::Manjaro,
        &nix::Nix,
        &pacman::Pacman,
        &slackware::Slackware,
        // Windows
        &choco::Choco,
        &msys2::Msys2,
        &scoop::Scoop,
        &winget::Winget,
        // macOS
        &brew::Brew,
        &homebrew_casks::HomebrewCasks,
        &macports::MacPorts,
        // Cross-platform
        &flatpak::Flatpak,
        &snap::Snap,
        // Containers
        &docker::Docker,
        // Mobile
        &fdroid::FDroid,
        &termux::Termux,
        // Language
        &bioconductor::Bioconductor,
        &vcpkg::Vcpkg,
        &clojars::Clojars,
        &cargo::CargoIndex,
        &ctan::Ctan,
        &composer::Composer,
        &conan::Conan,
        &conda::Conda,
        &cran::Cran,
        &deno::Deno,
        &dub::Dub,
        &gem::Gem,
        &go::Go,
        &hackage::Hackage,
        &hex::Hex,
        &hunter::Hunter,
        &jsr::Jsr,
        &julia::Julia,
        &luarocks::LuaRocks,
        &maven::Maven,
        &metacpan::MetaCpan,
        &nimble::Nimble,
        &npm::NpmIndex,
        &nuget::Nuget,
        &opam::Opam,
        &pip::PipIndex,
        &pub_dev::Pub,
        &racket::Racket,
    ]
}

/// Get a package index by ecosystem name.
pub fn get_index(name: &str) -> Option<&'static dyn PackageIndex> {
    let registry = INDEX_REGISTRY.get_or_init(init_builtin);
    registry.iter().find(|idx| idx.ecosystem() == name).copied()
}

/// List all available package index ecosystem names.
pub fn list_indices() -> Vec<&'static str> {
    let registry = INDEX_REGISTRY.get_or_init(init_builtin);
    registry.iter().map(|idx| idx.ecosystem()).collect()
}

/// Get all registered package indices.
pub fn all_indices() -> Vec<&'static dyn PackageIndex> {
    INDEX_REGISTRY.get_or_init(init_builtin).clone()
}
