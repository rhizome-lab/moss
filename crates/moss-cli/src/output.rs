//! Output formatting utilities.
//!
//! Provides consistent JSON/text output across all commands via the `OutputFormatter` trait.

use serde::Serialize;

/// Output format mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OutputFormat {
    /// Human-readable text output (default).
    #[default]
    Text,
    /// Compact JSON (single line).
    Json,
    /// Pretty-printed JSON (indented).
    JsonPretty,
}

impl OutputFormat {
    /// Create from common CLI flags.
    pub fn from_flags(json: bool, pretty: bool) -> Self {
        match (json, pretty) {
            (true, true) => OutputFormat::JsonPretty,
            (true, false) => OutputFormat::Json,
            (false, _) => OutputFormat::Text,
        }
    }

    /// Is this a JSON format?
    pub fn is_json(&self) -> bool {
        matches!(self, OutputFormat::Json | OutputFormat::JsonPretty)
    }
}

/// Trait for types that can format output in multiple formats.
///
/// Types implementing this trait can be printed as either JSON or text.
/// JSON serialization uses serde, while text formatting is custom.
pub trait OutputFormatter: Serialize {
    /// Format as human-readable text.
    fn format_text(&self) -> String;

    /// Print to stdout in the specified format.
    fn print(&self, format: OutputFormat) {
        match format {
            OutputFormat::Text => println!("{}", self.format_text()),
            OutputFormat::Json => {
                println!("{}", serde_json::to_string(self).unwrap_or_default())
            }
            OutputFormat::JsonPretty => {
                println!("{}", serde_json::to_string_pretty(self).unwrap_or_default())
            }
        }
    }

    /// Format to string in the specified format.
    fn format(&self, format: OutputFormat) -> String {
        match format {
            OutputFormat::Text => self.format_text(),
            OutputFormat::Json => serde_json::to_string(self).unwrap_or_default(),
            OutputFormat::JsonPretty => serde_json::to_string_pretty(self).unwrap_or_default(),
        }
    }
}

/// Helper to print any serializable value as JSON.
/// Use this for ad-hoc JSON output where implementing OutputFormatter is overkill.
pub fn print_json<T: Serialize>(value: &T) {
    println!("{}", serde_json::to_string(value).unwrap_or_default());
}

/// Helper to print any serializable value as pretty JSON.
pub fn print_json_pretty<T: Serialize>(value: &T) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_default()
    );
}

/// Helper to print based on format flag (for simple cases).
pub fn print_formatted<T: Serialize>(value: &T, json: bool, pretty: bool) {
    let format = OutputFormat::from_flags(json, pretty);
    match format {
        OutputFormat::Text => {
            // For values without custom text formatting, fall back to debug
            println!("{:#?}", serde_json::to_value(value).unwrap_or_default());
        }
        OutputFormat::Json => print_json(value),
        OutputFormat::JsonPretty => print_json_pretty(value),
    }
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
    fn test_output_format_from_flags() {
        assert_eq!(OutputFormat::from_flags(false, false), OutputFormat::Text);
        assert_eq!(OutputFormat::from_flags(true, false), OutputFormat::Json);
        assert_eq!(
            OutputFormat::from_flags(true, true),
            OutputFormat::JsonPretty
        );
        assert_eq!(OutputFormat::from_flags(false, true), OutputFormat::Text);
    }

    #[test]
    fn test_format() {
        let output = TestOutput {
            name: "test".into(),
            count: 42,
        };

        assert_eq!(output.format(OutputFormat::Text), "test: 42");
        assert_eq!(
            output.format(OutputFormat::Json),
            r#"{"name":"test","count":42}"#
        );
        assert!(output.format(OutputFormat::JsonPretty).contains("  "));
    }
}
