//! Nix/NixOS package index fetcher.
//!
//! Fetches package metadata from nixpkgs via the NixOS search API.
//! Uses the Bonsai Elasticsearch cluster that powers search.nixos.org.

use super::{IndexError, PackageIndex, PackageMeta, VersionMeta};

/// Simple base64 encoding for Basic Auth.
fn base64_encode(input: &str) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut result = String::with_capacity((bytes.len() + 2) / 3 * 4);

    for chunk in bytes.chunks(3) {
        let mut buf = [0u8; 3];
        buf[..chunk.len()].copy_from_slice(chunk);

        let indices = [
            (buf[0] >> 2) as usize,
            (((buf[0] & 0x03) << 4) | (buf[1] >> 4)) as usize,
            (((buf[1] & 0x0f) << 2) | (buf[2] >> 6)) as usize,
            (buf[2] & 0x3f) as usize,
        ];

        result.push(ALPHABET[indices[0]] as char);
        result.push(ALPHABET[indices[1]] as char);
        result.push(if chunk.len() > 1 {
            ALPHABET[indices[2]] as char
        } else {
            '='
        });
        result.push(if chunk.len() > 2 {
            ALPHABET[indices[3]] as char
        } else {
            '='
        });
    }

    result
}

/// Nix package index fetcher.
pub struct Nix;

impl Nix {
    /// NixOS Elasticsearch-based search API (Bonsai cluster).
    /// Public credentials from nixos-search repository.
    const NIXOS_SEARCH: &'static str =
        "https://nixos-search-7-1733963800.us-east-1.bonsaisearch.net";
    const AUTH_USER: &'static str = "aWVSALXpZv";
    const AUTH_PASS: &'static str = "X8gPHnzL52wFEekuxsfQ9cSh";
    /// Index pattern with wildcard to match any version number.
    const INDEX: &'static str = "latest-*-nixos-unstable";
}

impl PackageIndex for Nix {
    fn ecosystem(&self) -> &'static str {
        "nix"
    }

    fn display_name(&self) -> &'static str {
        "Nixpkgs (Nix/NixOS)"
    }

    fn fetch(&self, name: &str) -> Result<PackageMeta, IndexError> {
        // Use Elasticsearch query for exact match
        let query = serde_json::json!({
            "query": {
                "bool": {
                    "must": [
                        { "term": { "type": "package" } },
                        { "term": { "package_attr_name": name } }
                    ]
                }
            },
            "size": 1
        });

        let response: serde_json::Value =
            ureq::post(&format!("{}/{}/_search", Self::NIXOS_SEARCH, Self::INDEX))
                .set("Content-Type", "application/json")
                .set("Accept", "application/json")
                .set(
                    "Authorization",
                    &format!(
                        "Basic {}",
                        base64_encode(&format!("{}:{}", Self::AUTH_USER, Self::AUTH_PASS))
                    ),
                )
                .send_json(&query)?
                .into_json()?;

        let hits = response["hits"]["hits"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing hits".into()))?;

        let hit = hits
            .first()
            .ok_or_else(|| IndexError::NotFound(name.to_string()))?;

        let source = &hit["_source"];

        Ok(PackageMeta {
            name: source["package_attr_name"]
                .as_str()
                .unwrap_or(name)
                .to_string(),
            version: source["package_version"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            description: source["package_description"].as_str().map(String::from),
            homepage: source["package_homepage"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|u| u.as_str())
                .map(String::from),
            repository: extract_repo(&source["package_homepage"]),
            license: source["package_license"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|l| l["fullName"].as_str().or_else(|| l.as_str()))
                .map(String::from),
            binaries: source["package_programs"]
                .as_array()
                .map(|progs| {
                    progs
                        .iter()
                        .filter_map(|p| p.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            maintainers: source["package_maintainers"]
                .as_array()
                .map(|m| {
                    m.iter()
                        .filter_map(|p| {
                            p["name"]
                                .as_str()
                                .or_else(|| p["github"].as_str())
                                .map(String::from)
                        })
                        .collect()
                })
                .unwrap_or_default(),
            ..Default::default()
        })
    }

    fn fetch_versions(&self, name: &str) -> Result<Vec<VersionMeta>, IndexError> {
        // Nixpkgs doesn't expose version history via the search API
        // Each channel has its own version; we only return unstable here
        let pkg = self.fetch(name)?;
        Ok(vec![VersionMeta {
            version: pkg.version,
            released: None,
            yanked: false,
        }])
    }

    fn search(&self, query: &str) -> Result<Vec<PackageMeta>, IndexError> {
        let es_query = serde_json::json!({
            "query": {
                "bool": {
                    "must": [
                        { "term": { "type": "package" } },
                        {
                            "multi_match": {
                                "query": query,
                                "fields": [
                                    "package_attr_name^3",
                                    "package_pname^2",
                                    "package_description"
                                ]
                            }
                        }
                    ]
                }
            },
            "size": 50
        });

        let response: serde_json::Value =
            ureq::post(&format!("{}/{}/_search", Self::NIXOS_SEARCH, Self::INDEX))
                .set("Content-Type", "application/json")
                .set("Accept", "application/json")
                .set(
                    "Authorization",
                    &format!(
                        "Basic {}",
                        base64_encode(&format!("{}:{}", Self::AUTH_USER, Self::AUTH_PASS))
                    ),
                )
                .send_json(&es_query)?
                .into_json()?;

        let hits = response["hits"]["hits"]
            .as_array()
            .ok_or_else(|| IndexError::Parse("missing hits".into()))?;

        Ok(hits
            .iter()
            .filter_map(|hit| {
                let source = &hit["_source"];
                Some(PackageMeta {
                    name: source["package_attr_name"].as_str()?.to_string(),
                    version: source["package_version"]
                        .as_str()
                        .unwrap_or("unknown")
                        .to_string(),
                    description: source["package_description"].as_str().map(String::from),
                    homepage: source["package_homepage"]
                        .as_array()
                        .and_then(|a| a.first())
                        .and_then(|u| u.as_str())
                        .map(String::from),
                    repository: extract_repo(&source["package_homepage"]),
                    license: source["package_license"]
                        .as_array()
                        .and_then(|a| a.first())
                        .and_then(|l| l["fullName"].as_str().or_else(|| l.as_str()))
                        .map(String::from),
                    binaries: source["package_programs"]
                        .as_array()
                        .map(|progs| {
                            progs
                                .iter()
                                .filter_map(|p| p.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default(),
                    maintainers: source["package_maintainers"]
                        .as_array()
                        .map(|m| {
                            m.iter()
                                .filter_map(|p| {
                                    p["name"]
                                        .as_str()
                                        .or_else(|| p["github"].as_str())
                                        .map(String::from)
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    ..Default::default()
                })
            })
            .collect())
    }
}

fn extract_repo(homepage: &serde_json::Value) -> Option<String> {
    homepage.as_array().and_then(|urls| {
        urls.iter()
            .filter_map(|u| u.as_str())
            .find(|u| u.contains("github.com") || u.contains("gitlab.com"))
            .map(String::from)
    })
}
