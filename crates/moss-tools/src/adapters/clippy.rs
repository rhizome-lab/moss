//! Clippy adapter - Rust linter.
//!
//! Clippy is the official Rust linter that catches common mistakes and improves code.
//! https://doc.rust-lang.org/clippy/

use crate::{
    Diagnostic, DiagnosticSeverity, Location, Tool, ToolCategory, ToolError, ToolInfo, ToolResult,
};
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

/// Clippy Rust linter adapter.
pub struct Clippy {
    info: ToolInfo,
}

impl Clippy {
    pub fn new() -> Self {
        Self {
            info: ToolInfo {
                name: "clippy",
                category: ToolCategory::Linter,
                extensions: &["rs"],
                check_cmd: &["cargo", "clippy", "--version"],
                website: "https://doc.rust-lang.org/clippy/",
            },
        }
    }
}

impl Default for Clippy {
    fn default() -> Self {
        Self::new()
    }
}

/// Cargo/Clippy JSON message format.
#[derive(Debug, Deserialize)]
struct CargoMessage {
    reason: String,
    message: Option<CompilerMessage>,
}

#[derive(Debug, Deserialize)]
struct CompilerMessage {
    code: Option<DiagnosticCode>,
    level: String,
    message: String,
    spans: Vec<CompilerSpan>,
    #[allow(dead_code)]
    rendered: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DiagnosticCode {
    code: String,
    #[allow(dead_code)]
    explanation: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CompilerSpan {
    file_name: String,
    line_start: usize,
    line_end: usize,
    column_start: usize,
    column_end: usize,
    is_primary: bool,
    #[allow(dead_code)]
    label: Option<String>,
    #[allow(dead_code)]
    suggested_replacement: Option<String>,
}

impl Tool for Clippy {
    fn info(&self) -> &ToolInfo {
        &self.info
    }

    fn is_available(&self) -> bool {
        Command::new("cargo")
            .args(["clippy", "--version"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn version(&self) -> Option<String> {
        Command::new("cargo")
            .args(["clippy", "--version"])
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

    fn run(&self, _paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError> {
        // Clippy works on the whole project
        let output = Command::new("cargo")
            .args(["clippy", "--message-format=json", "--", "-W", "clippy::all"])
            .current_dir(root)
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        let mut diagnostics = Vec::new();

        for line in stdout.lines() {
            if line.trim().is_empty() {
                continue;
            }

            if let Ok(msg) = serde_json::from_str::<CargoMessage>(line) {
                if msg.reason != "compiler-message" {
                    continue;
                }

                if let Some(compiler_msg) = msg.message {
                    // Skip notes and help messages
                    if compiler_msg.level == "note" || compiler_msg.level == "help" {
                        continue;
                    }

                    // Get primary span
                    if let Some(span) = compiler_msg.spans.iter().find(|s| s.is_primary) {
                        let severity = match compiler_msg.level.as_str() {
                            "error" => DiagnosticSeverity::Error,
                            "warning" => DiagnosticSeverity::Warning,
                            _ => DiagnosticSeverity::Warning,
                        };

                        let rule_id = compiler_msg
                            .code
                            .map(|c| c.code)
                            .unwrap_or_else(|| "unknown".to_string());

                        diagnostics.push(Diagnostic {
                            tool: "clippy".to_string(),
                            rule_id,
                            message: compiler_msg.message,
                            severity,
                            location: Location {
                                file: span.file_name.clone().into(),
                                line: span.line_start,
                                column: span.column_start,
                                end_line: Some(span.line_end),
                                end_column: Some(span.column_end),
                            },
                            fix: None,
                            help_url: None,
                        });
                    }
                }
            }
        }

        Ok(ToolResult::success("clippy", diagnostics))
    }

    fn can_fix(&self) -> bool {
        true
    }

    fn fix(&self, _paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError> {
        // Run clippy --fix
        let output = Command::new("cargo")
            .args([
                "clippy",
                "--fix",
                "--allow-dirty",
                "--allow-staged",
                "--message-format=json",
            ])
            .current_dir(root)
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        let mut diagnostics = Vec::new();

        // Parse remaining unfixed warnings
        for line in stdout.lines() {
            if let Ok(msg) = serde_json::from_str::<CargoMessage>(line) {
                if msg.reason != "compiler-message" {
                    continue;
                }

                if let Some(compiler_msg) = msg.message {
                    if compiler_msg.level == "note" || compiler_msg.level == "help" {
                        continue;
                    }

                    if let Some(span) = compiler_msg.spans.iter().find(|s| s.is_primary) {
                        diagnostics.push(Diagnostic {
                            tool: "clippy".to_string(),
                            rule_id: compiler_msg
                                .code
                                .map(|c| c.code)
                                .unwrap_or_else(|| "unknown".to_string()),
                            message: compiler_msg.message,
                            severity: DiagnosticSeverity::Warning,
                            location: Location {
                                file: span.file_name.clone().into(),
                                line: span.line_start,
                                column: span.column_start,
                                end_line: Some(span.line_end),
                                end_column: Some(span.column_end),
                            },
                            fix: None,
                            help_url: None,
                        });
                    }
                }
            }
        }

        Ok(ToolResult::success("clippy", diagnostics))
    }
}
