//! Log format plugins.
//!
//! Each format implements the `LogFormat` trait for parsing session logs.

mod claude_code;
mod gemini_cli;

pub use claude_code::ClaudeCodeFormat;
pub use gemini_cli::GeminiCliFormat;

use crate::SessionAnalysis;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

/// Trait for session log format plugins.
pub trait LogFormat: Send + Sync {
    /// Format identifier (e.g., "claude", "gemini").
    fn name(&self) -> &'static str;

    /// Check if this format can parse the given file.
    /// Returns a confidence score 0.0-1.0.
    fn detect(&self, path: &Path) -> f64;

    /// Parse the log file and produce analysis.
    fn analyze(&self, path: &Path) -> Result<SessionAnalysis, String>;
}

/// Registry of available log formats.
pub struct FormatRegistry {
    formats: Vec<Box<dyn LogFormat>>,
}

impl Default for FormatRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatRegistry {
    pub fn new() -> Self {
        Self {
            formats: vec![Box::new(ClaudeCodeFormat), Box::new(GeminiCliFormat)],
        }
    }

    /// Detect the best format for a file.
    pub fn detect(&self, path: &Path) -> Option<&dyn LogFormat> {
        let mut best: Option<(&dyn LogFormat, f64)> = None;
        for fmt in &self.formats {
            let score = fmt.detect(path);
            if score > 0.0 {
                if best.is_none() || score > best.unwrap().1 {
                    best = Some((fmt.as_ref(), score));
                }
            }
        }
        best.map(|(fmt, _)| fmt)
    }

    /// Get a format by name.
    pub fn get(&self, name: &str) -> Option<&dyn LogFormat> {
        self.formats
            .iter()
            .find(|f| f.name() == name)
            .map(|f| f.as_ref())
    }
}

/// Analyze a session log with auto-format detection.
pub fn analyze_session(path: &Path) -> Result<SessionAnalysis, String> {
    let registry = FormatRegistry::new();
    let format = registry
        .detect(path)
        .ok_or_else(|| format!("Unknown log format: {}", path.display()))?;
    format.analyze(path)
}

/// Analyze a session log with explicit format.
pub fn analyze_session_with_format(
    path: &Path,
    format_name: &str,
) -> Result<SessionAnalysis, String> {
    let registry = FormatRegistry::new();
    let format = registry
        .get(format_name)
        .ok_or_else(|| format!("Unknown format: {}", format_name))?;
    format.analyze(path)
}

/// Helper: read first N lines of a file.
pub(crate) fn peek_lines(path: &Path, n: usize) -> Vec<String> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    BufReader::new(file)
        .lines()
        .take(n)
        .filter_map(|l| l.ok())
        .collect()
}

/// Helper: read entire file as string.
pub(crate) fn read_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|e| e.to_string())?;
    Ok(content)
}
