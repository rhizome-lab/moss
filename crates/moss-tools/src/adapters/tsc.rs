//! TypeScript compiler adapter - type checker.
//!
//! TypeScript's tsc is the official type checker for TypeScript projects.
//! https://www.typescriptlang.org/

use crate::{
    Diagnostic, DiagnosticSeverity, Location, Tool, ToolCategory, ToolError, ToolInfo, ToolResult,
};
use std::path::Path;
use std::process::Command;

fn tsc_command() -> Option<(&'static str, Vec<&'static str>)> {
    // tsc binary comes from the "typescript" package
    crate::tools::find_js_tool("tsc", Some("typescript"))
}

/// TypeScript compiler (tsc) type checker adapter.
pub struct Tsc {
    info: ToolInfo,
}

impl Tsc {
    pub fn new() -> Self {
        Self {
            info: ToolInfo {
                name: "tsc",
                category: ToolCategory::TypeChecker,
                extensions: &["ts", "tsx", "mts", "cts"],
                check_cmd: &["tsc", "--version"],
                website: "https://www.typescriptlang.org/",
            },
        }
    }
}

impl Default for Tsc {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for Tsc {
    fn info(&self) -> &ToolInfo {
        &self.info
    }

    fn is_available(&self) -> bool {
        tsc_command().is_some()
    }

    fn version(&self) -> Option<String> {
        let (cmd, base_args) = tsc_command()?;
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
        if crate::tools::has_config_file(root, &["tsconfig.json"]) {
            1.0
        } else {
            0.0
        }
    }

    fn run(&self, paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError> {
        let (cmd_name, base_args) =
            tsc_command().ok_or_else(|| ToolError::NotAvailable("tsc not found".to_string()))?;

        // tsc --noEmit for type checking only
        // Use --pretty false for machine-readable output
        let mut cmd = Command::new(cmd_name);
        cmd.args(&base_args);
        cmd.arg("--noEmit").arg("--pretty").arg("false");

        // If specific paths provided, we can't easily pass them to tsc
        // tsc works on the whole project based on tsconfig.json
        if !paths.is_empty() {
            // Add files explicitly if no tsconfig
            for path in paths {
                if let Some(p) = path.to_str() {
                    cmd.arg(p);
                }
            }
        }

        let output = cmd.current_dir(root).output()?;

        // tsc outputs to stderr for errors
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let combined = format!("{}{}", stdout, stderr);

        if combined.trim().is_empty() || output.status.success() {
            return Ok(ToolResult::success("tsc", vec![]));
        }

        // Parse tsc output: file(line,col): error TSxxxx: message
        let diagnostics = parse_tsc_output(&combined);

        Ok(ToolResult::success("tsc", diagnostics))
    }
}

/// Parse TypeScript compiler output.
///
/// Format: `file.ts(10,5): error TS2322: Type 'string' is not assignable to type 'number'.`
fn parse_tsc_output(output: &str) -> Vec<Diagnostic> {
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
                        tool: "tsc".to_string(),
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
