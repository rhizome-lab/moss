//! Rustfmt adapter - Rust formatter.
//!
//! Rustfmt is the official Rust code formatter.
//! https://rust-lang.github.io/rustfmt/

use crate::{
    Diagnostic, DiagnosticSeverity, Location, Tool, ToolCategory, ToolError, ToolInfo, ToolResult,
};
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

/// Rustfmt Rust formatter adapter.
pub struct Rustfmt {
    info: ToolInfo,
}

impl Rustfmt {
    pub fn new() -> Self {
        Self {
            info: ToolInfo {
                name: "rustfmt",
                category: ToolCategory::Formatter,
                extensions: &["rs"],
                check_cmd: &["rustfmt", "--version"],
                website: "https://rust-lang.github.io/rustfmt/",
            },
        }
    }
}

impl Default for Rustfmt {
    fn default() -> Self {
        Self::new()
    }
}

/// Rustfmt JSON output format.
#[derive(Debug, Deserialize)]
struct RustfmtMismatch {
    name: String,
    mismatches: Vec<RustfmtDiff>,
}

#[derive(Debug, Deserialize)]
struct RustfmtDiff {
    original_begin_line: usize,
    original_end_line: usize,
    #[allow(dead_code)]
    expected_begin_line: usize,
    #[allow(dead_code)]
    expected_end_line: usize,
    #[allow(dead_code)]
    original: String,
    #[allow(dead_code)]
    expected: String,
}

impl Tool for Rustfmt {
    fn info(&self) -> &ToolInfo {
        &self.info
    }

    fn is_available(&self) -> bool {
        Command::new("rustfmt")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn version(&self) -> Option<String> {
        Command::new("rustfmt")
            .arg("--version")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
    }

    fn detect(&self, root: &Path) -> f32 {
        if crate::tools::has_config_file(root, &["Cargo.toml"]) {
            1.0
        } else {
            0.0
        }
    }

    fn run(&self, paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError> {
        // Use cargo fmt --check for whole project, or rustfmt --check for specific files
        let output = if paths.is_empty() {
            Command::new("cargo")
                .args(["fmt", "--check", "--message-format=json"])
                .current_dir(root)
                .output()?
        } else {
            let path_args: Vec<&str> = paths.iter().filter_map(|p| p.to_str()).collect();
            Command::new("rustfmt")
                .arg("--check")
                .arg("--emit=json")
                .args(&path_args)
                .current_dir(root)
                .output()?
        };

        // Exit code 0 = formatted, non-zero = needs formatting
        if output.status.success() {
            return Ok(ToolResult::success("rustfmt", vec![]));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Try to parse JSON output
        let mut diagnostics = Vec::new();

        // rustfmt --emit=json outputs array of file mismatches
        if let Ok(mismatches) = serde_json::from_str::<Vec<RustfmtMismatch>>(&stdout) {
            for file in mismatches {
                for mismatch in file.mismatches {
                    diagnostics.push(Diagnostic {
                        tool: "rustfmt".to_string(),
                        rule_id: "formatting".to_string(),
                        message: "File needs formatting".to_string(),
                        severity: DiagnosticSeverity::Warning,
                        location: Location {
                            file: file.name.clone().into(),
                            line: mismatch.original_begin_line,
                            column: 1,
                            end_line: Some(mismatch.original_end_line),
                            end_column: None,
                        },
                        fix: None,
                        help_url: None,
                    });
                }
            }
        } else {
            // Fallback: parse cargo fmt --check output
            // Format: "Diff in /path/to/file.rs at line N:"
            for line in stderr.lines() {
                if line.starts_with("Diff in ") {
                    if let Some(rest) = line.strip_prefix("Diff in ") {
                        if let Some((file, loc)) = rest.rsplit_once(" at line ") {
                            let line_num: usize = loc.trim_end_matches(':').parse().unwrap_or(1);
                            diagnostics.push(Diagnostic {
                                tool: "rustfmt".to_string(),
                                rule_id: "formatting".to_string(),
                                message: "File needs formatting".to_string(),
                                severity: DiagnosticSeverity::Warning,
                                location: Location {
                                    file: file.to_string().into(),
                                    line: line_num,
                                    column: 1,
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
        }

        Ok(ToolResult::success("rustfmt", diagnostics))
    }

    fn can_fix(&self) -> bool {
        true
    }

    fn fix(&self, paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError> {
        // Run cargo fmt or rustfmt to fix
        let output = if paths.is_empty() {
            Command::new("cargo")
                .args(["fmt"])
                .current_dir(root)
                .output()?
        } else {
            let path_args: Vec<&str> = paths.iter().filter_map(|p| p.to_str()).collect();
            Command::new("rustfmt")
                .args(&path_args)
                .current_dir(root)
                .output()?
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Ok(ToolResult::failure("rustfmt", stderr.to_string()));
        }

        Ok(ToolResult::success("rustfmt", vec![]))
    }
}
