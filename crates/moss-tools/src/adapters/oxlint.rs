//! Oxlint adapter - fast JavaScript/TypeScript linter.
//!
//! Oxlint is an extremely fast JavaScript linter, written in Rust.
//! Supports type-aware linting with --type-aware flag when tsconfig.json is present.
//! https://oxc.rs/docs/guide/usage/linter.html

use crate::{
    Diagnostic, DiagnosticSeverity, Location, Tool, ToolCategory, ToolError, ToolInfo, ToolResult,
};
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

/// Oxlint JavaScript/TypeScript linter adapter.
pub struct Oxlint {
    info: ToolInfo,
}

impl Oxlint {
    pub fn new() -> Self {
        Self {
            info: ToolInfo {
                name: "oxlint",
                category: ToolCategory::Linter,
                extensions: &["js", "jsx", "ts", "tsx", "mjs", "cjs", "mts", "cts"],
                check_cmd: &["oxlint", "--version"],
                website: "https://oxc.rs/",
            },
        }
    }
}

impl Default for Oxlint {
    fn default() -> Self {
        Self::new()
    }
}

fn oxlint_command() -> Option<(&'static str, Vec<&'static str>)> {
    crate::tools::find_js_tool("oxlint", None)
}

fn has_tsconfig(root: &Path) -> bool {
    crate::tools::has_config_file(root, &["tsconfig.json"])
}

/// Oxlint JSON output format.
#[derive(Debug, Deserialize)]
struct OxlintOutput {
    #[serde(default)]
    diagnostics: Vec<OxlintDiagnostic>,
}

#[derive(Debug, Deserialize)]
struct OxlintDiagnostic {
    #[serde(rename = "ruleId")]
    rule_id: Option<String>,
    message: String,
    severity: String,
    #[serde(default)]
    labels: Vec<OxlintLabel>,
    #[serde(rename = "helpMessage")]
    help_message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OxlintLabel {
    #[serde(rename = "sourceText")]
    #[allow(dead_code)]
    source_text: Option<String>,
    span: OxlintSpan,
}

#[derive(Debug, Deserialize)]
struct OxlintSpan {
    file: String,
    start: OxlintPosition,
    end: OxlintPosition,
}

#[derive(Debug, Deserialize)]
struct OxlintPosition {
    line: usize,
    column: usize,
}

impl Tool for Oxlint {
    fn info(&self) -> &ToolInfo {
        &self.info
    }

    fn is_available(&self) -> bool {
        oxlint_command().is_some()
    }

    fn version(&self) -> Option<String> {
        let (cmd, base_args) = oxlint_command()?;
        let mut command = Command::new(cmd);
        command.args(&base_args).arg("--version");
        command
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
    }

    fn detect(&self, root: &Path) -> f32 {
        // Oxlint is a JS ecosystem tool - require package.json
        if !crate::tools::has_config_file(root, &["package.json"]) {
            return 0.0;
        }

        let mut score: f32 = 0.5;

        // TypeScript config
        if crate::tools::has_config_file(root, &["tsconfig.json", "jsconfig.json"]) {
            score += 0.2;
        }

        // Oxlint-specific config
        if crate::tools::has_config_file(root, &["oxlintrc.json", ".oxlintrc.json"]) {
            score += 0.3;
        }

        // JS/TS files exist
        if crate::tools::has_files_with_extensions(root, self.info.extensions) {
            score += 0.2;
        }

        score.min(1.0)
    }

