//! Chocolatey package index fetcher (Windows).
//!
//! Fetches package metadata from the Chocolatey community repository.
//! Uses the NuGet v2 OData API which returns XML (Atom feed format).
//!
//! ## API Strategy
//! - **fetch**: `community.chocolatey.org/api/v2/Packages(Id='{name}')` - NuGet OData XML
//! - **fetch_versions**: `community.chocolatey.org/api/v2/FindPackagesById()?id='{name}'`
//! - **search**: `community.chocolatey.org/api/v2/Search()?searchTerm='{query}'`
//! - **fetch_all**: Not supported (API requires search terms)
//!
//! ## Multi-source Support
//! ```rust,ignore
//! use moss_packages::index::choco::{Choco, ChocoSource};
//!
//! // All sources (default)
//! let all = Choco::all();
//!
//! // Community only
//! let community = Choco::community();
//! ```

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};
use quick_xml::de::from_str;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::Read;

/// Available Chocolatey sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChocoSource {
    /// Chocolatey community repository
    Community,
}

impl ChocoSource {
    /// Get the API base URL for this source.
    fn api_url(&self) -> &'static str {
        match self {
            Self::Community => "https://community.chocolatey.org/api/v2",
        }
    }

    /// Get the source name for tagging.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Community => "community",
        }
    }

    /// All available sources.
    pub fn all() -> &'static [ChocoSource] {
        &[Self::Community]
    }
}

/// Chocolatey package index fetcher with configurable sources.
pub struct Choco {
    sources: Vec<ChocoSource>,
}

impl Choco {
    /// Create a fetcher with all sources.
    pub fn all() -> Self {
        Self {
            sources: ChocoSource::all().to_vec(),
        }
    }

    /// Create a fetcher with community source only.
    pub fn community() -> Self {
        Self {
            sources: vec![ChocoSource::Community],
        }
    }

    /// Create a fetcher with custom source selection.
    pub fn with_sources(sources: &[ChocoSource]) -> Self {
        Self {
            sources: sources.to_vec(),
        }
    }

    /// Fetch a package from a specific source.
    fn fetch_from_source(name: &str, source: ChocoSource) -> Result<PackageMeta, IndexError> {
        let url = format!(
            "{}/Packages()?$filter=Id%20eq%20'{}'%20and%20IsLatestVersion&$top=1",
            source.api_url(),
            urlencoding::encode(name)
        );

        let response = ureq::get(&url).call()?;
        let xml = response.into_string()?;

        let packages = parse_odata_response(&xml)?;
        let props = packages
            .first()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        props
            .to_package_meta(name, source)
            .ok_or_else(|| IndexError::NotFound(name.to_string()))
    }

    /// Fetch versions from a specific source.
    fn fetch_versions_from_source(
        name: &str,
        source: ChocoSource,
    ) -> Result<Vec<VersionMeta>, IndexError> {
        let url = format!(
            "{}/Packages()?$filter=Id%20eq%20'{}'&$orderby=Version%20desc&$top=20",
            source.api_url(),
            urlencoding::encode(name)
        );

        let response = ureq::get(&url).call()?;
        let xml = response.into_string()?;

        let packages = parse_odata_response(&xml)?;

        Ok(packages
            .iter()
            .filter_map(|p| p.to_version_meta())
            .collect())
    }

    /// Search a specific source.
    fn search_source(query: &str, source: ChocoSource) -> Result<Vec<PackageMeta>, IndexError> {
        // Limit to 10 results (XML responses are verbose)
        let url = format!(
            "{}/Search()?searchTerm='{}'&includePrerelease=false&$top=10",
            source.api_url(),
            urlencoding::encode(query)
        );

        let response = ureq::get(&url).call()?;
        // Read full response body (into_string has 10MB limit which should be plenty)
        let mut xml = String::new();
        response.into_reader().read_to_string(&mut xml)?;

        let packages = parse_odata_response(&xml)?;

        Ok(packages
            .iter()
            .filter_map(|p| p.to_package_meta("", source))
            .collect())
    }
}

// OData Atom feed structures for deserialization
// Note: quick_xml serde sees namespace prefixes as literal element names
#[derive(Debug, Deserialize)]
struct Feed {
    #[serde(rename = "entry", default)]
    entries: Vec<Entry>,
}

#[derive(Debug, Deserialize)]
struct Entry {
    // Try both prefixed and unprefixed forms
    #[serde(rename = "m:properties", alias = "properties", default)]
    properties: Option<Properties>,
}

