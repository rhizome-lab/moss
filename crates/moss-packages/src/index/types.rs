//! Core types for package index ingestion.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Metadata about a package from an index.
///
/// This is the raw metadata extracted from a package manager's index,
/// before any correlation with packages from other ecosystems.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PackageMeta {
    /// Package name in this ecosystem.
    pub name: String,
    /// Latest version string.
    pub version: String,
    /// Package description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Homepage URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    /// Source repository URL (GitHub, GitLab, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    /// License identifier (SPDX when available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Binary/executable names provided by this package.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub binaries: Vec<String>,
    /// Keywords/tags for the package.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    /// Maintainers/authors.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub maintainers: Vec<String>,
    /// When this version was published/released.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published: Option<String>,
    /// Download/popularity count (semantics vary by ecosystem).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downloads: Option<u64>,
    /// Archive/download URL for this version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archive_url: Option<String>,
    /// Checksum of the archive (format: "algo:hash", e.g., "sha256:abc123").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
    /// Ecosystem-specific metadata that doesn't fit normalized fields.
    #[serde(default, skip_serializing_if = "HashMap::is_empty", flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Version information for a package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionMeta {
    /// Version string.
    pub version: String,
    /// Release date if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub released: Option<String>,
    /// Whether this version is yanked/deprecated.
    #[serde(default)]
    pub yanked: bool,
}

/// Errors that can occur during index operations.
#[derive(Debug)]
pub enum IndexError {
    /// Network request failed.
    Network(String),
    /// Failed to parse response.
    Parse(String),
    /// Package not found.
    NotFound(String),
    /// IO error.
    Io(std::io::Error),
    /// Decompression error.
    Decompress(String),
}

impl std::fmt::Display for IndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(msg) => write!(f, "network error: {msg}"),
            Self::Parse(msg) => write!(f, "parse error: {msg}"),
            Self::NotFound(pkg) => write!(f, "package not found: {pkg}"),
            Self::Io(err) => write!(f, "IO error: {err}"),
            Self::Decompress(msg) => write!(f, "decompression error: {msg}"),
        }
    }
}

impl std::error::Error for IndexError {}

impl From<std::io::Error> for IndexError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<ureq::Error> for IndexError {
    fn from(err: ureq::Error) -> Self {
        Self::Network(err.to_string())
    }
}

impl From<serde_json::Error> for IndexError {
    fn from(err: serde_json::Error) -> Self {
        Self::Parse(err.to_string())
    }
}

/// Trait for package index fetchers.
///
/// Each implementation pulls metadata from a package manager's index
/// (apt Sources, brew API, crates.io, etc.).
pub trait PackageIndex: Send + Sync {
    /// Ecosystem identifier (e.g., "apt", "pacman", "brew").
    fn ecosystem(&self) -> &'static str;

    /// Human-readable name.
    fn display_name(&self) -> &'static str;

    /// Fetch metadata for a specific package.
    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError>;

    /// Fetch available versions for a package.
    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError>;

    /// Whether this index supports bulk fetching via `fetch_all()`.
    fn supports_fetch_all(&self) -> bool {
        false
    }

    /// Iterate over all packages in the index.
    ///
    /// Check `supports_fetch_all()` first - this returns an error if not supported.
    /// This may be expensive for large indices.
    fn fetch_all(&self) -> Result<Vec<PackageMeta>, IndexError> {
        Err(IndexError::Network(
            "bulk fetch not implemented for this index".into(),
        ))
    }

    /// Search packages by name pattern.
    ///
    /// Default implementation fetches all and filters; override for
    /// indices with native search APIs.
    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let all = self.fetch_all()?;
        let query_lower = query.to_lowercase();
        Ok(all
            .into_iter()
            .filter(|p| p.name.to_lowercase().contains(&query_lower))
            .collect())
    }
}
