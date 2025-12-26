//! Grep command - search file contents for a pattern.

use crate::grep;
use crate::output::{OutputFormat, OutputFormatter};
use std::path::Path;

/// Search file contents for a pattern
pub fn cmd_grep(
    pattern: &str,
    root: Option<&Path>,
    glob_pattern: Option<&str>,
    limit: usize,
    ignore_case: bool,
    json: bool,
    jq: Option<&str>,
) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    match grep::grep(pattern, &root, glob_pattern, limit, ignore_case) {
        Ok(result) => {
            let format = OutputFormat::from_flags(json, jq);
            if result.matches.is_empty() && !format.is_json() {
                eprintln!("No matches found for: {}", pattern);
                return 1;
            }
            result.print(&format);
            0
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            1
        }
    }
}
