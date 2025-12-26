//! ESLint adapter - JavaScript/TypeScript linter.
//!
//! ESLint is a pluggable linting utility for JavaScript and TypeScript.
//! https://eslint.org/

use crate::{
    Diagnostic, DiagnosticSeverity, Location, Tool, ToolCategory, ToolError, ToolInfo, ToolResult,
};
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

fn eslint_command() -> Option<(&'static str, Vec<&'static str>)> {
    crate::tools::find_js_tool("eslint", None)
}

/// ESLint JavaScript/TypeScript linter adapter.
pub struct Eslint {
    info: ToolInfo,
}

impl Eslint {
    pub fn new() -> Self {
        Self {
            info: ToolInfo {
                name: "eslint",
                category: ToolCategory::Linter,
                extensions: &["js", "jsx", "ts", "tsx", "mjs", "cjs"],
                check_cmd: &["eslint", "--version"],
                website: "https://eslint.org/",
            },
        }
    }
}

impl Default for Eslint {
    fn default() -> Self {
        Self::new()
    }
}

/// ESLint JSON output format.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EslintFile {
    file_path: String,
    messages: Vec<EslintMessage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EslintMessage {
    rule_id: Option<String>,
    severity: u8, // 1 = warning, 2 = error
    message: String,
    line: usize,
    column: usize,
    end_line: Option<usize>,
    end_column: Option<usize>,
}

impl Tool for Eslint {
    fn info(&self) -> &ToolInfo {
        &self.info
    }

    fn is_available(&self) -> bool {
        eslint_command().is_some()
    }

    fn version(&self) -> Option<String> {
        let (cmd, base_args) = eslint_command()?;
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
        let config_files = [
            ".eslintrc",
            ".eslintrc.js",
            ".eslintrc.cjs",
            ".eslintrc.json",
            ".eslintrc.yml",
            ".eslintrc.yaml",
            "eslint.config.js",
            "eslint.config.mjs",
            "eslint.config.cjs",
        ];
        if crate::tools::has_config_file(root, &config_files) {
            1.0
        } else {
            0.0
        }
    }

    fn run(&self, paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError> {
        let (cmd, base_args) = eslint_command()
            .ok_or_else(|| ToolError::NotAvailable("eslint not found".to_string()))?;

        let path_args: Vec<&str> = if paths.is_empty() {
            vec!["."]
        } else {
            paths.iter().map(|p| p.to_str().unwrap_or(".")).collect()
        };

        let mut command = Command::new(cmd);
        command.args(&base_args);
        command.arg("--format").arg("json");

        let output = command.args(&path_args).current_dir(root).output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        if stdout.trim().is_empty() || stdout.trim() == "[]" {
            return Ok(ToolResult::success("eslint", vec![]));
        }

        let eslint_files: Vec<EslintFile> = serde_json::from_str(&stdout)
            .map_err(|e| ToolError::ParseError(format!("failed to parse eslint output: {}", e)))?;

        let diagnostics = eslint_files
            .into_iter()
            .flat_map(|file| {
                file.messages.into_iter().map(move |msg| {
                    let severity = if msg.severity >= 2 {
                        DiagnosticSeverity::Error
                    } else {
                        DiagnosticSeverity::Warning
                    };

                    Diagnostic {
                        tool: "eslint".to_string(),
                        rule_id: msg.rule_id.unwrap_or_else(|| "parse-error".to_string()),
                        message: msg.message,
                        severity,
                        location: Location {
                            file: file.file_path.clone().into(),
                            line: msg.line,
                            column: msg.column,
                            end_line: msg.end_line,
                            end_column: msg.end_column,
                        },
                        fix: None,
                        help_url: None,
                    }
                })
            })
            .collect();

        Ok(ToolResult::success("eslint", diagnostics))
    }

    fn can_fix(&self) -> bool {
        true
    }

    fn fix(&self, paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError> {
        let (cmd, base_args) = eslint_command()
            .ok_or_else(|| ToolError::NotAvailable("eslint not found".to_string()))?;

        let path_args: Vec<&str> = if paths.is_empty() {
            vec!["."]
        } else {
            paths.iter().map(|p| p.to_str().unwrap_or(".")).collect()
        };

        let mut command = Command::new(cmd);
        command.args(&base_args);
        command.arg("--fix").arg("--format").arg("json");

        let output = command.args(&path_args).current_dir(root).output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        if stdout.trim().is_empty() || stdout.trim() == "[]" {
            return Ok(ToolResult::success("eslint", vec![]));
        }

        // Parse remaining unfixable issues
        let eslint_files: Vec<EslintFile> = serde_json::from_str(&stdout).unwrap_or_default();

        let diagnostics = eslint_files
            .into_iter()
            .flat_map(|file| {
                file.messages.into_iter().map(move |msg| Diagnostic {
                    tool: "eslint".to_string(),
                    rule_id: msg.rule_id.unwrap_or_else(|| "parse-error".to_string()),
                    message: msg.message,
                    severity: DiagnosticSeverity::Warning,
                    location: Location {
                        file: file.file_path.clone().into(),
                        line: msg.line,
                        column: msg.column,
                        end_line: msg.end_line,
                        end_column: msg.end_column,
                    },
                    fix: None,
                    help_url: None,
                })
            })
            .collect();

        Ok(ToolResult::success("eslint", diagnostics))
    }
}
