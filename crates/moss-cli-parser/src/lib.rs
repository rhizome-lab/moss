//! Parse CLI --help output from various frameworks.
//!
//! # Architecture
//!
//! Similar to moss-sessions and moss-languages, this crate uses a trait-based
//! extensible architecture. Each CLI framework has a corresponding parser that
//! implements the `CliFormat` trait.
//!
//! # Supported Formats
//!
//! - `clap` - Rust's clap/structopt
//! - `argparse` - Python's argparse (stdlib)
//! - `click` - Python's click
//! - `commander` - Node.js commander.js
//! - `yargs` - Node.js yargs
//! - `cobra` - Go's cobra (spf13/cobra)
//!
//! # Example
//!
//! ```ignore
//! use rhizome_moss_cli_parser::{parse_help, CliSpec};
//!
//! let help_text = "mycli 1.0.0\n\nUsage: mycli [OPTIONS]\n\n...";
//! let spec = parse_help(help_text)?;
//! println!("Commands: {:?}", spec.commands);
//! ```

mod formats;

pub use formats::{CliFormat, FormatRegistry, detect_format, get_format, list_formats, register};

use serde::{Deserialize, Serialize};

/// A parsed CLI specification.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CliSpec {
    /// Program name
    pub name: Option<String>,
    /// Program version
    pub version: Option<String>,
    /// Program description
    pub description: Option<String>,
    /// Usage string
    pub usage: Option<String>,
    /// Global options/flags
    pub options: Vec<CliOption>,
    /// Subcommands
    pub commands: Vec<CliCommand>,
}

/// A CLI option/flag.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CliOption {
    /// Short flag (e.g., "-v")
    pub short: Option<String>,
    /// Long flag (e.g., "--verbose")
    pub long: Option<String>,
    /// Value placeholder (e.g., "<FILE>")
    pub value: Option<String>,
    /// Description
    pub description: Option<String>,
    /// Default value if any
    pub default: Option<String>,
    /// Whether this is required
    pub required: bool,
    /// Environment variable that sets this
    pub env: Option<String>,
}

/// A CLI subcommand.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CliCommand {
    /// Command name
    pub name: String,
    /// Command description
    pub description: Option<String>,
    /// Command aliases
    pub aliases: Vec<String>,
    /// Command-specific options
    pub options: Vec<CliOption>,
    /// Nested subcommands
    pub subcommands: Vec<CliCommand>,
}

/// Parse help text, auto-detecting the format.
pub fn parse_help(help_text: &str) -> Result<CliSpec, String> {
    let registry = FormatRegistry::new();

    // Try to detect format
    if let Some(format) = registry.detect(help_text) {
        format.parse(help_text)
    } else {
        Err("Could not detect CLI help format".to_string())
    }
}

/// Parse help text with a specific format.
pub fn parse_help_with_format(help_text: &str, format_name: &str) -> Result<CliSpec, String> {
    let registry = FormatRegistry::new();

    if let Some(format) = registry.get(format_name) {
        format.parse(help_text)
    } else {
        Err(format!("Unknown format: {}", format_name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_clap() {
        let help = r#"mycli 1.0.0
A simple CLI tool

Usage: mycli [OPTIONS] <COMMAND>

Commands:
  run   Run something
  help  Print help

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
  -V, --version  Print version
"#;

        let spec = parse_help(help).unwrap();
        assert_eq!(spec.name, Some("mycli".to_string()));
        assert_eq!(spec.version, Some("1.0.0".to_string()));
        assert_eq!(spec.commands.len(), 1); // "help" is filtered out
        assert_eq!(spec.options.len(), 1); // "help" and "version" are filtered out
    }
}
