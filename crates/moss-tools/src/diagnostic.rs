//! Diagnostic types shared across all tools.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Severity levels for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    /// Fatal error, must be fixed.
    Error,
    /// Potential problem, should be fixed.
    Warning,
    /// Informational message.
    Info,
    /// Suggestion or hint.
    Hint,
}

impl DiagnosticSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
            Self::Hint => "hint",
        }
    }

    /// Convert to SARIF level string.
    pub fn to_sarif_level(&self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info | Self::Hint => "note",
        }
    }
}

/// Source location of a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    /// File path (relative or absolute).
    pub file: PathBuf,
    /// 1-based line number.
    pub line: usize,
    /// 1-based column number.
    pub column: usize,
    /// End line (for ranges).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,
    /// End column (for ranges).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_column: Option<usize>,
}

impl Location {
    pub fn new(file: impl Into<PathBuf>, line: usize, column: usize) -> Self {
        Self {
            file: file.into(),
            line,
            column,
            end_line: None,
            end_column: None,
        }
    }

    pub fn with_end(mut self, end_line: usize, end_column: usize) -> Self {
        self.end_line = Some(end_line);
        self.end_column = Some(end_column);
        self
    }
}

/// A single diagnostic from a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Which tool produced this diagnostic.
    pub tool: String,
    /// Rule ID (e.g., "no-unused-vars", "E501", "clippy::unwrap_used").
    pub rule_id: String,
    /// Human-readable message.
    pub message: String,
    /// Severity level.
    pub severity: DiagnosticSeverity,
    /// Location in source.
    pub location: Location,
    /// Optional suggested fix (as replacement text or diff).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<Fix>,
    /// Optional URL to rule documentation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help_url: Option<String>,
}

/// A suggested fix for a diagnostic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fix {
    /// Description of the fix.
    pub description: String,
    /// The replacement text.
    pub replacement: String,
}

impl Diagnostic {
    pub fn error(tool: &str, rule_id: &str, message: &str, location: Location) -> Self {
        Self {
            tool: tool.to_string(),
            rule_id: rule_id.to_string(),
            message: message.to_string(),
            severity: DiagnosticSeverity::Error,
            location,
            fix: None,
            help_url: None,
        }
    }

    pub fn warning(tool: &str, rule_id: &str, message: &str, location: Location) -> Self {
        Self {
            tool: tool.to_string(),
            rule_id: rule_id.to_string(),
            message: message.to_string(),
            severity: DiagnosticSeverity::Warning,
            location,
            fix: None,
            help_url: None,
        }
    }

    pub fn with_fix(mut self, description: &str, replacement: &str) -> Self {
        self.fix = Some(Fix {
            description: description.to_string(),
            replacement: replacement.to_string(),
        });
        self
    }

    pub fn with_help_url(mut self, url: &str) -> Self {
        self.help_url = Some(url.to_string());
        self
    }
}
