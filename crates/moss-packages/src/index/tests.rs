//! Network tests for package index fetchers.
//!
//! These tests hit real APIs and are disabled by default.
//! Run with: cargo test -p rhizome-moss-packages --features test-network

#![cfg(feature = "test-network")]

use super::*;

// Helper to run a basic fetch test
fn test_fetch(index: &dyn PackageIndex, package: &str) {
    let result = index.fetch(package);
    assert!(
        result.is_ok(),
        "{} fetch({}) failed: {:?}",
        index.ecosystem(),
        package,
        result.err()
    );
    let pkg = result.unwrap();
    assert!(
        !pkg.name.is_empty(),
        "{}: name should not be empty",
        index.ecosystem()
    );
    assert!(
        !pkg.version.is_empty(),
        "{}: version should not be empty",
        index.ecosystem()
    );
    println!(
        "{}: {} v{} - {:?}",
        index.ecosystem(),
        pkg.name,
        pkg.version,
        pkg.repository
    );
}

fn test_versions(index: &dyn PackageIndex, package: &str) {
    let result = index.fetch_versions(package);
    assert!(
        result.is_ok(),
        "{} fetch_versions({}) failed: {:?}",
        index.ecosystem(),
        package,
        result.err()
    );
    let versions = result.unwrap();
    assert!(
        !versions.is_empty(),
        "{}: should have at least one version",
        index.ecosystem()
    );
    println!(
        "{}: {} has {} versions",
        index.ecosystem(),
        package,
        versions.len()
    );
}

fn test_search(index: &dyn PackageIndex, query: &str) {
    let result = index.search(query);
    // Search may not be implemented for all indices
    if let Ok(results) = result {
        println!(
            "{}: search('{}') returned {} results",
            index.ecosystem(),
            query,
            results.len()
        );
    } else {
        println!(
            "{}: search not implemented or failed: {:?}",
            index.ecosystem(),
            result.err()
        );
    }
}

fn test_fetch_all(index: &dyn PackageIndex) {
    let result = index.fetch_all();
    if let Ok(packages) = result {
        println!(
            "{}: fetch_all() returned {} packages",
            index.ecosystem(),
            packages.len()
        );
        assert!(
            !packages.is_empty(),
            "{}: fetch_all should return packages",
            index.ecosystem()
        );
    } else {
        println!(
            "{}: fetch_all not implemented: {:?}",
            index.ecosystem(),
            result.err()
        );
    }
}

// =============================================================================
// Distro package managers
// =============================================================================

#[test]
fn test_apt() {
    let index = apt::Apt;
    // apt uses source package names, "rust-ripgrep" not "ripgrep"
    test_fetch(&index, "curl");
    test_versions(&index, "curl");
    test_search(&index, "curl");
}

#[test]
fn test_apt_fetch_all() {
    let index = apt::Apt;
    test_fetch_all(&index);
}

#[test]
fn test_apt_enhanced_metadata() {
    let index = apt::Apt;
    let packages = index.fetch_all().unwrap();

    // Find curl package to verify enhanced metadata
    if let Some(curl) = packages.iter().find(|p| p.name == "curl") {
        println!("Package: {}", curl.name);
        println!("Version: {}", curl.version);
        println!("Archive URL: {:?}", curl.archive_url);
        println!("Checksum: {:?}", curl.checksum);
        println!("Dependencies: {:?}", curl.extra.get("depends"));
        println!("Size: {:?}", curl.extra.get("size"));

        // Verify enhanced fields are present
        assert!(curl.archive_url.is_some(), "archive_url should be set");
        assert!(curl.checksum.is_some(), "checksum should be set");
        assert!(
            curl.extra.contains_key("depends"),
            "depends should be in extra"
        );
    } else {
        panic!("curl package not found in apt index");
    }
}

#[test]
fn test_pacman() {
    let index = pacman::Pacman;
    test_fetch(&index, "ripgrep");
    test_versions(&index, "ripgrep");
    test_search(&index, "grep");
}

#[test]
fn test_pacman_enhanced_metadata() {
    let index = pacman::Pacman;

    // Test official repo package
    let curl = index.fetch("curl").unwrap();
    println!("Package: {}", curl.name);
    println!("Version: {}", curl.version);
    println!("Archive URL: {:?}", curl.archive_url);
    println!("Dependencies: {:?}", curl.extra.get("depends"));
    println!("Size: {:?}", curl.extra.get("size"));

    assert!(
        curl.archive_url.is_some(),
        "archive_url should be set for official packages"
    );
    assert!(
        curl.extra.contains_key("depends"),
        "depends should be in extra"
    );

    // Test AUR package
    let yay = index.fetch("yay").unwrap();
    println!("\nAUR Package: {}", yay.name);
    println!("Archive URL: {:?}", yay.archive_url);
    println!("Dependencies: {:?}", yay.extra.get("depends"));
    println!("Source: {:?}", yay.extra.get("source"));

    assert!(
        yay.archive_url.is_some(),
        "archive_url should be set for AUR packages"
    );
    assert_eq!(yay.extra.get("source"), Some(&serde_json::json!("aur")));
}

