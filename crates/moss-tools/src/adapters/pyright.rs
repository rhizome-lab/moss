//! Pyright adapter - Python static type checker.
//!
//! Pyright is a fast type checker for Python, written in TypeScript.
//! https://github.com/microsoft/pyright

use crate::{
    Diagnostic, DiagnosticSeverity, Location, Tool, ToolCategory, ToolError, ToolInfo, ToolResult,
};
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

fn pyright_command() -> Option<(&'static str, Vec<&'static str>)> {
    // Pyright can be installed via npm or pip
    // Try Python first (more common in Python projects), then npm
    if let Some(cmd) = crate::tools::find_python_tool("pyright") {
        return Some(cmd);
    }
    crate::tools::find_js_tool("pyright", None)
}

/// Pyright Python type checker adapter.
pub struct Pyright {
    info: ToolInfo,
}

impl Pyright {
    pub fn new() -> Self {
        Self {
            info: ToolInfo {
                name: "pyright",
                category: ToolCategory::TypeChecker,
                extensions: &["py", "pyi"],
                check_cmd: &["pyright", "--version"],
                website: "https://github.com/microsoft/pyright",
            },
        }
    }
}

impl Default for Pyright {
    fn default() -> Self {
        Self::new()
    }
}

/// Pyright JSON output format.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PyrightOutput {
    #[serde(default)]
    general_diagnostics: Vec<PyrightDiagnostic>,
}

#[derive(Debug, Deserialize)]
struct PyrightDiagnostic {
    file: String,
    severity: String,
    message: String,
    rule: Option<String>,
    range: PyrightRange,
}

#[derive(Debug, Deserialize)]
struct PyrightRange {
    start: PyrightPosition,
    end: PyrightPosition,
}

#[derive(Debug, Deserialize)]
struct PyrightPosition {
    line: usize,
    character: usize,
}

impl Tool for Pyright {
    fn info(&self) -> &ToolInfo {
        &self.info
    }

    fn is_available(&self) -> bool {
        pyright_command().is_some()
    }

    fn version(&self) -> Option<String> {
        let (cmd, base_args) = pyright_command()?;
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
            "pyrightconfig.json",
            "pyproject.toml",
            "uv.lock",
            "poetry.lock",
            "Pipfile.lock",
            "requirements.txt",
        ];
        if crate::tools::has_config_file(root, &config_files) {
            1.0
        } else {
            0.0
        }
    }

    fn run(&self, paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError> {
        let (cmd, base_args) = pyright_command()
            .ok_or_else(|| ToolError::NotAvailable("pyright not found".to_string()))?;

        let path_args: Vec<&str> = if paths.is_empty() {
            vec!["."]
        } else {
            paths.iter().map(|p| p.to_str().unwrap_or(".")).collect()
        };

        let mut command = Command::new(cmd);
        command.args(&base_args);
        command.arg("--outputjson");

        let output = command.args(&path_args).current_dir(root).output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        if stdout.trim().is_empty() {
            return Ok(ToolResult::success("pyright", vec![]));
        }

        let pyright_output: PyrightOutput = serde_json::from_str(&stdout)
            .map_err(|e| ToolError::ParseError(format!("failed to parse pyright output: {}", e)))?;

        let diagnostics = pyright_output
            .general_diagnostics
            .into_iter()
            .map(|d| {
                let severity = match d.severity.as_str() {
                    "error" => DiagnosticSeverity::Error,
                    _ => DiagnosticSeverity::Warning,
                };

                Diagnostic {
                    tool: "pyright".to_string(),
                    rule_id: d.rule.unwrap_or_else(|| "type-error".to_string()),
                    message: d.message,
                    severity,
                    location: Location {
                        file: d.file.into(),
                        line: d.range.start.line + 1, // 0-indexed
                        column: d.range.start.character + 1,
                        end_line: Some(d.range.end.line + 1),
                        end_column: Some(d.range.end.character + 1),
                    },
                    fix: None,
                    help_url: None,
                }
            })
            .collect();

        Ok(ToolResult::success("pyright", diagnostics))
    }

    fn can_fix(&self) -> bool {
        false
    }

    fn fix(&self, _paths: &[&Path], _root: &Path) -> Result<ToolResult, ToolError> {
        Err(ToolError::FixNotSupported)
    }
}
