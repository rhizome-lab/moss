//! Fast text search using ripgrep's grep crate.

use crate::output::OutputFormatter;
use grep_matcher::Matcher;
use grep_regex::RegexMatcher;
use grep_searcher::sinks::UTF8;
use grep_searcher::Searcher;
use ignore::WalkBuilder;
use std::fmt::Write;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// A single match result
#[derive(Debug, Clone, serde::Serialize)]
pub struct GrepMatch {
    pub file: String,
    pub line: usize,
    pub content: String,
    pub start: usize,
    pub end: usize,
}

/// Result of a grep search
#[derive(Debug, serde::Serialize)]
pub struct GrepResult {
    pub matches: Vec<GrepMatch>,
    pub total_matches: usize,
    pub files_searched: usize,
}

/// Search for a pattern in files
pub fn grep(
    pattern: &str,
    root: &Path,
    glob_pattern: Option<&str>,
    limit: usize,
    ignore_case: bool,
) -> io::Result<GrepResult> {
    // Build the regex matcher
    let pattern_str = if ignore_case {
        format!("(?i){}", pattern)
    } else {
        pattern.to_string()
    };
    let matcher = RegexMatcher::new_line_matcher(&pattern_str)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

    let matches: Mutex<Vec<GrepMatch>> = Mutex::new(Vec::new());
    let total_matches = AtomicUsize::new(0);
    let files_searched = AtomicUsize::new(0);

    // Build the file walker
    let mut builder = WalkBuilder::new(root);
    builder.hidden(true); // skip hidden files
    builder.git_ignore(true);
    builder.git_global(true);
    builder.git_exclude(true);

    // Apply glob pattern if provided
    if let Some(glob) = glob_pattern {
        let mut types = ignore::types::TypesBuilder::new();
        types.add("custom", glob).ok();
        types.select("custom");
        if let Ok(types) = types.build() {
            builder.types(types);
        }
    }

    let walker = builder.build_parallel();

    walker.run(|| {
        let matcher = &matcher;
        let matches = &matches;
        let total_matches = &total_matches;
        let files_searched = &files_searched;

        Box::new(move |entry| {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => return ignore::WalkState::Continue,
            };

            // Skip directories
            if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                return ignore::WalkState::Continue;
            }

            let path = entry.path();
            files_searched.fetch_add(1, Ordering::Relaxed);

            let mut searcher = Searcher::new();
            let mut file_matches: Vec<GrepMatch> = Vec::new();

            let rel_path = path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();

            let _ = searcher.search_path(
                matcher,
                path,
                UTF8(|line_num, line| {
                    // Find match positions
                    let mut start = 0;
                    let mut end = 0;
                    if let Ok(Some(m)) = matcher.find(line.as_bytes()) {
                        start = m.start();
                        end = m.end();
                    }

                    file_matches.push(GrepMatch {
                        file: rel_path.clone(),
                        line: line_num as usize,
                        content: line.trim_end().to_string(),
                        start,
                        end,
                    });
                    Ok(true)
                }),
            );

            if !file_matches.is_empty() {
                total_matches.fetch_add(file_matches.len(), Ordering::Relaxed);

                let mut guard = matches.lock().unwrap();
                for m in file_matches {
                    if guard.len() < limit {
                        guard.push(m);
                    }
                }

                // Stop early if we have enough matches
                if guard.len() >= limit {
                    return ignore::WalkState::Quit;
                }
            }

            ignore::WalkState::Continue
        })
    });

    let matches = matches.into_inner().unwrap();
    Ok(GrepResult {
        matches,
        total_matches: total_matches.load(Ordering::Relaxed),
        files_searched: files_searched.load(Ordering::Relaxed),
    })
}

impl OutputFormatter for GrepResult {
    fn format_text(&self) -> String {
        let mut out = String::new();
        for m in &self.matches {
            writeln!(out, "{}:{}:{}", m.file, m.line, m.content).unwrap();
        }
        write!(
            out,
            "\n{} matches in {} files",
            self.total_matches, self.files_searched
        )
        .unwrap();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_grep_basic() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("test.txt");
        fs::write(&file, "hello world\nfoo bar\nhello again").unwrap();

        let result = grep("hello", dir.path(), None, 100, false).unwrap();
        assert_eq!(result.total_matches, 2);
        assert_eq!(result.matches.len(), 2);
        assert_eq!(result.matches[0].line, 1);
        assert_eq!(result.matches[1].line, 3);
    }

    #[test]
    fn test_grep_case_insensitive() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("test.txt");
        fs::write(&file, "Hello World\nHELLO AGAIN").unwrap();

        let result = grep("hello", dir.path(), None, 100, true).unwrap();
        assert_eq!(result.total_matches, 2);
    }

    #[test]
    fn test_grep_limit() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("test.txt");
        fs::write(&file, "a\na\na\na\na").unwrap();

        let result = grep("a", dir.path(), None, 2, false).unwrap();
        assert_eq!(result.matches.len(), 2);
        assert!(result.total_matches >= 2);
    }
}
