//! Oxfmt adapter - fast JavaScript/TypeScript formatter.
//!
//! Oxfmt is an extremely fast JavaScript/TypeScript formatter, written in Rust.
//! https://oxc.rs/

use crate::{
    Diagnostic, DiagnosticSeverity, Location, Tool, ToolCategory, ToolError, ToolInfo, ToolResult,
};
use std::path::Path;
use std::process::Command;

fn oxfmt_command() -> Option<(String, Vec<String>)> {
    crate::tools::find_js_tool("oxfmt", None)
}

/// Oxfmt JavaScript/TypeScript formatter adapter.
pub struct Oxfmt;

const OXFMT_INFO: ToolInfo = ToolInfo {
    name: "oxfmt",
    category: ToolCategory::Formatter,
    extensions: &["js", "jsx", "ts", "tsx", "mjs", "cjs", "mts", "cts"],
    check_cmd: &["oxfmt", "--version"],
    website: "https://oxc.rs/",
};

impl Oxfmt {
    pub fn new() -> Self {
        Self
    }
}

impl Default for Oxfmt {
    fn default() -> Self {
        Self
    }
}

impl Tool for Oxfmt {
    fn info(&self) -> &ToolInfo {
        &OXFMT_INFO
    }

    fn is_available(&self) -> bool {
        oxfmt_command().is_some()
    }

    fn version(&self) -> Option<String> {
        let (cmd, base_args) = oxfmt_command()?;
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
        // Oxfmt is a JS ecosystem tool - require package.json
        if !crate::tools::has_config_file(root, &["package.json"]) {
            return 0.0;
        }

        let mut score = 0.3f32;

        // Oxfmt-specific config - strong signal
        if crate::tools::has_config_file(root, &[".oxfmtrc.json", ".oxfmtrc.jsonc", "oxfmt.json"]) {
            score += 0.5;
        }

        // TypeScript config
        if crate::tools::has_config_file(root, &["tsconfig.json", "jsconfig.json"]) {
            score += 0.2;
        }

        score.min(1.0)
    }

    fn run(&self, paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError> {
        let (cmd, base_args) = oxfmt_command()
            .ok_or_else(|| ToolError::NotAvailable("oxfmt not found".to_string()))?;

        let path_args: Vec<&str> = if paths.is_empty() {
            vec!["."]
        } else {
            paths.iter().map(|p| p.to_str().unwrap_or(".")).collect()
        };

        let mut command = Command::new(cmd);
        command.args(&base_args);
        command.arg("--check");

        let output = command.args(&path_args).current_dir(root).output()?;

        // Exit code 0 = all formatted, non-zero = some need formatting
        if output.status.success() {
            return Ok(ToolResult::success("oxfmt", vec![]));
        }

        // Parse output for files needing formatting
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}{}", stdout, stderr);

        let diagnostics: Vec<Diagnostic> = combined
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.contains("oxfmt"))
            .filter_map(|line| {
                // Files needing formatting are typically listed as paths
                let file = line.trim();
                if file.is_empty() {
                    return None;
                }

                // Check if it looks like a file path
                if file.ends_with(".js")
                    || file.ends_with(".jsx")
                    || file.ends_with(".ts")
                    || file.ends_with(".tsx")
                    || file.ends_with(".mjs")
                    || file.ends_with(".cjs")
                {
                    Some(Diagnostic {
                        tool: "oxfmt".to_string(),
                        rule_id: "formatting".to_string(),
                        message: "File needs formatting".to_string(),
                        severity: DiagnosticSeverity::Warning,
                        location: Location {
                            file: file.to_string().into(),
                            line: 1,
                            column: 1,
                            end_line: None,
                            end_column: None,
                        },
                        fix: None,
                        help_url: None,
                    })
                } else {
                    None
                }
            })
            .collect();

        Ok(ToolResult::success("oxfmt", diagnostics))
    }

    fn can_fix(&self) -> bool {
        true
    }

    fn fix(&self, paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError> {
        let (cmd, base_args) = oxfmt_command()
            .ok_or_else(|| ToolError::NotAvailable("oxfmt not found".to_string()))?;

        let path_args: Vec<&str> = if paths.is_empty() {
            vec!["."]
        } else {
            paths.iter().map(|p| p.to_str().unwrap_or(".")).collect()
        };

        let mut command = Command::new(cmd);
        command.args(&base_args);
        command.arg("--write");

        let output = command.args(&path_args).current_dir(root).output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Ok(ToolResult::failure("oxfmt", stderr.to_string()));
        }

        Ok(ToolResult::success("oxfmt", vec![]))
    }
}
