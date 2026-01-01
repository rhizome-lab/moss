//! Fast text search using ripgrep's grep crate.

use crate::filter::Filter;
use crate::output::OutputFormatter;
use crate::symbols::SymbolParser;
use grep_matcher::Matcher;
use grep_regex::RegexMatcher;
use grep_searcher::Searcher;
use grep_searcher::sinks::UTF8;
use ignore::WalkBuilder;
use nu_ansi_term::Color::{Cyan, Green, Red, Yellow};
use std::collections::HashMap;
use std::fmt::Write;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A single match result
#[derive(Debug, Clone, serde::Serialize)]
pub struct GrepMatch {
    pub file: String,
    pub line: usize,
    pub content: String,
    pub start: usize,
    pub end: usize,
    /// Containing symbol name (if found)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Containing symbol start line
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_start: Option<usize>,
    /// Containing symbol end line
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_end: Option<usize>,
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
    filter: Option<&Filter>,
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

            // Apply filter if provided
            let rel_path = path.strip_prefix(root).unwrap_or(path);
            if let Some(f) = filter {
                if !f.matches(rel_path) {
                    return ignore::WalkState::Continue;
                }
            }

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

                    let trimmed = line.trim();
                    // Adjust match positions for trimmed content
                    let leading_ws = line.len() - line.trim_start().len();
                    let adj_start = start.saturating_sub(leading_ws);
                    let adj_end = end.saturating_sub(leading_ws).min(trimmed.len());

                    file_matches.push(GrepMatch {
                        file: rel_path.clone(),
                        line: line_num as usize,
                        content: trimmed.to_string(),
                        start: adj_start,
                        end: adj_end,
                        symbol: None,
                        symbol_start: None,
                        symbol_end: None,
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

    let mut matches = matches.into_inner().unwrap();

    // Enrich matches with containing symbol info
    add_symbol_context(&mut matches, root);

    Ok(GrepResult {
        matches,
        total_matches: total_matches.load(Ordering::Relaxed),
        files_searched: files_searched.load(Ordering::Relaxed),
    })
}

/// Enrich grep matches with containing symbol information.
fn add_symbol_context(matches: &mut [GrepMatch], root: &Path) {
    if matches.is_empty() {
        return;
    }

    // Group match indices by file (clone file strings to avoid borrow issues)
    let mut by_file: HashMap<String, Vec<usize>> = HashMap::new();
    for (idx, m) in matches.iter().enumerate() {
        by_file.entry(m.file.clone()).or_default().push(idx);
    }

    let parser = SymbolParser::new();

    // For each file with matches, parse symbols and find containing symbol
    for (file, indices) in by_file {
        let path = root.join(&file);
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let symbols = parser.parse_file(&path, &content);
        if symbols.is_empty() {
            continue;
        }

        // For each match in this file, find the smallest containing symbol
        for idx in indices {
            let line = matches[idx].line;

            // Find smallest symbol that contains this line
            let mut best: Option<&crate::symbols::FlatSymbol> = None;
            for sym in &symbols {
                if sym.start_line <= line && line <= sym.end_line {
                    // Prefer smaller (more specific) symbols
                    if best.is_none()
                        || (sym.end_line - sym.start_line)
                            < (best.unwrap().end_line - best.unwrap().start_line)
                    {
                        best = Some(sym);
                    }
                }
            }

            if let Some(sym) = best {
                // Format name with parent if present (use / for consistency with moss paths)
                let name = if let Some(parent) = &sym.parent {
                    format!("{}/{}", parent, sym.name)
                } else {
                    sym.name.clone()
                };
                matches[idx].symbol = Some(name);
                matches[idx].symbol_start = Some(sym.start_line);
                matches[idx].symbol_end = Some(sym.end_line);
            }
        }
    }
}

/// Format symbol info for display: " (symbol_name L10-25)" or empty string
fn format_symbol_info(m: &GrepMatch, colorize: bool) -> String {
    match (&m.symbol, m.symbol_start, m.symbol_end) {
        (Some(name), Some(start), Some(end)) => {
            if colorize {
                format!(" ({} L{}-{})", Green.paint(name), start, end)
            } else {
                format!(" ({} L{}-{})", name, start, end)
            }
        }
        _ => String::new(),
    }
}

impl OutputFormatter for GrepResult {
    fn format_text(&self) -> String {
        use std::collections::BTreeMap;

        // Group matches by file
        let mut by_file: BTreeMap<&str, Vec<&GrepMatch>> = BTreeMap::new();
        for m in &self.matches {
            by_file.entry(&m.file).or_default().push(m);
        }

        let mut out = String::new();
        for (file, matches) in by_file {
            writeln!(out, "{}:", file).unwrap();
            for m in matches {
                let sym_info = format_symbol_info(m, false);
                writeln!(out, "  {}{}:{}", m.line, sym_info, m.content).unwrap();
            }
        }
        write!(
            out,
            "\n{} matches in {} files",
            self.total_matches, self.files_searched
        )
        .unwrap();
        out
    }

    fn format_pretty(&self) -> String {
        use std::collections::BTreeMap;

        // Group matches by file
        let mut by_file: BTreeMap<&str, Vec<&GrepMatch>> = BTreeMap::new();
        for m in &self.matches {
            by_file.entry(&m.file).or_default().push(m);
        }

        let mut out = String::new();
        for (file, matches) in by_file {
            writeln!(out, "{}:", Cyan.paint(file)).unwrap();
            for m in matches {
                // Highlight the match within the content
                let content = if m.start < m.end && m.end <= m.content.len() {
                    format!(
                        "{}{}{}",
                        &m.content[..m.start],
                        Red.bold().paint(&m.content[m.start..m.end]),
                        &m.content[m.end..]
                    )
                } else {
                    m.content.clone()
                };
                let sym_info = format_symbol_info(m, true);
                writeln!(
                    out,
                    "  {}{}:{}",
                    Yellow.paint(m.line.to_string()),
                    sym_info,
                    content
                )
                .unwrap();
            }
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
