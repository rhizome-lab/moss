//! Mypy adapter - Python static type checker.
//!
//! Mypy is a static type checker for Python.
//! https://mypy-lang.org/

use crate::{
    Diagnostic, DiagnosticSeverity, Location, Tool, ToolCategory, ToolError, ToolInfo, ToolResult,
};
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

fn mypy_command() -> Option<(&'static str, Vec<&'static str>)> {
    crate::tools::find_python_tool("mypy")
}

/// Mypy Python type checker adapter.
pub struct Mypy {
    info: ToolInfo,
}

impl Mypy {
    pub fn new() -> Self {
        Self {
            info: ToolInfo {
                name: "mypy",
                category: ToolCategory::TypeChecker,
                extensions: &["py", "pyi"],
                check_cmd: &["mypy", "--version"],
                website: "https://mypy-lang.org/",
            },
        }
    }
}

impl Default for Mypy {
    fn default() -> Self {
        Self::new()
    }
}

/// Mypy JSON output format (one object per line).
#[derive(Debug, Deserialize)]
struct MypyDiagnostic {
    file: String,
    line: usize,
    column: usize,
    message: String,
    severity: String,
    code: Option<String>,
}

impl Tool for Mypy {
    fn info(&self) -> &ToolInfo {
        &self.info
    }

    fn is_available(&self) -> bool {
        mypy_command().is_some()
    }

    fn version(&self) -> Option<String> {
        let (cmd, base_args) = mypy_command()?;
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
            "mypy.ini",
            ".mypy.ini",
            "pyproject.toml",
            "setup.cfg",
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
        let (cmd, base_args) =
            mypy_command().ok_or_else(|| ToolError::NotAvailable("mypy not found".to_string()))?;

        let path_args: Vec<&str> = if paths.is_empty() {
            vec!["."]
        } else {
            paths.iter().map(|p| p.to_str().unwrap_or(".")).collect()
        };

        let mut command = Command::new(cmd);
        command.args(&base_args);
        command.arg("--output").arg("json");

        let output = command.args(&path_args).current_dir(root).output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        if stdout.trim().is_empty() {
            return Ok(ToolResult::success("mypy", vec![]));
        }

        // Mypy outputs one JSON object per line
        let mut diagnostics = Vec::new();
        for line in stdout.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(diag) = serde_json::from_str::<MypyDiagnostic>(line) {
                let severity = match diag.severity.as_str() {
                    "error" => DiagnosticSeverity::Error,
                    _ => DiagnosticSeverity::Warning,
                };

                diagnostics.push(Diagnostic {
                    tool: "mypy".to_string(),
                    rule_id: diag.code.unwrap_or_else(|| "type-error".to_string()),
                    message: diag.message,
                    severity,
                    location: Location {
                        file: diag.file.into(),
                        line: diag.line,
                        column: diag.column,
                        end_line: None,
                        end_column: None,
                    },
                    fix: None,
                    help_url: None,
                });
            }
        }

        Ok(ToolResult::success("mypy", diagnostics))
    }

    fn can_fix(&self) -> bool {
        false
    }

    fn fix(&self, _paths: &[&Path], _root: &Path) -> Result<ToolResult, ToolError> {
        Err(ToolError::FixNotSupported)
    }
}
