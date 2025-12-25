//! SARIF 2.1.0 output format.
//!
//! SARIF (Static Analysis Results Interchange Format) is a standard format
//! for static analysis tool output. Supported by GitHub, VS Code, and many CI systems.

use crate::Diagnostic;
use serde::Serialize;
use std::collections::HashMap;

/// SARIF 2.1.0 report.
#[derive(Debug, Serialize)]
pub struct SarifReport {
    #[serde(rename = "$schema")]
    pub schema: &'static str,
    pub version: &'static str,
    pub runs: Vec<SarifRun>,
}

#[derive(Debug, Serialize)]
pub struct SarifRun {
    pub tool: SarifTool,
    pub results: Vec<SarifResult>,
}

#[derive(Debug, Serialize)]
pub struct SarifTool {
    pub driver: SarifDriver,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifDriver {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub information_uri: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<SarifRule>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifRule {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short_description: Option<SarifMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help_uri: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SarifMessage {
    pub text: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifResult {
    pub rule_id: String,
    pub level: String,
    pub message: SarifMessage,
    pub locations: Vec<SarifLocation>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fixes: Vec<SarifFix>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifLocation {
    pub physical_location: SarifPhysicalLocation,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifPhysicalLocation {
    pub artifact_location: SarifArtifactLocation,
    pub region: SarifRegion,
}

#[derive(Debug, Serialize)]
pub struct SarifArtifactLocation {
    pub uri: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifRegion {
    pub start_line: usize,
    pub start_column: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_column: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifFix {
    pub description: SarifMessage,
    pub artifact_changes: Vec<SarifArtifactChange>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifArtifactChange {
    pub artifact_location: SarifArtifactLocation,
    pub replacements: Vec<SarifReplacement>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifReplacement {
    pub deleted_region: SarifRegion,
    pub inserted_content: SarifContent,
}

#[derive(Debug, Serialize)]
pub struct SarifContent {
    pub text: String,
}

impl SarifReport {
    /// Create a SARIF report from diagnostics.
    pub fn from_diagnostics(diagnostics: &[Diagnostic]) -> Self {
        // Group diagnostics by tool
        let mut by_tool: HashMap<&str, Vec<&Diagnostic>> = HashMap::new();
        for d in diagnostics {
            by_tool.entry(&d.tool).or_default().push(d);
        }

        let runs = by_tool
            .into_iter()
            .map(|(tool_name, diags)| {
                // Collect unique rules
                let mut rules_map: HashMap<&str, SarifRule> = HashMap::new();
                for d in &diags {
                    rules_map.entry(&d.rule_id).or_insert_with(|| SarifRule {
                        id: d.rule_id.clone(),
                        short_description: Some(SarifMessage {
                            text: d.message.clone(),
                        }),
                        help_uri: d.help_url.clone(),
                    });
                }

                let results = diags
                    .iter()
                    .map(|d| {
                        let fixes = if let Some(fix) = &d.fix {
                            vec![SarifFix {
                                description: SarifMessage {
                                    text: fix.description.clone(),
                                },
                                artifact_changes: vec![SarifArtifactChange {
                                    artifact_location: SarifArtifactLocation {
                                        uri: d.location.file.display().to_string(),
                                    },
                                    replacements: vec![SarifReplacement {
                                        deleted_region: SarifRegion {
                                            start_line: d.location.line,
                                            start_column: d.location.column,
                                            end_line: d.location.end_line,
                                            end_column: d.location.end_column,
                                        },
                                        inserted_content: SarifContent {
                                            text: fix.replacement.clone(),
                                        },
                                    }],
                                }],
                            }]
                        } else {
                            vec![]
                        };

                        SarifResult {
                            rule_id: d.rule_id.clone(),
                            level: d.severity.to_sarif_level().to_string(),
                            message: SarifMessage {
                                text: d.message.clone(),
                            },
                            locations: vec![SarifLocation {
                                physical_location: SarifPhysicalLocation {
                                    artifact_location: SarifArtifactLocation {
                                        uri: d.location.file.display().to_string(),
                                    },
                                    region: SarifRegion {
                                        start_line: d.location.line,
                                        start_column: d.location.column,
                                        end_line: d.location.end_line,
                                        end_column: d.location.end_column,
                                    },
                                },
                            }],
                            fixes,
                        }
                    })
                    .collect();

                SarifRun {
                    tool: SarifTool {
                        driver: SarifDriver {
                            name: tool_name.to_string(),
                            version: None,
                            information_uri: None,
                            rules: rules_map.into_values().collect(),
                        },
                    },
                    results,
                }
            })
            .collect();

        SarifReport {
            schema: "https://json.schemastore.org/sarif-2.1.0.json",
            version: "2.1.0",
            runs,
        }
    }

    /// Serialize to JSON string.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}
