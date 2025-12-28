//! Output formatting utilities.
//!
//! Provides consistent JSON/text output across all commands via the `OutputFormatter` trait.

use crate::merge::Merge;
use serde::{Deserialize, Serialize};
use std::io::IsTerminal;

/// Color output mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorMode {
    /// Auto-detect based on TTY (default)
    #[default]
    Auto,
    /// Always use colors
    Always,
    /// Never use colors
    Never,
}

impl Merge for ColorMode {
    fn merge(self, other: Self) -> Self {
        other
    }
}

/// Configuration for pretty output mode.
///
/// Example config.toml:
/// ```toml
/// [pretty]
/// enabled = true       # auto-enable when TTY (default: auto)
/// colors = "auto"      # "auto", "always", or "never"
/// highlight = true     # syntax highlighting on signatures
/// ```
#[derive(Debug, Clone, Deserialize, Merge, Default)]
#[serde(default)]
pub struct PrettyConfig {
    /// Enable pretty mode. None = auto (true when stdout is TTY)
    pub enabled: Option<bool>,
    /// Color mode: auto (default), always, or never
    pub colors: Option<ColorMode>,
    /// Enable syntax highlighting. Default: true
    pub highlight: Option<bool>,
}

impl PrettyConfig {
    /// Should pretty mode be enabled?
    /// Respects explicit setting, otherwise auto-detects TTY.
    pub fn enabled(&self) -> bool {
        self.enabled
            .unwrap_or_else(|| std::io::stdout().is_terminal())
    }

    /// Should colors be used?
    /// Respects colors setting and NO_COLOR env var.
    pub fn use_colors(&self) -> bool {
        // Check NO_COLOR env var first (standard)
        if std::env::var("NO_COLOR").is_ok() {
            return false;
        }

        match self.colors.unwrap_or_default() {
            ColorMode::Always => true,
            ColorMode::Never => false,
            ColorMode::Auto => std::io::stdout().is_terminal(),
        }
    }

    /// Should syntax highlighting be used?
    pub fn highlight(&self) -> bool {
        self.highlight.unwrap_or(true)
    }
}

/// Output format and display mode.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum OutputFormat {
    /// Compact text output (LLM-optimized, no colors).
    #[default]
    Compact,
    /// Pretty text output (human-friendly, with colors if available).
    Pretty { colors: bool },
    /// JSON output.
    Json,
    /// JSON filtered through jq expression.
    Jq(String),
}

impl OutputFormat {
    /// Create from CLI flags and config (fully resolved).
    pub fn from_cli(
        json: bool,
        jq: Option<&str>,
        pretty: bool,
        compact: bool,
        config: &PrettyConfig,
    ) -> Self {
        // JSON modes take precedence
        if let Some(filter) = jq {
            return OutputFormat::Jq(filter.to_string());
        }
        if json {
            return OutputFormat::Json;
        }

        // Determine text mode
        let is_pretty = if compact {
            false
        } else {
            pretty || config.enabled()
        };

        if is_pretty {
            OutputFormat::Pretty {
                colors: config.use_colors(),
            }
        } else {
            OutputFormat::Compact
        }
    }

    /// Is this a JSON-based format?
    pub fn is_json(&self) -> bool {
        matches!(self, OutputFormat::Json | OutputFormat::Jq(_))
    }

    /// Is this pretty mode?
    pub fn is_pretty(&self) -> bool {
        matches!(self, OutputFormat::Pretty { .. })
    }

    /// Are colors enabled?
    pub fn use_colors(&self) -> bool {
        matches!(self, OutputFormat::Pretty { colors: true })
    }
}

/// Trait for types that can format output in multiple formats.
///
/// Types implementing this trait can be printed as either JSON or text.
/// JSON serialization uses serde, while text formatting is custom.
pub trait OutputFormatter: Serialize {
    /// Format as minimal text (LLM-optimized, default).
    fn format_text(&self) -> String;

    /// Format as pretty text (human-friendly with colors).
    /// Default implementation falls back to format_text().
    fn format_pretty(&self) -> String {
        self.format_text()
    }

