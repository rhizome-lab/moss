//! Deno adapter - JavaScript/TypeScript runtime with built-in type checking.
//!
//! Deno is a secure runtime for JavaScript and TypeScript.
//! https://deno.land/

use crate::{
    Diagnostic, DiagnosticSeverity, Location, Tool, ToolCategory, ToolError, ToolInfo, ToolResult,
};
use std::path::Path;
use std::process::Command;

fn deno_command() -> Option<&'static str> {
    // Deno is typically installed globally
    if Command::new("deno").arg("--version").output().is_ok() {
        Some("deno")
    } else {
        None
    }
}

/// Deno type checker adapter.
pub struct Deno;

const DENO_INFO: ToolInfo = ToolInfo {
    name: "deno",
    category: ToolCategory::TypeChecker,
    extensions: &["ts", "tsx", "js", "jsx"],
    check_cmd: &["deno", "--version"],
    website: "https://deno.land/",
};

impl Deno {
    pub fn new() -> Self {
        Self
    }
}

impl Default for Deno {
    fn default() -> Self {
        Self
    }
}

impl Tool for Deno {
    fn info(&self) -> &ToolInfo {
        &DENO_INFO
    }

    fn is_available(&self) -> bool {
        deno_command().is_some()
    }

    fn version(&self) -> Option<String> {
        Command::new("deno")
            .arg("--version")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
    }

    fn detect(&self, root: &Path) -> f32 {
        let mut score = 0.0f32;

        // Deno config files
        let config_files = ["deno.json", "deno.jsonc"];
        if crate::tools::has_config_file(root, &config_files) {
            score += 0.8;
        }

        // Deno lock file
        if root.join("deno.lock").exists() {
            score += 0.2;
        }

        score.min(1.0)
    }

    fn run(&self, paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError> {
        let _cmd =
            deno_command().ok_or_else(|| ToolError::NotAvailable("deno not found".to_string()))?;

        let path_args: Vec<&str> = if paths.is_empty() {
            // Deno check needs explicit files or will check based on deno.json
            vec![]
        } else {
            paths.iter().map(|p| p.to_str().unwrap_or(".")).collect()
        };

        let mut command = Command::new("deno");
        command.arg("check");

        if path_args.is_empty() {
            // Check all based on deno.json
            command.arg(".");
        } else {
            command.args(&path_args);
        }

        let output = command.current_dir(root).output()?;

        // Deno outputs errors to stderr
        let stderr = String::from_utf8_lossy(&output.stderr);

        if stderr.trim().is_empty() || output.status.success() {
            return Ok(ToolResult::success("deno", vec![]));
        }

        // Parse Deno's error format: "error: TS2304 [ERROR]: Cannot find name 'x'."
        // followed by "    at file:///path/to/file.ts:line:col"
        let mut diagnostics = Vec::new();
        let mut current_message: Option<String> = None;
        let mut current_code: Option<String> = None;

        for line in stderr.lines() {
            if line.starts_with("error:") {
                // Extract error code and message
                let rest = line.strip_prefix("error:").unwrap().trim();
                if let Some(bracket_start) = rest.find('[') {
                    let code = rest[..bracket_start].trim().to_string();
                    if let Some(bracket_end) = rest.find(']') {
                        let message = rest[bracket_end + 2..].trim().to_string();
                        current_code = Some(code);
                        current_message = Some(message);
                    }
                } else {
                    current_code = Some("error".to_string());
                    current_message = Some(rest.to_string());
                }
            } else if line.trim().starts_with("at ") && current_message.is_some() {
                // Parse location: "    at file:///path/to/file.ts:10:5"
                let location_part = line.trim().strip_prefix("at ").unwrap();
                if let Some(file_path) = location_part.strip_prefix("file://") {
                    // Parse path:line:col
                    let parts: Vec<&str> = file_path.rsplitn(3, ':').collect();
                    if parts.len() >= 3 {
                        let col: usize = parts[0].parse().unwrap_or(1);
                        let line_num: usize = parts[1].parse().unwrap_or(1);
                        let path = parts[2];

                        diagnostics.push(Diagnostic {
                            tool: "deno".to_string(),
                            rule_id: current_code.take().unwrap_or_else(|| "error".to_string()),
                            message: current_message.take().unwrap_or_default(),
                            severity: DiagnosticSeverity::Error,
                            location: Location {
                                file: path.into(),
                                line: line_num,
                                column: col,
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

        Ok(ToolResult::success("deno", diagnostics))
    }

    fn can_fix(&self) -> bool {
        false
    }

    fn fix(&self, _paths: &[&Path], _root: &Path) -> Result<ToolResult, ToolError> {
        Err(ToolError::FixNotSupported)
    }
}
