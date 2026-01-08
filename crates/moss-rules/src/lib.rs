//! Syntax-based linting with tree-sitter queries.
//!
//! This crate provides:
//! - Rule loading from multiple sources (builtins, user global, project)
//! - Rule execution with combined query optimization
//! - Pluggable data sources for rule conditionals
//!
//! # Rule File Format
//!
//! ```scm
//! # ---
//! # id = "no-unwrap"
//! # severity = "warning"
//! # message = "Avoid unwrap() on user input"
//! # allow = ["**/tests/**"]
//! # requires = { "rust.edition" = ">=2024" }
//! # enabled = true  # set to false to disable a builtin
//! # fix = ""  # empty = delete match, or use "$capture" to substitute
//! # ---
//!
//! (call_expression
//!   function: (field_expression
//!     field: (field_identifier) @method)
//!   (#eq? @method "unwrap")) @match
//! ```

mod builtin;
mod loader;
mod runner;
mod sources;

pub use builtin::BUILTIN_RULES;
pub use loader::{RuleOverride, RulesConfig, load_all_rules, parse_rule_content};
pub use runner::{DebugFlags, Finding, apply_fixes, evaluate_predicates, run_rules};
pub use sources::{
    EnvSource, GitSource, GoSource, PathSource, PythonSource, RuleSource, RustSource,
    SourceContext, SourceRegistry, TypeScriptSource, builtin_registry,
};

use glob::Pattern;
use std::collections::HashMap;
use std::path::PathBuf;

/// Severity level for rule findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Severity {
    Error,
    #[default]
    Warning,
    Info,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
            Severity::Warning => write!(f, "warning"),
            Severity::Info => write!(f, "info"),
        }
    }
}

impl std::str::FromStr for Severity {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "error" => Ok(Severity::Error),
            "warning" | "warn" => Ok(Severity::Warning),
            "info" | "note" => Ok(Severity::Info),
            _ => Err(format!("unknown severity: {}", s)),
        }
    }
}

/// A syntax rule definition.
#[derive(Debug)]
pub struct Rule {
    /// Unique identifier for this rule.
    pub id: String,
    /// The tree-sitter query pattern.
    pub query_str: String,
    /// Severity level.
    pub severity: Severity,
    /// Message to display when the rule matches.
    pub message: String,
    /// Glob patterns for files where matches are allowed.
    pub allow: Vec<Pattern>,
    /// Source file path of this rule (empty for builtins).
    pub source_path: PathBuf,
    /// Languages this rule applies to (inferred from query or explicit).
    pub languages: Vec<String>,
    /// Whether this rule is enabled.
    pub enabled: bool,
    /// Whether this is a builtin rule.
    pub builtin: bool,
    /// Conditions that must be met for this rule to apply.
    /// Format: { "namespace.key" = "value" } or { "namespace.key" = ">=value" }
    pub requires: HashMap<String, String>,
    /// Auto-fix template using capture names from the query.
    /// Use `$capture_name` to reference captures, `$match` for the full match.
    /// Empty string means "delete the match".
    pub fix: Option<String>,
}

/// A builtin rule definition (id, content).
pub struct BuiltinRule {
    pub id: &'static str,
    pub content: &'static str,
}
