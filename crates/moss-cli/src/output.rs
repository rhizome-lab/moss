//! Output formatting utilities.
//!
//! Provides consistent JSON/text output across all commands via the `OutputFormatter` trait.

use serde::Serialize;

/// Output format mode.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum OutputFormat {
    /// Human-readable text output (default).
    #[default]
    Text,
    /// Compact JSON (single line).
    Json,
    /// Pretty-printed JSON (indented).
    JsonPretty,
    /// JSON filtered through jq expression.
    Jq(String),
}

impl OutputFormat {
    /// Create from common CLI flags.
    pub fn from_flags(json: bool, jq: Option<&str>) -> Self {
        match (json, jq) {
            (_, Some(filter)) => OutputFormat::Jq(filter.to_string()),
            (true, None) => OutputFormat::Json,
            (false, None) => OutputFormat::Text,
        }
    }

    /// Is this a JSON-based format?
    pub fn is_json(&self) -> bool {
        matches!(
            self,
            OutputFormat::Json | OutputFormat::JsonPretty | OutputFormat::Jq(_)
        )
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
    fn print(&self, format: &OutputFormat) {
        match format {
            OutputFormat::Text => println!("{}", self.format_text()),
            OutputFormat::Json => {
                println!("{}", serde_json::to_string(self).unwrap_or_default())
            }
            OutputFormat::JsonPretty => {
                println!("{}", serde_json::to_string_pretty(self).unwrap_or_default())
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
    fn test_output_format_from_flags() {
        assert_eq!(OutputFormat::from_flags(false, None), OutputFormat::Text);
        assert_eq!(OutputFormat::from_flags(true, None), OutputFormat::Json);
        assert_eq!(
            OutputFormat::from_flags(false, Some(".name")),
            OutputFormat::Jq(".name".to_string())
        );
        assert_eq!(
            OutputFormat::from_flags(true, Some(".name")),
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
}