    fn run(&self, paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError> {
        let (cmd, base_args) = oxlint_command()
            .ok_or_else(|| ToolError::NotAvailable("oxlint not found".to_string()))?;

        let path_args: Vec<&str> = if paths.is_empty() {
            vec!["."]
        } else {
            paths.iter().map(|p| p.to_str().unwrap_or(".")).collect()
        };

        let mut command = Command::new(cmd);
        command.args(&base_args);
        command.arg("--format=json");

        // Enable type-aware linting when tsconfig.json is present
        if has_tsconfig(root) {
            command.arg("--type-aware");
        }

        let output = command.args(&path_args).current_dir(root).output()?;

        // Oxlint returns exit code 1 if there are violations
        let stdout = String::from_utf8_lossy(&output.stdout);

        if stdout.trim().is_empty() {
            return Ok(ToolResult::success("oxlint", vec![]));
        }

        // Try parsing as array first (older format), then as object with diagnostics field
        let oxlint_diags: Vec<OxlintDiagnostic> =
            if let Ok(output) = serde_json::from_str::<OxlintOutput>(&stdout) {
                output.diagnostics
            } else if let Ok(diags) = serde_json::from_str::<Vec<OxlintDiagnostic>>(&stdout) {
                diags
            } else {
                return Err(ToolError::ParseError(format!(
                    "failed to parse oxlint output: {}",
                    &stdout[..stdout.len().min(200)]
                )));
            };

        let diagnostics = oxlint_diags
            .into_iter()
            .filter_map(|d| {
                let label = d.labels.first()?;
                let severity = match d.severity.as_str() {
                    "error" => DiagnosticSeverity::Error,
                    "warning" => DiagnosticSeverity::Warning,
                    _ => DiagnosticSeverity::Warning,
                };

                Some(Diagnostic {
                    tool: "oxlint".to_string(),
                    rule_id: d.rule_id.unwrap_or_else(|| "unknown".to_string()),
                    message: d.message,
                    severity,
                    location: Location {
                        file: label.span.file.clone().into(),
                        line: label.span.start.line,
                        column: label.span.start.column,
                        end_line: Some(label.span.end.line),
                        end_column: Some(label.span.end.column),
                    },
                    fix: d.help_message.map(|msg| crate::diagnostic::Fix {
                        description: msg,
                        replacement: String::new(),
                    }),
                    help_url: None,
                })
            })
            .collect();

        Ok(ToolResult::success("oxlint", diagnostics))
    }

    fn can_fix(&self) -> bool {
        true
    }

    fn fix(&self, paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError> {
        let (cmd, base_args) = oxlint_command()
            .ok_or_else(|| ToolError::NotAvailable("oxlint not found".to_string()))?;

        let path_args: Vec<&str> = if paths.is_empty() {
            vec!["."]
        } else {
            paths.iter().map(|p| p.to_str().unwrap_or(".")).collect()
        };

        let mut command = Command::new(cmd);
        command.args(&base_args);
        command.arg("--fix").arg("--format=json");

        // Enable type-aware linting when tsconfig.json is present
        if has_tsconfig(root) {
            command.arg("--type-aware");
        }

        let output = command.args(&path_args).current_dir(root).output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        if stdout.trim().is_empty() {
            return Ok(ToolResult::success("oxlint", vec![]));
        }

        // Parse remaining unfixable issues
        let oxlint_diags: Vec<OxlintDiagnostic> =
            if let Ok(output) = serde_json::from_str::<OxlintOutput>(&stdout) {
                output.diagnostics
            } else {
                serde_json::from_str(&stdout).unwrap_or_default()
            };

        let diagnostics = oxlint_diags
            .into_iter()
            .filter_map(|d| {
                let label = d.labels.first()?;
                Some(Diagnostic {
                    tool: "oxlint".to_string(),
                    rule_id: d.rule_id.unwrap_or_else(|| "unknown".to_string()),
                    message: d.message,
                    severity: DiagnosticSeverity::Warning,
                    location: Location {
                        file: label.span.file.clone().into(),
                        line: label.span.start.line,
                        column: label.span.start.column,
                        end_line: Some(label.span.end.line),
                        end_column: Some(label.span.end.column),
                    },
                    fix: None,
                    help_url: None,
                })
            })
            .collect();

        Ok(ToolResult::success("oxlint", diagnostics))
    }
}
