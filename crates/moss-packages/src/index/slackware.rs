//! Slackware package index fetcher (SlackBuilds).
//!
//! Fetches package metadata from slackbuilds.org.

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// Slackware package index fetcher (SlackBuilds.org).
pub struct Slackware;

impl Slackware {
    /// SlackBuilds.org API.
    const SBO_API: &'static str = "https://slackbuilds.org";
}

impl PackageIndex for Slackware {
    fn ecosystem(&self) -> &'static str {
        "slackware"
    }

    fn display_name(&self) -> &'static str {
        "SlackBuilds.org"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // SlackBuilds doesn't have a proper JSON API, so we construct
        // package info from the .info file in their git repo.
        // Try common categories to find the package.
        for category in &[
            "system",
            "development",
            "network",
            "multimedia",
            "desktop",
            "misc",
            "libraries",
            "games",
            "graphics",
            "office",
            "audio",
            "academic",
            "accessibility",
            "business",
            "gis",
            "ham",
            "haskell",
            "perl",
            "python",
            "ruby",
        ] {
            let info_url = format!(
                "https://raw.githubusercontent.com/SlackBuildsOrg/slackbuilds/master/{}/{}/{}.info",
                category, name, name
            );

            if let Ok(response) = ureq::get(&info_url).call() {
                if let Ok(body) = response.into_string() {
                    return parse_sbo_info(&body, name, *category);
                }
            }
        }

        Err(IndexError::NotFound(name.to_string()))
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // SlackBuilds only maintains current version
        let pkg = self.fetch(name)?;
        Ok(vec![VersionMeta {
            version: pkg.version,
            released: None,
            yanked: false,
        }])
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        // SlackBuilds doesn't have a JSON search API
        // Return an error suggesting to use fetch() directly
        Err(IndexError::Network(format!(
            "SlackBuilds search not implemented via API. Use fetch() with exact package name, or visit: {}/result/?search={}",
            Self::SBO_API,
            query
        )))
    }
}

fn parse_sbo_info(content: &str, name: &str, category: &str) -> Result<PackageMeta, IndexError> {
    let mut version = String::new();
    let mut homepage = None;
    let mut maintainer = None;
    let mut email = None;

    for line in content.lines() {
        if let Some(val) = line.strip_prefix("VERSION=") {
            version = val.trim_matches('"').to_string();
        } else if let Some(val) = line.strip_prefix("HOMEPAGE=") {
            homepage = Some(val.trim_matches('"').to_string());
        } else if let Some(val) = line.strip_prefix("MAINTAINER=") {
            maintainer = Some(val.trim_matches('"').to_string());
        } else if let Some(val) = line.strip_prefix("EMAIL=") {
            email = Some(val.trim_matches('"').to_string());
        }
    }

    let maintainers = match (maintainer, email) {
        (Some(m), Some(e)) => vec![format!("{} <{}>", m, e)],
        (Some(m), None) => vec![m],
        _ => Vec::new(),
    };

    Ok(PackageMeta {
        name: format!("{}/{}", category, name),
        version,
        description: None, // Would need to parse README
        homepage,
        repository: Some(format!(
            "https://github.com/SlackBuildsOrg/slackbuilds/tree/master/{}/{}",
            category, name
        )),
        license: None,
        maintainers,
        binaries: Vec::new(),
        ..Default::default()
    })
}
