//! Common parsing logic for Arch-based package APIs.
//!
//! Shared between Arch Linux, Artix, and other Arch derivatives.

use super::{IndexError, PackageMeta};
use crate::cache;
use flate2::read::GzDecoder;
use std::io::{Cursor, Read};
use std::time::Duration;

/// Cache TTL for AUR archive (1 hour).
const AUR_CACHE_TTL: Duration = Duration::from_secs(60 * 60);

/// Construct download URL for an official Arch package.
fn build_arch_download_url(pkg: &serde_json::Value) -> Option<String> {
    let repo = pkg["repo"].as_str()?;
    let arch = pkg["arch"].as_str()?;
    let filename = pkg["filename"].as_str()?;
    Some(format!(
        "https://mirror.archlinux.org/{}/os/{}/{}",
        repo, arch, filename
    ))
}

/// Parse a package from Arch-style official repo JSON response.
pub fn parse_official_package(pkg: &serde_json::Value, name: &str) -> Option<PackageMeta> {
    let mut extra = std::collections::HashMap::new();

    // Extract dependencies
    if let Some(deps) = pkg["depends"].as_array() {
        let parsed_deps: Vec<serde_json::Value> = deps
            .iter()
            .filter_map(|d| d.as_str())
            .map(|d| {
                // Strip version constraints: "libc6>=2.17" -> "libc6"
                let name = d
                    .split(|c| c == '>' || c == '<' || c == '=' || c == ':')
                    .next()
                    .unwrap_or(d);
                serde_json::Value::String(name.to_string())
            })
            .collect();
        extra.insert("depends".to_string(), serde_json::Value::Array(parsed_deps));
    }

    // Extract provides (virtual packages and shared libraries)
    if let Some(provides) = pkg["provides"].as_array() {
        let parsed_provides: Vec<serde_json::Value> = provides
            .iter()
            .filter_map(|p| p.as_str())
            .map(|p| {
                // Strip version constraints: "libfoo.so=1" -> "libfoo.so"
                let name = p
                    .split(|c| c == '>' || c == '<' || c == '=' || c == ':')
                    .next()
                    .unwrap_or(p);
                serde_json::Value::String(name.to_string())
            })
            .collect();
        if !parsed_provides.is_empty() {
            extra.insert(
                "provides".to_string(),
                serde_json::Value::Array(parsed_provides),
            );
        }
    }

    // Extract size
    if let Some(size) = pkg["compressed_size"].as_u64() {
        extra.insert("size".to_string(), serde_json::Value::Number(size.into()));
    }

    Some(PackageMeta {
        name: pkg["pkgname"].as_str().unwrap_or(name).to_string(),
        version: pkg["pkgver"].as_str().unwrap_or("unknown").to_string(),
        description: pkg["pkgdesc"].as_str().map(String::from),
        homepage: pkg["url"].as_str().map(String::from),
        repository: None,
        license: pkg["licenses"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|l| l.as_str())
            .map(String::from),
        binaries: Vec::new(),
        keywords: Vec::new(),
        maintainers: pkg["maintainers"]
            .as_array()
            .map(|m| {
                m.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        published: pkg["last_update"].as_str().map(String::from),
        downloads: None,
        archive_url: build_arch_download_url(pkg),
        checksum: None, // Arch uses .sig files for checksums
        extra,
    })
}

/// Parse a package from AUR-style JSON response.
pub fn parse_aur_package(pkg: &serde_json::Value, name: &str) -> Option<PackageMeta> {
    let mut extra = std::collections::HashMap::new();

    // Extract dependencies
    if let Some(deps) = pkg["Depends"].as_array() {
        let parsed_deps: Vec<serde_json::Value> = deps
            .iter()
            .filter_map(|d| d.as_str())
            .map(|d| {
                // Strip version constraints: "pacman>6.1" -> "pacman"
                let name = d
                    .split(|c| c == '>' || c == '<' || c == '=' || c == ':')
                    .next()
                    .unwrap_or(d);
                serde_json::Value::String(name.to_string())
            })
            .collect();
        extra.insert("depends".to_string(), serde_json::Value::Array(parsed_deps));
    }

    // Extract provides (virtual packages and shared libraries)
    if let Some(provides) = pkg["Provides"].as_array() {
        let parsed_provides: Vec<serde_json::Value> = provides
            .iter()
            .filter_map(|p| p.as_str())
            .map(|p| {
                // Strip version constraints
                let name = p
                    .split(|c| c == '>' || c == '<' || c == '=' || c == ':')
                    .next()
                    .unwrap_or(p);
                serde_json::Value::String(name.to_string())
            })
            .collect();
        if !parsed_provides.is_empty() {
            extra.insert(
                "provides".to_string(),
                serde_json::Value::Array(parsed_provides),
            );
        }
    }

    // Mark as AUR package
    extra.insert(
        "source".to_string(),
        serde_json::Value::String("aur".to_string()),
    );

    // Build download URL for source tarball
    let archive_url = pkg["URLPath"]
        .as_str()
        .map(|path| format!("https://aur.archlinux.org{}", path));

    Some(PackageMeta {
        name: pkg["Name"].as_str().unwrap_or(name).to_string(),
        version: pkg["Version"].as_str().unwrap_or("unknown").to_string(),
        description: pkg["Description"].as_str().map(String::from),
        homepage: pkg["URL"].as_str().map(String::from),
        repository: None,
        license: pkg["License"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|l| l.as_str())
            .map(String::from),
        binaries: Vec::new(),
        keywords: pkg["Keywords"]
            .as_array()
            .map(|k| {
                k.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        maintainers: pkg["Maintainer"]
            .as_str()
            .map(|m| vec![m.to_string()])
            .unwrap_or_default(),
        published: pkg["LastModified"].as_u64().map(|t| format!("{}", t)),
        downloads: pkg["NumVotes"].as_u64(),
        archive_url,
        checksum: None, // AUR packages are built from source
        extra,
    })
}

/// Fetch and parse from an Arch-style official API endpoint.
pub fn fetch_official(api_base: &str, name: &str) -> Result<PackageMeta, IndexError> {
    let url = format!("{}?name={}", api_base, name);
    let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

    let results = response["results"]
        .as_array()
        .ok_or_else(|| IndexError::Parse("missing results".into()))?;

    let pkg = results
        .first()
        .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

    parse_official_package(pkg, name).ok_or_else(|| IndexError::NotFound(name.to_string()))
}

/// Fetch and parse from an AUR-style API endpoint.
pub fn fetch_aur(api_base: &str, name: &str) -> Result<PackageMeta, IndexError> {
    let url = format!("{}?v=5&type=info&arg={}", api_base, name);
    let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

    let results = response["results"]
        .as_array()
        .ok_or_else(|| IndexError::Parse("missing results".into()))?;

    let pkg = results
        .first()
        .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

    parse_aur_package(pkg, name).ok_or_else(|| IndexError::NotFound(name.to_string()))
}

/// Search an Arch-style official API.
pub fn search_official(api_base: &str, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
    let url = format!("{}?q={}", api_base, query);
    let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

    let results = response["results"]
        .as_array()
        .ok_or_else(|| IndexError::Parse("missing results".into()))?;

    Ok(results
        .iter()
        .filter_map(|pkg| parse_official_package(pkg, ""))
        .collect())
}

/// Search an AUR-style API.
pub fn search_aur(api_base: &str, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
    let url = format!("{}?v=5&type=search&arg={}", api_base, query);
    let response: serde_json::Value = ureq::get(&url).call()?.into_json()?;

    let results = response["results"]
        .as_array()
        .ok_or_else(|| IndexError::Parse("missing results".into()))?;

    Ok(results
        .iter()
        .filter_map(|pkg| parse_aur_package(pkg, ""))
        .collect())
}

/// Fetch all AUR packages using the packages-meta-ext-v1.json.gz archive.
/// This is the recommended way to get all AUR packages instead of bulk API queries.
pub fn fetch_all_aur() -> Result<Vec<PackageMeta>, IndexError> {
    const AUR_ARCHIVE: &str = "https://aur.archlinux.org/packages-meta-ext-v1.json.gz";

    // Try cache first
    let (data, _was_cached) =
        cache::fetch_with_cache("pacman", "aur-packages-all", AUR_ARCHIVE, AUR_CACHE_TTL)
            .map_err(|e| IndexError::Network(e))?;

    // Decompress gzipped data
    let mut decoder = GzDecoder::new(Cursor::new(data));
    let mut json_data = String::new();
    decoder
        .read_to_string(&mut json_data)
        .map_err(|e| IndexError::Parse(format!("gzip decode error: {}", e)))?;

    let packages: Vec<serde_json::Value> = serde_json::from_str(&json_data)
        .map_err(|e| IndexError::Parse(format!("JSON parse error: {}", e)))?;

    Ok(packages
        .iter()
        .filter_map(|pkg| parse_aur_package(pkg, ""))
        .collect())
}