#[test]
fn test_artix() {
    let index = artix::Artix;
    test_fetch(&index, "ripgrep");
    test_versions(&index, "ripgrep");
    test_search(&index, "grep");
}

#[test]
fn test_nix() {
    let index = nix::Nix;
    test_fetch(&index, "ripgrep");
    test_versions(&index, "ripgrep");
    test_search(&index, "grep");
}

#[test]
fn test_gentoo() {
    let index = gentoo::Gentoo;
    test_fetch(&index, "sys-apps/ripgrep");
    test_versions(&index, "sys-apps/ripgrep");
    // Note: search returns HTML not JSON, so it's not supported
}

#[test]
#[ignore = "Slow: downloads full Guix package list (~25MB decompressed)"]
fn test_guix() {
    let index = guix::Guix;
    test_fetch(&index, "ripgrep");
    test_versions(&index, "ripgrep");
    test_search(&index, "grep");
}

#[test]
#[ignore = "SlackBuilds uses GitHub raw files, may be slow"]
fn test_slackware() {
    let index = slackware::Slackware;
    test_fetch(&index, "ripgrep");
    test_versions(&index, "ripgrep");
}

#[test]
fn test_cachyos() {
    // CachyOS uses Arch repos + AUR
    let index = cachyos::CachyOs;
    test_fetch(&index, "ripgrep");
    test_search(&index, "grep");
}

#[test]
fn test_endeavouros() {
    // EndeavourOS uses Arch repos + AUR
    let index = endeavouros::EndeavourOs;
    test_fetch(&index, "ripgrep");
    test_search(&index, "grep");
}

#[test]
fn test_manjaro() {
    // Manjaro uses its own repos + AUR
    let index = manjaro::Manjaro;
    test_fetch(&index, "firefox");
    test_versions(&index, "firefox");
    test_search(&index, "browser");
}

#[test]
fn test_dnf() {
    let index = dnf::Dnf;
    test_fetch(&index, "curl");
    test_versions(&index, "curl");
    // Search API is currently broken (fcomm_connector redirects to 404)
    // test_search(&index, "curl");
}

#[test]
fn test_copr() {
    let index = copr::Copr;
    // Copr uses owner/project format
    test_search(&index, "vim");
}

#[test]
fn test_chaotic_aur() {
    let index = chaotic_aur::ChaoticAur;
    test_fetch(&index, "neovim-git");
    test_search(&index, "firefox");
}

#[test]
fn test_dnf_enhanced_metadata() {
    let index = dnf::Dnf;
    let curl = index.fetch("curl").unwrap();
    println!("Package: {}", curl.name);
    println!("Version: {}", curl.version);
    println!("Dependencies: {:?}", curl.extra.get("depends"));
    println!("Arch: {:?}", curl.extra.get("arch"));

    // Verify enhanced fields are present (libcurl is a common dep for curl)
    assert!(curl.extra.contains_key("arch"), "arch should be in extra");
}

#[test]
fn test_apk() {
    let index = apk::Apk;
    test_fetch(&index, "curl");
    test_versions(&index, "curl");
    test_search(&index, "curl");
}

#[test]
fn test_apk_enhanced_metadata() {
    let index = apk::Apk;
    let curl = index.fetch("curl").unwrap();
    println!("Package: {}", curl.name);
    println!("Version: {}", curl.version);
    println!("Archive URL: {:?}", curl.archive_url);
    println!("Checksum: {:?}", curl.checksum);
    println!("Dependencies: {:?}", curl.extra.get("depends"));
    println!("Size: {:?}", curl.extra.get("size"));

    assert!(curl.archive_url.is_some(), "archive_url should be set");
    assert!(curl.checksum.is_some(), "checksum should be set");
}

#[test]
fn test_apk_fetch_all() {
    let index = apk::Apk;
    let packages = index.fetch_all().unwrap();
    println!("apk: fetch_all() returned {} packages", packages.len());
    assert!(!packages.is_empty(), "fetch_all should return packages");
}

// =============================================================================
// Windows package managers
// =============================================================================

