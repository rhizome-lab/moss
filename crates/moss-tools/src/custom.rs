//! Custom tool support - run arbitrary commands that output SARIF or JSON.
//!
//! Tools are configured in `.moss/tools.toml`:
//!
//! ```toml
//! [tools.semgrep]
//! command = ["semgrep", "--sarif", "--config=auto", "."]
//! output = "sarif"
//! category = "linter"
//! extensions = ["py", "js", "go"]
//! detect = ["semgrep.yaml", ".semgrep.yml"]
//! website = "https://semgrep.dev"
//!
//! [tools.custom-script]
//! command = ["./scripts/lint.sh"]
//! output = "sarif"
//! category = "linter"
//! ```

use crate::{
    Diagnostic, SarifReport, Tool, ToolCategory, ToolError, ToolInfo, ToolResult, has_config_file,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

/// Configuration for custom tools.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolsConfig {
    #[serde(default)]
    pub tools: HashMap<String, CustomToolConfig>,
}

/// Configuration for a single custom tool.
#[derive(Debug, Clone, Deserialize)]
pub struct CustomToolConfig {
    /// Command to run (first element is executable, rest are args).
    pub command: Vec<String>,

    /// Output format: "sarif" or "json".
    #[serde(default = "default_output")]
    pub output: OutputFormat,

    /// Tool category.
    #[serde(default)]
    pub category: CategoryConfig,

    /// File extensions this tool handles.
    #[serde(default)]
    pub extensions: Vec<String>,

    /// Config files that indicate this tool should be used.
    #[serde(default)]
    pub detect: Vec<String>,

    /// Website/documentation URL.
    #[serde(default)]
    pub website: Option<String>,

    /// Command to check if tool is available (defaults to first command arg with --version).
    #[serde(default)]
    pub check_cmd: Option<Vec<String>>,

    /// Command to run in fix mode (optional).
    #[serde(default)]
    pub fix_command: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Sarif,
    Json,
}

