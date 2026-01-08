//! Biome adapter - JavaScript/TypeScript linter and formatter.
//!
//! Biome is a fast formatter and linter for JavaScript, TypeScript, JSX, and JSON.
//! https://biomejs.dev/

use crate::{
    Diagnostic, DiagnosticSeverity, Location, Tool, ToolCategory, ToolError, ToolInfo, ToolResult,
};
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

fn biome_command() -> Option<(String, Vec<String>)> {
    // biome binary comes from the "@biomejs/biome" package
    crate::tools::find_js_tool("biome", Some("@biomejs/biome"))
}

/// Biome linter adapter.
pub struct BiomeLint;

const BIOME_LINT_INFO: ToolInfo = ToolInfo {
    name: "biome",
    category: ToolCategory::Linter,
    extensions: &["js", "jsx", "ts", "tsx", "mjs", "cjs", "mts", "cts", "json"],
    check_cmd: &["biome", "--version"],
    website: "https://biomejs.dev/",
};

impl BiomeLint {
    pub fn new() -> Self {
        Self
    }
}

impl Default for BiomeLint {
    fn default() -> Self {
        Self
    }
}

/// Biome formatter adapter.
pub struct BiomeFormat;

const BIOME_FORMAT_INFO: ToolInfo = ToolInfo {
    name: "biome-fmt",
    category: ToolCategory::Formatter,
    extensions: &["js", "jsx", "ts", "tsx", "mjs", "cjs", "mts", "cts", "json"],
    check_cmd: &["biome", "--version"],
    website: "https://biomejs.dev/",
};

impl BiomeFormat {
    pub fn new() -> Self {
        Self
    }
}

impl Default for BiomeFormat {
    fn default() -> Self {
        Self
    }
}

/// Biome JSON output format.
#[derive(Debug, Deserialize)]
struct BiomeOutput {
    #[serde(default)]
    diagnostics: Vec<BiomeDiagnostic>,
}

#[derive(Debug, Deserialize)]
struct BiomeDiagnostic {
    category: Option<String>,
    message: String,
    severity: String,
    location: Option<BiomeLocation>,
    #[serde(default)]
    #[allow(dead_code)]
    tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct BiomeLocation {
    path: Option<BiomePath>,
    #[allow(dead_code)]
    span: Option<BiomeSpan>,
}

#[derive(Debug, Deserialize)]
struct BiomePath {
    file: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct BiomeSpan {
    start: usize,
    end: usize,
}

fn detect_biome(root: &Path, _extensions: &[&str]) -> f32 {
    // Biome is a JS ecosystem tool - require package.json or biome config
    let has_biome_config = crate::tools::has_config_file(root, &["biome.json", "biome.jsonc"]);
    let has_package_json = crate::tools::has_config_file(root, &["package.json"]);

    if !has_biome_config && !has_package_json {
        return 0.0;
    }

    let mut score = 0.0f32;

    // Biome-specific config - strong signal
    if has_biome_config {
        score += 0.7;
    }

    // Package.json indicates JS/TS project
    if has_package_json {
        score += 0.3;
    }

    score.min(1.0)
}

fn is_biome_available() -> bool {
    biome_command().is_some()
}

fn biome_version() -> Option<String> {
    let (cmd, base_args) = biome_command()?;
    let mut command = Command::new(cmd);
    command.args(&base_args).arg("--version");
    command
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
}

fn parse_biome_output(stdout: &str) -> Result<Vec<Diagnostic>, ToolError> {
    if stdout.trim().is_empty() {
        return Ok(vec![]);
    }

    let output: BiomeOutput = serde_json::from_str(stdout)
        .map_err(|e| ToolError::ParseError(format!("failed to parse biome output: {}", e)))?;

    let diagnostics = output
        .diagnostics
        .into_iter()
        .filter_map(|d| {
            let loc = d.location?;
            let path = loc.path?;

            let severity = match d.severity.as_str() {
                "error" | "fatal" => DiagnosticSeverity::Error,
                "warning" => DiagnosticSeverity::Warning,
                _ => DiagnosticSeverity::Warning,
            };

            // Biome uses byte offsets, not line/column
            // We'd need to read the file to convert, so we use 0,0 as placeholder
            let (line, column) = (1, 1);

            Some(Diagnostic {
                tool: "biome".to_string(),
                rule_id: d.category.unwrap_or_else(|| "unknown".to_string()),
                message: d.message,
                severity,
                location: Location {
                    file: path.file.into(),
                    line,
                    column,
                    end_line: None,
                    end_column: None,
                },
                fix: None,
                help_url: None,
            })
        })
        .collect();

    Ok(diagnostics)
}

impl Tool for BiomeLint {
    fn info(&self) -> &ToolInfo {
        &BIOME_LINT_INFO
    }