#[test]
fn test_winget() {
    let index = winget::Winget;
    // winget.run API may not have all packages, use a common one
    test_fetch(&index, "Microsoft.VisualStudioCode");
    // Skip version test as API structure varies
    test_search(&index, "vscode");
}

#[test]
fn test_scoop() {
    let index = scoop::Scoop;
    test_fetch(&index, "git");
    test_versions(&index, "git");
    test_search(&index, "git");
}

#[test]
fn test_choco() {
    let index = choco::Choco;
    test_fetch(&index, "git");
    test_versions(&index, "git");
    test_search(&index, "git");
}

#[test]
fn test_msys2() {
    let index = msys2::Msys2;
    test_fetch(&index, "git");
    test_versions(&index, "git");
    test_search(&index, "git");
}

// =============================================================================
// macOS
// =============================================================================

#[test]
fn test_brew() {
    let index = brew::Brew;
    test_fetch(&index, "ripgrep");
    test_versions(&index, "ripgrep");
    test_search(&index, "grep");
}

#[test]
fn test_brew_fetch_all() {
    let index = brew::Brew;
    test_fetch_all(&index);
}

#[test]
fn test_macports() {
    let index = macports::MacPorts;
    test_fetch(&index, "git");
    test_versions(&index, "git");
    test_search(&index, "git");
}

// =============================================================================
// Cross-platform app stores
// =============================================================================

#[test]
fn test_snap() {
    let index = snap::Snap;
    test_fetch(&index, "firefox");
    test_versions(&index, "firefox");
    test_search(&index, "browser");
}

// =============================================================================
// Containers
// =============================================================================

#[test]
fn test_docker() {
    let index = docker::Docker;
    test_fetch(&index, "nginx");
    test_versions(&index, "nginx");
    test_search(&index, "nginx");
}

// =============================================================================
// Mobile
// =============================================================================

#[test]
fn test_fdroid() {
    let index = fdroid::FDroid;
    test_fetch(&index, "org.fdroid.fdroid");
    test_versions(&index, "org.fdroid.fdroid");
    test_search(&index, "browser");
}

#[test]
fn test_termux() {
    let index = termux::Termux;
    test_fetch(&index, "bash");
    test_versions(&index, "bash");
    // Note: search not implemented (requires GitHub API)
}

// =============================================================================
// Language package managers
// =============================================================================

#[test]
fn test_vcpkg() {
    let index = vcpkg::Vcpkg;
    test_fetch(&index, "zlib");
    test_versions(&index, "zlib");
    test_search(&index, "json");
}

#[test]
fn test_hunter() {
    let index = hunter::Hunter;
    test_fetch(&index, "Boost");
    test_versions(&index, "Boost");
    test_search(&index, "curl");
}

#[test]
fn test_vcpkg_fetch_all() {
    let index = vcpkg::Vcpkg;
    test_fetch_all(&index);
}

#[test]
fn test_clojars() {
    let index = clojars::Clojars;
    test_fetch(&index, "ring");
    test_versions(&index, "ring");
    test_search(&index, "ring");
}

#[test]
fn test_cargo() {
    let index = cargo::CargoIndex;
    test_fetch(&index, "serde");
    test_versions(&index, "serde");
    test_search(&index, "json");
}

#[test]
fn test_npm() {
    let index = npm::NpmIndex;
    test_fetch(&index, "typescript");
    test_versions(&index, "typescript");
    test_search(&index, "react");
}

#[test]
fn test_pip() {
    let index = pip::PipIndex;
    test_fetch(&index, "requests");
    test_versions(&index, "requests");
    // PyPI search not implemented via API
}

#[test]
fn test_deno() {
    let index = deno::Deno;
    test_fetch(&index, "oak");
    test_versions(&index, "oak");
    test_search(&index, "http");
}

#[test]
#[ignore = "Slow: paginates through entire deno.land/x index"]
fn test_deno_fetch_all() {
    let index = deno::Deno;
    test_fetch_all(&index);
}

#[test]
fn test_jsr() {
    let index = jsr::Jsr;
    test_fetch(&index, "@std/path");
    test_versions(&index, "@std/path");
    test_search(&index, "path");
}

#[test]
fn test_hex() {
    let index = hex::Hex;
    test_fetch(&index, "phoenix");
    test_versions(&index, "phoenix");
    test_search(&index, "web");
}

#[test]
fn test_maven() {
    let index = maven::Maven;
    test_fetch(&index, "com.google.guava:guava");
    test_versions(&index, "com.google.guava:guava");
    test_search(&index, "guava");
}