fn default_output() -> OutputFormat {
    OutputFormat::Sarif
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CategoryConfig {
    #[default]
    Linter,
    Formatter,
    TypeChecker,
}

impl From<CategoryConfig> for ToolCategory {
    fn from(c: CategoryConfig) -> Self {
        match c {
            CategoryConfig::Linter => ToolCategory::Linter,
            CategoryConfig::Formatter => ToolCategory::Formatter,
            CategoryConfig::TypeChecker => ToolCategory::TypeChecker,
        }
    }
}

/// A custom tool loaded from configuration.
pub struct CustomTool {
    name: String,
    config: CustomToolConfig,
    info: ToolInfo,
}

impl CustomTool {
    /// Create a custom tool from configuration.
    pub fn new(name: String, config: CustomToolConfig) -> Self {
        // We need to leak the strings to get &'static str for ToolInfo
        // This is acceptable because custom tools are loaded once at startup
        let name_static: &'static str = Box::leak(name.clone().into_boxed_str());
        let website_static: &'static str = config
            .website
            .as_ref()
            .map(|s| Box::leak(s.clone().into_boxed_str()) as &'static str)
            .unwrap_or("");

        let extensions: &'static [&'static str] = Box::leak(
            config
                .extensions
                .iter()
                .map(|s| Box::leak(s.clone().into_boxed_str()) as &'static str)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        );

        // Default check command: first element of command with --version
        let check_cmd: &'static [&'static str] = if let Some(check) = &config.check_cmd {
            Box::leak(
                check
                    .iter()
                    .map(|s| Box::leak(s.clone().into_boxed_str()) as &'static str)
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            )
        } else if !config.command.is_empty() {
            let cmd = Box::leak(config.command[0].clone().into_boxed_str()) as &'static str;
            Box::leak(vec![cmd, "--version"].into_boxed_slice())
        } else {
            &[]
        };

        let info = ToolInfo {
            name: name_static,
            category: config.category.into(),
            extensions,
            check_cmd,
            website: website_static,
        };

        Self { name, config, info }
    }
}

impl Tool for CustomTool {
    fn info(&self) -> &ToolInfo {
        &self.info
    }

    fn is_available(&self) -> bool {
        if self.config.command.is_empty() {
            return false;
        }

        if let Some(check_cmd) = &self.config.check_cmd {
            if check_cmd.is_empty() {
                return false;
            }
            Command::new(&check_cmd[0])
                .args(&check_cmd[1..])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        } else {
            // Default: try running the command with --version
            Command::new(&self.config.command[0])
                .arg("--version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        }
    }

    fn version(&self) -> Option<String> {
        if self.config.command.is_empty() {
            return None;
        }

        let check_cmd = self
            .config
            .check_cmd
            .as_ref()
            .cloned()
            .unwrap_or_else(|| vec![self.config.command[0].clone(), "--version".to_string()]);

        if check_cmd.is_empty() {
            return None;
        }

        Command::new(&check_cmd[0])
            .args(&check_cmd[1..])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.lines().next().unwrap_or("").trim().to_string())
    }

    fn detect(&self, root: &Path) -> f32 {
        // Check for config files specified in detect field
        if !self.config.detect.is_empty() {
            let detect_files: Vec<&str> = self.config.detect.iter().map(|s| s.as_str()).collect();
            if has_config_file(root, &detect_files) {
                return 1.0;
            }
        }
        0.0
    }

    fn run(&self, paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError> {
        run_custom_command(
            &self.name,
            &self.config.command,
            self.config.output,
            paths,
            root,
        )
    }

    fn can_fix(&self) -> bool {
        self.config.fix_command.is_some()
    }

    fn fix(&self, paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError> {
        if let Some(fix_cmd) = &self.config.fix_command {
            run_custom_command(&self.name, fix_cmd, self.config.output, paths, root)
        } else {
            self.run(paths, root)
        }
    }
}

fn run_custom_command(
    tool_name: &str,
    command: &[String],
    output_format: OutputFormat,
    paths: &[&Path],
    root: &Path,
) -> Result<ToolResult, ToolError> {
    if command.is_empty() {
        return Err(ToolError::ExecutionFailed("empty command".to_string()));
    }

    let mut cmd = Command::new(&command[0]);
    cmd.args(&command[1..]);

    // Add paths if provided
    for path in paths {
        if let Some(p) = path.to_str() {
            cmd.arg(p);
        }
    }

    cmd.current_dir(root);

    let output = cmd.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    if stdout.trim().is_empty() {
        return Ok(ToolResult::success(tool_name, vec![]));
    }

    let diagnostics = match output_format {
        OutputFormat::Sarif => {
            let report = SarifReport::from_json(&stdout).map_err(|e| {
                ToolError::ParseError(format!("failed to parse SARIF output: {}", e))
            })?;
            report.to_diagnostics()
        }
        OutputFormat::Json => {
            // Try parsing as array of diagnostics directly
            serde_json::from_str::<Vec<Diagnostic>>(&stdout)
                .map_err(|e| ToolError::ParseError(format!("failed to parse JSON output: {}", e)))?
        }
    };

    Ok(ToolResult::success(tool_name, diagnostics))
}

/// Load custom tools from a config file.
pub fn load_custom_tools(root: &Path) -> Vec<Box<dyn Tool>> {
    let config_path = root.join(".moss").join("tools.toml");

    if !config_path.exists() {
        return Vec::new();
    }

    let content = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let config: ToolsConfig = match toml::from_str(&content) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Warning: failed to parse {}: {}", config_path.display(), e);
            return Vec::new();
        }
    };

    config
        .tools
        .into_iter()
        .map(|(name, tool_config)| Box::new(CustomTool::new(name, tool_config)) as Box<dyn Tool>)
        .collect()
}
