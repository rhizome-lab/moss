//! Grep command - search file contents for a pattern.

use crate::grep;
use std::path::Path;

/// Search file contents for a pattern
pub fn cmd_grep(
    pattern: &str,
    root: Option<&Path>,
    glob_pattern: Option<&str>,
    limit: usize,
    ignore_case: bool,
    json: bool,
) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    match grep::grep(pattern, &root, glob_pattern, limit, ignore_case) {
        Ok(result) => {
            if json {
                println!("{}", serde_json::to_string(&result).unwrap());
            } else {
                if result.matches.is_empty() {
                    eprintln!("No matches found for: {}", pattern);
                    return 1;
                }
                for m in &result.matches {
                    println!("{}:{}:{}", m.file, m.line, m.content);
                }
                eprintln!(
                    "\n{} matches in {} files",
                    result.total_matches, result.files_searched
                );
            }
            0
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            1
        }
    }
}