#[derive(Debug, Deserialize)]
struct Properties {
    // quick_xml sees "d:Id" as the element name when namespace prefixes are used
    #[serde(rename = "d:Id", alias = "Id", default)]
    id: Option<String>,
    #[serde(rename = "d:Version", alias = "Version", default)]
    version: Option<String>,
    #[serde(rename = "d:Description", alias = "Description", default)]
    description: Option<String>,
    #[serde(rename = "d:Summary", alias = "Summary", default)]
    summary: Option<String>,
    #[serde(rename = "d:ProjectUrl", alias = "ProjectUrl", default)]
    project_url: Option<String>,
    #[serde(rename = "d:ProjectSourceUrl", alias = "ProjectSourceUrl", default)]
    project_source_url: Option<String>,
    #[serde(rename = "d:PackageSourceUrl", alias = "PackageSourceUrl", default)]
    package_source_url: Option<String>,
    #[serde(rename = "d:LicenseUrl", alias = "LicenseUrl", default)]
    license_url: Option<String>,
    #[serde(rename = "d:Published", alias = "Published", default)]
    published: Option<String>,
    #[serde(rename = "d:IsPrerelease", alias = "IsPrerelease", default)]
    is_prerelease: Option<String>,
}

impl Properties {
    fn to_package_meta(&self, name: &str, source: ChocoSource) -> Option<PackageMeta> {
        let mut extra = HashMap::new();
        extra.insert(
            "source_repo".to_string(),
            serde_json::Value::String(source.name().to_string()),
        );

        Some(PackageMeta {
            name: self.id.clone().unwrap_or_else(|| name.to_string()),
            version: self
                .version
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            description: self.description.clone().or_else(|| self.summary.clone()),
            homepage: self.project_url.clone(),
            repository: self
                .project_source_url
                .clone()
                .or_else(|| self.package_source_url.clone()),
            license: self.license_url.clone(),
            binaries: Vec::new(),
            keywords: Vec::new(),    // NuGet OData doesn't expose tags in search
            maintainers: Vec::new(), // Not exposed in OData API
            published: self.published.clone(),
            downloads: None, // Would need separate API call
            archive_url: None,
            checksum: None,
            extra,
        })
    }

    fn to_version_meta(&self) -> Option<VersionMeta> {
        Some(VersionMeta {
            version: self.version.clone()?,
            released: self.published.clone(),
            yanked: self.is_prerelease.as_deref() == Some("true"),
        })
    }
}

/// Sanitize potentially malformed OData XML from Chocolatey API.
/// The Search endpoint sometimes returns truncated responses with unclosed `<link rel="next">` tags.
fn sanitize_odata_xml(xml: &str) -> String {
    // If we have an unclosed <link rel="next">, remove it and close the feed
    if let Some(pos) = xml.find("<link rel=\"next\">") {
        let mut sanitized = xml[..pos].to_string();
        sanitized.push_str("</feed>");
        sanitized
    } else if !xml.contains("</feed>") {
        // If there's no closing feed tag, add it
        let mut sanitized = xml.to_string();
        sanitized.push_str("</feed>");
        sanitized
    } else {
        xml.to_string()
    }
}

fn parse_odata_response(xml: &str) -> Result<Vec<Properties>, IndexError> {
    let xml = sanitize_odata_xml(xml);

    // Try to parse as a feed with multiple entries
    match from_str::<Feed>(&xml) {
        Ok(feed) => {
            return Ok(feed
                .entries
                .into_iter()
                .filter_map(|e| e.properties)
                .collect());
        }
        Err(feed_err) => {
            // Try to parse as a single entry
            match from_str::<Entry>(&xml) {
                Ok(entry) => {
                    if let Some(props) = entry.properties {
                        return Ok(vec![props]);
                    }
                }
                Err(_) => {
                    // Return the feed error since that's more likely what we expected
                    return Err(IndexError::Parse(format!(
                        "failed to parse OData XML: {}",
                        feed_err
                    )));
                }
            }
        }
    }

    Err(IndexError::Parse(
        "OData XML parsed but no properties found".into(),
    ))
}

impl PackageIndex for Choco {
    fn ecosystem(&self) -> &'static str {
        "choco"
    }

    fn display_name(&self) -> &'static str {
        "Chocolatey (Windows)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Try each configured source until we find the package
        for &source in &self.sources {
            match Self::fetch_from_source(name, source) {
                Ok(pkg) => return Ok(pkg),
                Err(IndexError::NotFound(_)) => continue,
                Err(e) => return Err(e),
            }
        }

        Err(IndexError::NotFound(name.to_string()))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        let mut all_versions = Vec::new();

        for &source in &self.sources {
            if let Ok(versions) = Self::fetch_versions_from_source(name, source) {
                all_versions.extend(versions);
            }
        }

        if all_versions.is_empty() {
            return Err(IndexError::NotFound(name.to_string()));
        }

        Ok(all_versions)
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let mut results = Vec::new();

        for &source in &self.sources {
            if let Ok(packages) = Self::search_source(query, source) {
                results.extend(packages);
            }
        }

        Ok(results)
    }
}
