//! CLI format parsers.

mod argparse;
mod clap;
mod click;
mod cobra;
mod commander;
mod yargs;

pub use self::argparse::ArgparseFormat;
pub use self::clap::ClapFormat;
pub use self::click::ClickFormat;
pub use self::cobra::CobraFormat;
pub use self::commander::CommanderFormat;
pub use self::yargs::YargsFormat;

use crate::CliSpec;

/// Trait for CLI help format parsers.
pub trait CliFormat: Send + Sync {
    /// Format name (e.g., "clap", "argparse").
    fn name(&self) -> &'static str;

    /// Confidence score (0.0-1.0) that this format matches the help text.
    fn detect(&self, help_text: &str) -> f64;

    /// Parse help text into a CliSpec.
    fn parse(&self, help_text: &str) -> Result<CliSpec, String>;
}

/// Registry of all available CLI format parsers.
pub struct FormatRegistry {
    formats: Vec<Box<dyn CliFormat>>,
}

impl FormatRegistry {
    /// Create a new registry with all built-in formats.
    pub fn new() -> Self {
        Self {
            formats: vec![
                Box::new(ClapFormat),
                Box::new(ArgparseFormat),
                Box::new(ClickFormat),
                Box::new(CommanderFormat),
                Box::new(YargsFormat),
                Box::new(CobraFormat),
            ],
        }
    }

    /// Get a format by name.
    pub fn get(&self, name: &str) -> Option<&dyn CliFormat> {
        self.formats
            .iter()
            .find(|f| f.name() == name)
            .map(|f| f.as_ref())
    }

    /// Auto-detect format from help text.
    pub fn detect(&self, help_text: &str) -> Option<&dyn CliFormat> {
        self.formats
            .iter()
            .map(|f| (f, f.detect(help_text)))
            .filter(|(_, score)| *score > 0.5)
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .map(|(f, _)| f.as_ref())
    }

    /// List all available format names.
    pub fn list(&self) -> Vec<&'static str> {
        self.formats.iter().map(|f| f.name()).collect()
    }
}

impl Default for FormatRegistry {
    fn default() -> Self {
        Self::new()
    }
}