    fn is_available(&self) -> bool {
        is_biome_available()
    }

    fn version(&self) -> Option<String> {
        biome_version()
    }

    fn detect(&self, root: &Path) -> f32 {
        detect_biome(root, BIOME_LINT_INFO.extensions)
    }

    fn run(&self, paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError> {
        let (cmd, base_args) = biome_command()
            .ok_or_else(|| ToolError::NotAvailable("biome not found".to_string()))?;

        let path_args: Vec<&str> = if paths.is_empty() {
            vec!["."]
        } else {
            paths.iter().map(|p| p.to_str().unwrap_or(".")).collect()
        };

        let mut command = Command::new(cmd);
        command.args(&base_args);
        command.arg("lint").arg("--reporter=json");

        let output = command.args(&path_args).current_dir(root).output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let diagnostics = parse_biome_output(&stdout)?;

        Ok(ToolResult::success("biome", diagnostics))
    }

    fn can_fix(&self) -> bool {
        true
    }

    fn fix(&self, paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError> {
        let (cmd, base_args) = biome_command()
            .ok_or_else(|| ToolError::NotAvailable("biome not found".to_string()))?;

        let path_args: Vec<&str> = if paths.is_empty() {
            vec!["."]
        } else {
            paths.iter().map(|p| p.to_str().unwrap_or(".")).collect()
        };

        let mut command = Command::new(cmd);
        command.args(&base_args);
        command.arg("lint").arg("--write").arg("--reporter=json");

        let output = command.args(&path_args).current_dir(root).output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let diagnostics = parse_biome_output(&stdout)?;

        Ok(ToolResult::success("biome", diagnostics))
    }
}

impl Tool for BiomeFormat {
    fn info(&self) -> &ToolInfo {
        &BIOME_FORMAT_INFO
    }

    fn is_available(&self) -> bool {
        is_biome_available()
    }

    fn version(&self) -> Option<String> {
        biome_version()
    }

    fn detect(&self, root: &Path) -> f32 {
        detect_biome(root, BIOME_FORMAT_INFO.extensions)
    }

    fn run(&self, paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError> {
        let (cmd, base_args) = biome_command()
            .ok_or_else(|| ToolError::NotAvailable("biome not found".to_string()))?;

        let path_args: Vec<&str> = if paths.is_empty() {
            vec!["."]
        } else {
            paths.iter().map(|p| p.to_str().unwrap_or(".")).collect()
        };

        let mut command = Command::new(cmd);
        command.args(&base_args);
        command.arg("format").arg("--reporter=json");

        let output = command.args(&path_args).current_dir(root).output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let diagnostics = parse_biome_output(&stdout)?;

        Ok(ToolResult::success("biome-fmt", diagnostics))
    }

    fn can_fix(&self) -> bool {
        true
    }

    fn fix(&self, paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError> {
        let (cmd, base_args) = biome_command()
            .ok_or_else(|| ToolError::NotAvailable("biome not found".to_string()))?;

        let path_args: Vec<&str> = if paths.is_empty() {
            vec!["."]
        } else {
            paths.iter().map(|p| p.to_str().unwrap_or(".")).collect()
        };

        let mut command = Command::new(cmd);
        command.args(&base_args);
        command.arg("format").arg("--write").arg("--reporter=json");

        let output = command.args(&path_args).current_dir(root).output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let diagnostics = parse_biome_output(&stdout)?;

        Ok(ToolResult::success("biome-fmt", diagnostics))
    }
}