#[test]
fn test_nuget() {
    let index = nuget::Nuget;
    test_fetch(&index, "Newtonsoft.Json");
    test_versions(&index, "Newtonsoft.Json");
    test_search(&index, "json");
}

#[test]
fn test_gem() {
    let index = gem::Gem;
    test_fetch(&index, "rails");
    test_versions(&index, "rails");
    test_search(&index, "web");
}

#[test]
fn test_go() {
    let index = go::Go;
    test_fetch(&index, "github.com/gin-gonic/gin");
    test_versions(&index, "github.com/gin-gonic/gin");
    // Go search not implemented via API
}

#[test]
fn test_composer() {
    let index = composer::Composer;
    test_fetch(&index, "laravel/framework");
    test_versions(&index, "laravel/framework");
    test_search(&index, "laravel");
}

#[test]
fn test_conan() {
    let index = conan::Conan;
    test_fetch(&index, "zlib");
    test_versions(&index, "zlib");
    test_search(&index, "boost");
}

#[test]
fn test_hackage() {
    let index = hackage::Hackage;
    test_fetch(&index, "aeson");
    test_versions(&index, "aeson");
    test_search(&index, "json");
}

#[test]
fn test_luarocks() {
    let index = luarocks::LuaRocks;
    test_fetch(&index, "luasocket");
    test_versions(&index, "luasocket");
    // Note: search not implemented (requires HTML scraping)
}

#[test]
fn test_metacpan() {
    let index = metacpan::MetaCpan;
    test_fetch(&index, "Moose");
    test_versions(&index, "Moose");
    test_search(&index, "object");
}

#[test]
fn test_pub() {
    let index = pub_dev::Pub;
    test_fetch(&index, "http");
    test_versions(&index, "http");
    test_search(&index, "http");
}

#[test]
fn test_opam() {
    let index = opam::Opam;
    test_fetch(&index, "dune");
    test_versions(&index, "dune");
    test_search(&index, "build");
}

#[test]
fn test_cran() {
    let index = cran::Cran;
    test_fetch(&index, "ggplot2");
    test_versions(&index, "ggplot2");
    test_search(&index, "plot");
}

#[test]
fn test_bioconductor() {
    let index = bioconductor::Bioconductor;
    test_fetch(&index, "BiocManager");
    test_versions(&index, "BiocManager");
    test_search(&index, "genomic");
}

#[test]
fn test_homebrew_casks() {
    let index = homebrew_casks::HomebrewCasks;
    test_fetch(&index, "visual-studio-code");
    test_versions(&index, "visual-studio-code");
    test_search(&index, "vscode");
}

#[test]
#[ignore = "Slow: downloads ~50MB conda repodata"]
fn test_conda() {
    let index = conda::Conda;
    test_fetch(&index, "python");
    test_versions(&index, "python");
    test_search(&index, "numpy");
}

#[test]
fn test_flatpak() {
    let index = flatpak::Flatpak;
    test_fetch(&index, "org.mozilla.firefox");
    test_versions(&index, "org.mozilla.firefox");
    test_search(&index, "firefox");
}

#[test]
fn test_nimble() {
    let index = nimble::Nimble;
    test_fetch(&index, "jester");
    test_versions(&index, "jester");
    test_search(&index, "http");
}

#[test]
fn test_julia() {
    let index = julia::Julia;
    test_fetch(&index, "DataFrames");
    test_versions(&index, "DataFrames");
    // Note: search not implemented (requires full registry download)
}

#[test]
fn test_dub() {
    let index = dub::Dub;
    test_fetch(&index, "vibe-d");
    test_versions(&index, "vibe-d");
    test_search(&index, "http");
}

#[test]
fn test_ctan() {
    let index = ctan::Ctan;
    test_fetch(&index, "pgf");
    test_versions(&index, "pgf");
    test_search(&index, "tikz");
}

#[test]
fn test_racket() {
    let index = racket::Racket;
    test_fetch(&index, "racket-doc");
    test_versions(&index, "racket-doc");
    test_search(&index, "web");
}

// =============================================================================
// Registry tests
// =============================================================================

#[test]
#[ignore = "Slow: downloads entire AUR package archive (~30MB)"]
fn test_pacman_fetch_all() {
    let index = pacman::Pacman;
    test_fetch_all(&index);
}

#[test]
fn test_list_indices() {
    let indices = list_indices();
    assert!(
        indices.len() >= 58,
        "should have at least 58 indices, got {}",
        indices.len()
    );
    println!("Available indices: {:?}", indices);
}

#[test]
fn test_get_index() {
    assert!(get_index("brew").is_some());
    assert!(get_index("cargo").is_some());
    assert!(get_index("nonexistent").is_none());
}
