//! Go fmt adapter - Go formatter.
//!
//! gofmt is the official Go code formatter.
//! https://pkg.go.dev/cmd/gofmt

use crate::{
    Diagnostic, DiagnosticSeverity, Location, Tool, ToolCategory, ToolError, ToolInfo, ToolResult,
};
use std::path::Path;
use std::process::Command;

/// Go formatter adapter.
pub struct Gofmt {
    info: ToolInfo,
}

impl Gofmt {
    pub fn new() -> Self {
        Self {
            info: ToolInfo {
                name: "gofmt",
                category: ToolCategory::Formatter,
                extensions: &["go"],
                check_cmd: &["gofmt", "-h"],
                website: "https://pkg.go.dev/cmd/gofmt",
            },
        }
    }
}

impl Default for Gofmt {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for Gofmt {
    fn info(&self) -> &ToolInfo {
        &self.info
    }

    fn is_available(&self) -> bool {
        Command::new("gofmt")
            .arg("-h")
            .output()
            .map(|_| true) // gofmt -h always exits 0
            .unwrap_or(false)
    }

    fn version(&self) -> Option<String> {
        // gofmt doesn't have --version, use go version instead
        Command::new("go")
            .arg("version")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
    }

    fn detect(&self, root: &Path) -> f32 {
        if crate::tools::has_config_file(root, &["go.mod"]) {
            1.0
        } else {
            0.0
        }
    }

    fn run(&self, paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError> {
        // gofmt -l lists files that need formatting
        let path_args: Vec<&str> = if paths.is_empty() {
            vec!["."]
        } else {
            paths.iter().map(|p| p.to_str().unwrap_or(".")).collect()
        };

        let output = Command::new("gofmt")
            .arg("-l")
            .args(&path_args)
            .current_dir(root)
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        if stdout.trim().is_empty() {
            return Ok(ToolResult::success("gofmt", vec![]));
        }

        // Each line is a file that needs formatting
        let diagnostics: Vec<Diagnostic> = stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|file| Diagnostic {
                tool: "gofmt".to_string(),
                rule_id: "formatting".to_string(),
                message: "File needs formatting".to_string(),
                severity: DiagnosticSeverity::Warning,
                location: Location {
                    file: file.trim().to_string().into(),
                    line: 1,
                    column: 1,
                    end_line: None,
                    end_column: None,
                },
                fix: None,
                help_url: None,
            })
            .collect();

        Ok(ToolResult::success("gofmt", diagnostics))
    }

    fn can_fix(&self) -> bool {
        true
    }

    fn fix(&self, paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError> {
        // gofmt -w writes formatted output back to files
        let path_args: Vec<&str> = if paths.is_empty() {
            vec!["."]
        } else {
            paths.iter().map(|p| p.to_str().unwrap_or(".")).collect()
        };

        let output = Command::new("gofmt")
            .arg("-w")
            .args(&path_args)
            .current_dir(root)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Ok(ToolResult::failure("gofmt", stderr.to_string()));
        }

        Ok(ToolResult::success("gofmt", vec![]))
    }
}

/// Go vet adapter - Go static analyzer.
pub struct Govet {
    info: ToolInfo,
}

impl Govet {
    pub fn new() -> Self {
        Self {
            info: ToolInfo {
                name: "go-vet",
                category: ToolCategory::Linter,
                extensions: &["go"],
                check_cmd: &["go", "vet", "-h"],
                website: "https://pkg.go.dev/cmd/vet",
            },
        }
    }
}

impl Default for Govet {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for Govet {
    fn info(&self) -> &ToolInfo {
        &self.info
    }

    fn is_available(&self) -> bool {
        Command::new("go")
            .args(["vet", "-h"])
            .output()
            .map(|_| true)
            .unwrap_or(false)
    }

    fn version(&self) -> Option<String> {
        Command::new("go")
            .arg("version")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
    }

    fn detect(&self, root: &Path) -> f32 {
        if crate::tools::has_config_file(root, &["go.mod"]) {
            1.0
        } else {
            0.0
        }
    }

    fn run(&self, paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError> {
        let path_args: Vec<&str> = if paths.is_empty() {
            vec!["./..."]
        } else {
            paths.iter().map(|p| p.to_str().unwrap_or(".")).collect()
        };

        let output = Command::new("go")
            .arg("vet")
            .args(&path_args)
            .current_dir(root)
            .output()?;

        // go vet outputs to stderr
        let stderr = String::from_utf8_lossy(&output.stderr);

        if stderr.trim().is_empty() || output.status.success() {
            return Ok(ToolResult::success("go-vet", vec![]));
        }

        // Parse go vet output: file.go:line:col: message
        let diagnostics = parse_go_vet_output(&stderr);

        Ok(ToolResult::success("go-vet", diagnostics))
    }
}

fn parse_go_vet_output(output: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for line in output.lines() {
        // Format: file.go:line:col: message
        // or: file.go:line: message
        let parts: Vec<&str> = line.splitn(4, ':').collect();
        if parts.len() >= 3 {
            let file = parts[0];
            let line_num: usize = parts[1].parse().unwrap_or(1);
            let (column, message) = if parts.len() == 4 {
                (parts[2].trim().parse().unwrap_or(1), parts[3].trim())
            } else {
                (1, parts[2].trim())
            };

            diagnostics.push(Diagnostic {
                tool: "go-vet".to_string(),
                rule_id: "vet".to_string(),
                message: message.to_string(),
                severity: DiagnosticSeverity::Warning,
                location: Location {
                    file: file.to_string().into(),
                    line: line_num,
                    column,
                    end_line: None,
                    end_column: None,
                },
                fix: None,
                help_url: None,
            });
        }
    }

    diagnostics
}
