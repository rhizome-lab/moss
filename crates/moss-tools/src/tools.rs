//! Tool trait and common types.

use crate::Diagnostic;
use std::path::Path;
use thiserror::Error;

/// Category of tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolCategory {
    /// Finds code issues (eslint, oxlint, ruff check, clippy).
    Linter,
    /// Checks/fixes code style (prettier, black, rustfmt, gofmt).
    Formatter,
    /// Finds type errors (tsc, mypy, pyright, cargo check).
    TypeChecker,
}

impl ToolCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Linter => "linter",
            Self::Formatter => "formatter",
            Self::TypeChecker => "type-checker",
        }
    }
}

/// Information about a tool.
#[derive(Debug, Clone)]
pub struct ToolInfo {
    /// Tool name (e.g., "oxlint", "ruff", "prettier").
    pub name: &'static str,
    /// Tool category.
    pub category: ToolCategory,
    /// File extensions this tool handles (e.g., ["js", "ts", "jsx", "tsx"]).
    pub extensions: &'static [&'static str],
    /// Command to check if tool is available.
    pub check_cmd: &'static [&'static str],
    /// URL to tool website.
    pub website: &'static str,
}

/// Result of running a tool.
#[derive(Debug, Clone)]
pub struct ToolResult {
    /// Tool that produced this result.
    pub tool: String,
    /// Diagnostics found.
    pub diagnostics: Vec<Diagnostic>,
    /// Whether the tool ran successfully.
    pub success: bool,
    /// Optional error message if tool failed.
    pub error: Option<String>,
}

impl ToolResult {
    pub fn success(tool: &str, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            tool: tool.to_string(),
            diagnostics,
            success: true,
            error: None,
        }
    }

    pub fn failure(tool: &str, error: impl ToString) -> Self {
        Self {
            tool: tool.to_string(),
            diagnostics: Vec::new(),
            success: false,
            error: Some(error.to_string()),
        }
    }

    pub fn error_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == crate::DiagnosticSeverity::Error)
            .count()
    }

    pub fn warning_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == crate::DiagnosticSeverity::Warning)
            .count()
    }
}

/// Error type for tool operations.
#[derive(Debug, Error)]
pub enum ToolError {
    #[error("tool not found: {0}")]
    NotFound(String),
    #[error("tool execution failed: {0}")]
    ExecutionFailed(String),
    #[error("failed to parse tool output: {0}")]
    ParseError(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Trait for tool adapters.
///
/// Each tool (oxlint, ruff, prettier, etc.) implements this trait.
pub trait Tool: Send + Sync {
    /// Get tool information.
    fn info(&self) -> &ToolInfo;

    /// Check if the tool is available on the system.
    fn is_available(&self) -> bool;

    /// Get the tool version, if available.
    fn version(&self) -> Option<String>;

    /// Detect if this tool is relevant for the given project.
    ///
    /// Checks for:
    /// - Config files (tsconfig.json, pyproject.toml, Cargo.toml, etc.)
    /// - Source files matching the tool's extensions
    /// - Lock files or other indicators
    ///
    /// Returns a confidence score (0.0 = not relevant, 1.0 = definitely relevant).
    fn detect(&self, root: &Path) -> f32;

    /// Run the tool on the given paths.
    ///
    /// # Arguments
    /// * `paths` - Files or directories to check.
    /// * `root` - Working directory for the tool.
    fn run(&self, paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError>;

    /// Whether this tool can fix issues automatically.
    fn can_fix(&self) -> bool {
        false
    }

    /// Run the tool in fix mode (if supported).
    fn fix(&self, paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError> {
        // Default: just run without fixing
        self.run(paths, root)
    }
}

/// Helper to check if any files with given extensions exist.
pub fn has_files_with_extensions(root: &Path, extensions: &[&str]) -> bool {
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if extensions.contains(&ext) {
                        return true;
                    }
                }
            } else if path.is_dir()
                && !path
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with('.'))
            {
                if has_files_with_extensions(&path, extensions) {
                    return true;
                }
            }
        }
    }
    false
}

/// Helper to check if a config file exists.
pub fn has_config_file(root: &Path, names: &[&str]) -> bool {
    names.iter().any(|name| root.join(name).exists())
}
