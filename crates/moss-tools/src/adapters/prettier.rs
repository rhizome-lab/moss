//! Prettier adapter - code formatter.
//!
//! Prettier is an opinionated code formatter supporting many languages.
//! https://prettier.io/

use crate::{
    Diagnostic, DiagnosticSeverity, Location, Tool, ToolCategory, ToolError, ToolInfo, ToolResult,
};
use std::path::Path;
use std::process::Command;

fn prettier_command() -> Option<(String, Vec<String>)> {
    crate::tools::find_js_tool("prettier", None)
}

/// Prettier formatter adapter.
pub struct Prettier;

const PRETTIER_INFO: ToolInfo = ToolInfo {
    name: "prettier",
    category: ToolCategory::Formatter,
    extensions: &[
        "js", "jsx", "ts", "tsx", "mjs", "cjs", "mts", "cts", "json", "md", "yaml", "yml", "css",
        "scss", "less", "html", "vue", "svelte", "graphql",
    ],
    check_cmd: &["prettier", "--version"],
    website: "https://prettier.io/",
};

impl Prettier {
    pub fn new() -> Self {
        Self
    }
}

impl Default for Prettier {
    fn default() -> Self {
        Self
    }
}

impl Tool for Prettier {
    fn info(&self) -> &ToolInfo {
        &PRETTIER_INFO
    }

    fn is_available(&self) -> bool {
        prettier_command().is_some()
    }

    fn version(&self) -> Option<String> {
        let (cmd, base_args) = prettier_command()?;
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
        let mut score = 0.0f32;

        // Prettier config files - strong signal
        let prettier_configs = [
            ".prettierrc",
            ".prettierrc.json",
            ".prettierrc.yaml",
            ".prettierrc.yml",
            ".prettierrc.js",
            ".prettierrc.cjs",
            "prettier.config.js",
            "prettier.config.cjs",
        ];
        if crate::tools::has_config_file(root, &prettier_configs) {
            score += 0.7;
        }

        // Package.json required - prettier is a JS ecosystem tool
        // Don't run on non-JS projects just because they have .json/.md files
        if crate::tools::has_config_file(root, &["package.json"]) {
            score += 0.3;
        } else {
            // No package.json = not a JS project, don't run prettier
            return 0.0;
        }

        score.min(1.0)
    }

    fn run(&self, paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError> {
        let (cmd, base_args) = prettier_command()
            .ok_or_else(|| ToolError::NotAvailable("prettier not found".to_string()))?;

        let path_args: Vec<&str> = if paths.is_empty() {
            vec!["."]
        } else {
            paths.iter().map(|p| p.to_str().unwrap_or(".")).collect()
        };

        let mut command = Command::new(cmd);
        command.args(&base_args);
        command.arg("--check");

        let output = command.args(&path_args).current_dir(root).output()?;

        // Exit code 0 = all formatted, 1 = some need formatting
        if output.status.success() {
            return Ok(ToolResult::success("prettier", vec![]));
        }

        // Parse output for files needing formatting
        // Format varies, but typically lists files that differ
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}{}", stdout, stderr);

        let diagnostics: Vec<Diagnostic> = combined
            .lines()
            .filter(|line| {
                // Skip info messages
                !line.starts_with("Checking")
                    && !line.starts_with("[warn]")
                    && !line.is_empty()
                    && !line.contains("Code style issues")
            })
            .filter_map(|line| {
                // Files needing formatting are often just listed as paths
                let file = line.trim();
                if file.is_empty() || file.starts_with("error") {
                    return None;
                }

                // Check if it looks like a file path
                if file.contains('.') && !file.contains("prettier") {
                    Some(Diagnostic {
                        tool: "prettier".to_string(),
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

        Ok(ToolResult::success("prettier", diagnostics))
    }

    fn can_fix(&self) -> bool {
        true
    }

    fn fix(&self, paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError> {
        let (cmd, base_args) = prettier_command()
            .ok_or_else(|| ToolError::NotAvailable("prettier not found".to_string()))?;

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
            return Ok(ToolResult::failure("prettier", stderr.to_string()));
        }

        Ok(ToolResult::success("prettier", vec![]))
    }
}
