//! File length analysis - find longest files in codebase

use crate::path_resolve;
use glob::Pattern;
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::Path;

/// File length info
#[derive(Debug, Clone)]
pub struct FileLength {
    pub path: String,
    pub lines: usize,
    pub language: String,
}

/// File length report
#[derive(Debug)]
pub struct FileLengthReport {
    pub files: Vec<FileLength>,
    pub total_lines: usize,
    pub by_language: HashMap<String, usize>,
}

impl FileLengthReport {
    pub fn format(&self) -> String {
        let mut lines = Vec::new();

        lines.push("# Longest Files".to_string());
        lines.push(String::new());

        lines.push(format!(
            "Total: {} lines across all files",
            self.total_lines
        ));
        lines.push(String::new());

        if !self.files.is_empty() {
            lines.push("## Top Files".to_string());
            for f in &self.files {
                lines.push(format!("{:>6} lines  {}", f.lines, f.path));
            }
            lines.push(String::new());
        }

        if !self.by_language.is_empty() {
            lines.push("## By Language".to_string());
            let mut langs: Vec<_> = self.by_language.iter().collect();
            langs.sort_by(|a, b| b.1.cmp(a.1));
            for (lang, count) in langs {
                lines.push(format!("{:>6} lines  {}", count, lang));
            }
        }

        lines.join("\n")
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "total_lines": self.total_lines,
            "files": self.files.iter().map(|f| {
                serde_json::json!({
                    "path": f.path,
                    "lines": f.lines,
                    "language": f.language
                })
            }).collect::<Vec<_>>(),
            "by_language": self.by_language
        })
    }
}

/// Run file length analysis
pub fn cmd_files(root: &Path, limit: usize, exclude: &[String], json: bool) -> i32 {
    let report = analyze_files(root, limit, exclude);

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report.to_json()).unwrap()
        );
    } else {
        println!("{}", report.format());
    }

    0
}

/// Analyze file lengths
pub fn analyze_files(root: &Path, limit: usize, exclude: &[String]) -> FileLengthReport {
    let all_files = path_resolve::all_files(root);
    let files: Vec<_> = all_files.iter().filter(|f| f.kind == "file").collect();

    // Compile exclude patterns
    let excludes: Vec<Pattern> = exclude
        .iter()
        .filter_map(|p| Pattern::new(p).ok())
        .collect();

    let file_lengths: Vec<FileLength> = files
        .par_iter()
        .filter_map(|file| {
            // Skip excluded files
            if excludes.iter().any(|pat| pat.matches(&file.path)) {
                return None;
            }

            let path = root.join(&file.path);
            let lang = moss_languages::support_for_path(&path)?;

            let content = std::fs::read_to_string(&path).ok()?;
            let lines = content.lines().count();

            Some(FileLength {
                path: file.path.clone(),
                lines,
                language: lang.name().to_string(),
            })
        })
        .collect();

    let total_lines: usize = file_lengths.iter().map(|f| f.lines).sum();

    let mut by_language: HashMap<String, usize> = HashMap::new();
    for f in &file_lengths {
        *by_language.entry(f.language.clone()).or_insert(0) += f.lines;
    }

    let mut sorted = file_lengths;
    sorted.sort_by(|a, b| b.lines.cmp(&a.lines));
    sorted.truncate(limit);

    FileLengthReport {
        files: sorted,
        total_lines,
        by_language,
    }
}
