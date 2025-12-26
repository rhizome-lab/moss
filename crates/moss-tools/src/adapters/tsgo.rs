//! Tsgo adapter - native TypeScript type checker.
//!
//! Tsgo is the native TypeScript implementation from Microsoft, written in Go.
//! ~10x faster than tsc for type checking. Will become TypeScript 7.
//! https://github.com/microsoft/typescript-go

use crate::{
    Diagnostic, DiagnosticSeverity, Location, Tool, ToolCategory, ToolError, ToolInfo, ToolResult,
};
use std::path::Path;
use std::process::Command;

fn tsgo_command() -> Option<(&'static str, Vec<&'static str>)> {
    // @typescript/native-preview provides the tsgo binary
    crate::tools::find_js_tool("tsgo", Some("@typescript/native-preview"))
}

/// Tsgo native TypeScript type checker adapter.
pub struct Tsgo {
    info: ToolInfo,
}

impl Tsgo {
    pub fn new() -> Self {
        Self {
            info: ToolInfo {
                name: "tsgo",
                category: ToolCategory::TypeChecker,
                extensions: &["ts", "tsx", "mts", "cts"],
                check_cmd: &["tsgo", "--version"],
                website: "https://github.com/microsoft/typescript-go",
            },
        }
    }
}

impl Default for Tsgo {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for Tsgo {
    fn info(&self) -> &ToolInfo {
        &self.info
    }

    fn is_available(&self) -> bool {
        tsgo_command().is_some()
    }

    fn version(&self) -> Option<String> {
        let (cmd, base_args) = tsgo_command()?;
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
        // Prefer tsgo over tsc when tsconfig exists (tsgo is faster)
        if crate::tools::has_config_file(root, &["tsconfig.json"]) {
            1.0
        } else {
            0.0
        }
    }

    fn run(&self, paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError> {
        let (cmd_name, base_args) =
            tsgo_command().ok_or_else(|| ToolError::NotAvailable("tsgo not found".to_string()))?;

        // tsgo uses similar flags to tsc
        let mut cmd = Command::new(cmd_name);
        cmd.args(&base_args);
        cmd.arg("--noEmit").arg("--pretty").arg("false");

        // If specific paths provided, pass them
        if !paths.is_empty() {
            for path in paths {
                if let Some(p) = path.to_str() {
                    cmd.arg(p);
                }
            }
        }

        let output = cmd.current_dir(root).output()?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let combined = format!("{}{}", stdout, stderr);

        if combined.trim().is_empty() || output.status.success() {
            return Ok(ToolResult::success("tsgo", vec![]));
        }

        // Parse output - same format as tsc
        let diagnostics = parse_tsgo_output(&combined);

        Ok(ToolResult::success("tsgo", diagnostics))
    }
}

/// Parse tsgo output (same format as tsc).
///
/// Format: `file.ts(10,5): error TS2322: Type 'string' is not assignable to type 'number'.`
fn parse_tsgo_output(output: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for line in output.lines() {
        // Match pattern: file(line,col): severity TScode: message
        if let Some((loc_part, rest)) = line.split_once("): ") {
            if let Some((file, pos)) = loc_part.rsplit_once('(') {
                let parts: Vec<&str> = pos.split(',').collect();
                if parts.len() >= 2 {
                    let line_num = parts[0].parse().unwrap_or(1);
                    let col_num = parts[1].parse().unwrap_or(1);

                    // Parse severity and code
                    let (severity, code, message) =
                        if let Some((sev_code, msg)) = rest.split_once(": ") {
                            let (sev, code) = sev_code.split_once(' ').unwrap_or((sev_code, ""));
                            let severity = match sev {
                                "error" => DiagnosticSeverity::Error,
                                "warning" => DiagnosticSeverity::Warning,
                                _ => DiagnosticSeverity::Error,
                            };
                            (severity, code.to_string(), msg.to_string())
                        } else {
                            (
                                DiagnosticSeverity::Error,
                                "unknown".to_string(),
                                rest.to_string(),
                            )
                        };

                    diagnostics.push(Diagnostic {
                        tool: "tsgo".to_string(),
                        rule_id: code,
                        message,
                        severity,
                        location: Location {
                            file: file.to_string().into(),
                            line: line_num,
                            column: col_num,
                            end_line: None,
                            end_column: None,
                        },
                        fix: None,
                        help_url: None,
                    });
                }
            }
        }
    }

    diagnostics
}