    /// Print to stdout in the specified format.
    fn print(&self, format: &OutputFormat) {
        match format {
            OutputFormat::Compact => println!("{}", self.format_text()),
            OutputFormat::Pretty { .. } => println!("{}", self.format_pretty()),
            OutputFormat::Json => {
                println!("{}", serde_json::to_string(self).unwrap_or_default())
            }
            OutputFormat::Jq(filter) => {
                let json = serde_json::to_value(self).unwrap_or_default();
                match apply_jq(&json, filter) {
                    Ok(results) => {
                        for result in results {
                            println!("{}", result);
                        }
                    }
                    Err(e) => {
                        eprintln!("jq error: {}", e);
                    }
                }
            }
        }
    }
}

/// Apply a jq filter to a JSON value.
pub fn apply_jq(value: &serde_json::Value, filter: &str) -> Result<Vec<String>, String> {
    use jaq_core::load::{Arena, File as JaqFile, Loader};
    use jaq_core::{Compiler, Ctx, RcIter};
    use jaq_json::Val;

    let loader = Loader::new(jaq_std::defs().chain(jaq_json::defs()));
    let arena = Arena::default();

    let program = JaqFile {
        code: filter,
        path: (),
    };

    let modules = loader
        .load(&arena, program)
        .map_err(|errs| format!("jq parse error: {:?}", errs))?;

    let filter_compiled = Compiler::default()
        .with_funs(jaq_std::funs().chain(jaq_json::funs()))
        .compile(modules)
        .map_err(|errs| format!("jq compile error: {:?}", errs))?;

    let val = Val::from(value.clone());
    let inputs = RcIter::new(core::iter::empty());
    let out = filter_compiled.run((Ctx::new([], &inputs), val));

    let mut results = Vec::new();
    for result in out {
        match result {
            Ok(v) => results.push(v.to_string()),
            Err(e) => return Err(format!("jq runtime error: {:?}", e)),
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct TestOutput {
        name: String,
        count: usize,
    }

    impl OutputFormatter for TestOutput {
        fn format_text(&self) -> String {
            format!("{}: {}", self.name, self.count)
        }
    }

    #[test]
    fn test_output_format_from_cli() {
        let config = PrettyConfig::default();
        // compact=true overrides auto
        assert_eq!(
            OutputFormat::from_cli(false, None, false, true, &config),
            OutputFormat::Compact
        );
        assert_eq!(
            OutputFormat::from_cli(true, None, false, false, &config),
            OutputFormat::Json
        );
        assert_eq!(
            OutputFormat::from_cli(false, Some(".name"), false, false, &config),
            OutputFormat::Jq(".name".to_string())
        );
        // jq takes precedence over json
        assert_eq!(
            OutputFormat::from_cli(true, Some(".name"), false, false, &config),
            OutputFormat::Jq(".name".to_string())
        );
    }

    #[test]
    fn test_apply_jq() {
        let value = serde_json::json!({"name": "test", "count": 42});
        let results = apply_jq(&value, ".name").unwrap();
        assert_eq!(results, vec!["\"test\""]);

        let results = apply_jq(&value, ".count").unwrap();
        assert_eq!(results, vec!["42"]);
    }

    #[test]
    fn test_color_mode_merge() {
        // Later value wins
        assert_eq!(ColorMode::Auto.merge(ColorMode::Always), ColorMode::Always);
        assert_eq!(ColorMode::Always.merge(ColorMode::Never), ColorMode::Never);
        assert_eq!(ColorMode::Never.merge(ColorMode::Auto), ColorMode::Auto);
    }

    #[test]
    fn test_pretty_config_use_colors() {
        // Always mode
        let config = PrettyConfig {
            colors: Some(ColorMode::Always),
            ..Default::default()
        };
        assert!(config.use_colors());

        // Never mode
        let config = PrettyConfig {
            colors: Some(ColorMode::Never),
            ..Default::default()
        };
        assert!(!config.use_colors());
    }

    #[test]
    fn test_pretty_config_highlight() {
        // Default is true
        let config = PrettyConfig::default();
        assert!(config.highlight());

        // Explicit false
        let config = PrettyConfig {
            highlight: Some(false),
            ..Default::default()
        };
        assert!(!config.highlight());
    }
}
