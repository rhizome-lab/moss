//! Ruff adapter - Python linter and formatter.
//!
//! Ruff is an extremely fast Python linter, written in Rust.
//! https://docs.astral.sh/ruff/

use crate::{
    Diagnostic, DiagnosticSeverity, Location, Tool, ToolCategory, ToolError, ToolInfo, ToolResult,
};
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

/// Ruff Python linter/formatter adapter.
pub struct Ruff {
    info: ToolInfo,
}

impl Ruff {
    pub fn new() -> Self {
        Self {
            info: ToolInfo {
                name: "ruff",
                category: ToolCategory::Linter,
                extensions: &["py", "pyi"],
                check_cmd: &["ruff", "--version"],
                website: "https://docs.astral.sh/ruff/",
            },
        }
    }
}

impl Default for Ruff {
    fn default() -> Self {
        Self::new()
    }
}

/// Ruff JSON output format.
#[derive(Debug, Deserialize)]
struct RuffDiagnostic {
    code: Option<String>,
    message: String,
    filename: String,
    location: RuffLocation,
    end_location: RuffLocation,
    fix: Option<RuffFix>,
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RuffLocation {
    row: usize,
    column: usize,
}

#[derive(Debug, Deserialize)]
struct RuffFix {
    message: Option<String>,
    #[allow(dead_code)]
    applicability: String,
    // edits field omitted for simplicity
}

impl Tool for Ruff {
    fn info(&self) -> &ToolInfo {
        &self.info
    }

    fn is_available(&self) -> bool {
        Command::new("ruff")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn version(&self) -> Option<String> {
        Command::new("ruff")
            .arg("--version")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
    }

    fn detect(&self, root: &Path) -> f32 {
        let mut score: f32 = 0.0;

        // Config files
        let config_files = [
            "pyproject.toml",
            "ruff.toml",
            ".ruff.toml",
            "setup.py",
            "setup.cfg",
        ];
        if crate::tools::has_config_file(root, &config_files) {
            score += 0.5;
        }

        // Lock files indicate Python project
        let lock_files = ["uv.lock", "poetry.lock", "Pipfile.lock", "requirements.txt"];
        if crate::tools::has_config_file(root, &lock_files) {
            score += 0.3;
        }

        // Python files exist
        if crate::tools::has_files_with_extensions(root, self.info.extensions) {
            score += 0.2;
        }

        score.min(1.0)
    }

    fn run(&self, paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError> {
        let path_args: Vec<&str> = if paths.is_empty() {
            vec!["."]
        } else {
            paths.iter().map(|p| p.to_str().unwrap_or(".")).collect()
        };

        let output = Command::new("ruff")
            .arg("check")
            .arg("--output-format=json")
            .args(&path_args)
            .current_dir(root)
            .output()?;

        // Ruff returns exit code 1 if there are violations, which is expected
        let stdout = String::from_utf8_lossy(&output.stdout);

        if stdout.trim().is_empty() {
            return Ok(ToolResult::success("ruff", vec![]));
        }

        let ruff_diags: Vec<RuffDiagnostic> = serde_json::from_str(&stdout)
            .map_err(|e| ToolError::ParseError(format!("failed to parse ruff output: {}", e)))?;

        let diagnostics = ruff_diags
            .into_iter()
            .map(|d| {
                let severity = match d.code.as_deref() {
                    Some(code) if code.starts_with('E') || code.starts_with('F') => {
                        DiagnosticSeverity::Error
                    }
                    Some(code) if code.starts_with('W') => DiagnosticSeverity::Warning,
                    _ => DiagnosticSeverity::Warning,
                };

                let mut diag = Diagnostic {
                    tool: "ruff".to_string(),
                    rule_id: d.code.unwrap_or_else(|| "unknown".to_string()),
                    message: d.message,
                    severity,
                    location: Location {
                        file: d.filename.into(),
                        line: d.location.row,
                        column: d.location.column,
                        end_line: Some(d.end_location.row),
                        end_column: Some(d.end_location.column),
                    },
                    fix: None,
                    help_url: d.url,
                };

                if let Some(fix) = d.fix {
                    if let Some(msg) = fix.message {
                        diag.fix = Some(crate::diagnostic::Fix {
                            description: msg,
                            replacement: String::new(), // Would need to parse edits
                        });
                    }
                }

                diag
            })
            .collect();

        Ok(ToolResult::success("ruff", diagnostics))
    }

    fn can_fix(&self) -> bool {
        true
    }

    fn fix(&self, paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError> {
        let path_args: Vec<&str> = if paths.is_empty() {
            vec!["."]
        } else {
            paths.iter().map(|p| p.to_str().unwrap_or(".")).collect()
        };

        let output = Command::new("ruff")
            .arg("check")
            .arg("--fix")
            .arg("--output-format=json")
            .args(&path_args)
            .current_dir(root)
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        if stdout.trim().is_empty() {
            return Ok(ToolResult::success("ruff", vec![]));
        }

        // Parse remaining unfixable issues
        let ruff_diags: Vec<RuffDiagnostic> = serde_json::from_str(&stdout).unwrap_or_default();

        let diagnostics = ruff_diags
            .into_iter()
            .map(|d| Diagnostic {
                tool: "ruff".to_string(),
                rule_id: d.code.unwrap_or_else(|| "unknown".to_string()),
                message: d.message,
                severity: DiagnosticSeverity::Warning,
                location: Location {
                    file: d.filename.into(),
                    line: d.location.row,
                    column: d.location.column,
                    end_line: Some(d.end_location.row),
                    end_column: Some(d.end_location.column),
                },
                fix: None,
                help_url: d.url,
            })
            .collect();

        Ok(ToolResult::success("ruff", diagnostics))
    }
}
