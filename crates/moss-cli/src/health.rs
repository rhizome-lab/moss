//! Codebase health metrics.
//!
//! Quick overview of codebase health including file counts,
//! complexity summary, and structural metrics.

use std::collections::HashMap;
use std::path::Path;

use crate::index::FileIndex;

/// Large file info for reporting
#[derive(Debug, Clone)]
pub struct LargeFile {
    pub path: String,
    pub lines: usize,
}

/// Health metrics for a codebase
#[derive(Debug)]
pub struct HealthReport {
    pub total_files: usize,
    pub files_by_language: HashMap<String, usize>,
    pub total_lines: usize,
    pub avg_complexity: f64,
    pub max_complexity: usize,
    pub high_risk_functions: usize,
    pub total_functions: usize,
    pub large_files: Vec<LargeFile>,
}

impl HealthReport {
    pub fn format(&self) -> String {
        let mut lines = Vec::new();

        lines.push("# Codebase Health".to_string());
        lines.push(String::new());

        lines.push("## Files".to_string());
        lines.push(format!("  Total: {}", self.total_files));
        for (lang, count) in &self.files_by_language {
            if *count > 0 {
                lines.push(format!("  {}: {}", lang, count));
            }
        }
        lines.push(format!("  Lines: {}", self.total_lines));
        lines.push(String::new());

        lines.push("## Complexity".to_string());
        lines.push(format!("  Functions: {}", self.total_functions));
        lines.push(format!("  Average: {:.1}", self.avg_complexity));
        lines.push(format!("  Maximum: {}", self.max_complexity));
        lines.push(format!("  High risk (>10): {}", self.high_risk_functions));

        if !self.large_files.is_empty() {
            lines.push(String::new());
            lines.push("## Large Files (>500 lines)".to_string());
            for lf in self.large_files.iter().take(10) {
                lines.push(format!("  {} ({} lines)", lf.path, lf.lines));
            }
            if self.large_files.len() > 10 {
                lines.push(format!("  ... and {} more", self.large_files.len() - 10));
            }
        }

        let health_score = self.calculate_health_score();
        let grade = self.grade();
        lines.push(String::new());
        lines.push(format!(
            "## Score: {} ({:.0}%)",
            grade,
            health_score * 100.0
        ));

        lines.join("\n")
    }

    fn calculate_health_score(&self) -> f64 {
        // Simple scoring based on complexity
        // Lower average complexity = better
        // Lower high-risk ratio = better

        let complexity_score = if self.avg_complexity <= 3.0 {
            1.0
        } else if self.avg_complexity <= 5.0 {
            0.9
        } else if self.avg_complexity <= 7.0 {
            0.8
        } else if self.avg_complexity <= 10.0 {
            0.7
        } else if self.avg_complexity <= 15.0 {
            0.5
        } else {
            0.3
        };

        let high_risk_ratio = if self.total_functions > 0 {
            self.high_risk_functions as f64 / self.total_functions as f64
        } else {
            0.0
        };

        let risk_score = if high_risk_ratio <= 0.01 {
            1.0
        } else if high_risk_ratio <= 0.02 {
            0.9
        } else if high_risk_ratio <= 0.03 {
            0.8
        } else if high_risk_ratio <= 0.05 {
            0.7
        } else if high_risk_ratio <= 0.1 {
            0.5
        } else {
            0.3
        };

        (complexity_score + risk_score) / 2.0
    }

    fn grade(&self) -> &'static str {
        let score = self.calculate_health_score();
        if score >= 0.9 {
            "A"
        } else if score >= 0.8 {
            "B"
        } else if score >= 0.7 {
            "C"
        } else if score >= 0.6 {
            "D"
        } else {
            "F"
        }
    }
}

/// Threshold for "large" files
const LARGE_FILE_THRESHOLD: usize = 500;

/// Check if a path is a lockfile (generated, not a code smell)
fn is_lockfile(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    matches!(
        name,
        "uv.lock"
            | "Cargo.lock"
            | "package-lock.json"
            | "yarn.lock"
            | "pnpm-lock.yaml"
            | "bun.lockb"
            | "bun.lock"
            | "poetry.lock"
            | "Pipfile.lock"
            | "Gemfile.lock"
            | "composer.lock"
            | "go.sum"
            | "flake.lock"
            | "packages.lock.json" // NuGet
            | "paket.lock"
            | "pubspec.lock" // Dart/Flutter
            | "mix.lock" // Elixir
            | "rebar.lock" // Erlang
            | "Podfile.lock" // CocoaPods
            | "shrinkwrap.yaml" // pnpm
            | "deno.lock" // Deno
            | "gradle.lockfile" // Gradle
    )
}

pub fn analyze_health(root: &Path) -> HealthReport {
    // Open the index (creates/refreshes if needed)
    let mut index = match FileIndex::open(root) {
        Ok(idx) => idx,
        Err(_) => {
            return HealthReport {
                total_files: 0,
                files_by_language: HashMap::new(),
                total_lines: 0,
                avg_complexity: 0.0,
                max_complexity: 0,
                high_risk_functions: 0,
                total_functions: 0,
                large_files: Vec::new(),
            };
        }
    };

    // Ensure file index is up to date (fast check)
    if index.needs_refresh() {
        let _ = index.refresh();
    }
    // Note: We don't call refresh_call_graph() here - it's slow.
    // Complexity is computed during symbol indexing, which happens via other commands.

    // Query complexity stats from the index
    let conn = index.connection();

    // Get file counts by language (using file extensions)
    let mut files_by_language: HashMap<String, usize> = HashMap::new();
    let mut total_files = 0;

    if let Ok(mut stmt) = conn.prepare(
        "SELECT path FROM files WHERE is_dir = 0"
    ) {
        if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
            for path_result in rows.flatten() {
                total_files += 1;
                let path = std::path::Path::new(&path_result);
                if let Some(lang) = moss_languages::support_for_path(path) {
                    *files_by_language.entry(lang.name().to_string()).or_insert(0) += 1;
                }
            }
        }
    }

    // Get complexity stats from symbols table
    let mut total_functions = 0usize;
    let mut total_complexity = 0usize;
    let mut max_complexity = 0usize;
    let mut high_risk_functions = 0usize;

    if let Ok(mut stmt) = conn.prepare(
        "SELECT complexity FROM symbols WHERE complexity IS NOT NULL"
    ) {
        if let Ok(rows) = stmt.query_map([], |row| row.get::<_, i64>(0)) {
            for complexity_result in rows.flatten() {
                let complexity = complexity_result as usize;
                total_functions += 1;
                total_complexity += complexity;
                if complexity > max_complexity {
                    max_complexity = complexity;
                }
                if complexity > 10 {
                    high_risk_functions += 1;
                }
            }
        }
    }

    // Count total lines (still need to read files for this)
    // TODO: Cache line counts in the index
    let mut total_lines = 0usize;
    let mut large_files = Vec::new();

    if let Ok(files) = index.all_files() {
        for file in files {
            if file.is_dir {
                continue;
            }
            let full_path = root.join(&file.path);
            if let Ok(content) = std::fs::read_to_string(&full_path) {
                let lines = content.lines().count();
                total_lines += lines;
                if lines >= LARGE_FILE_THRESHOLD && !is_lockfile(&file.path) {
                    large_files.push(LargeFile {
                        path: file.path,
                        lines,
                    });
                }
            }
        }
    }

    // Sort large files by line count descending
    large_files.sort_by(|a, b| b.lines.cmp(&a.lines));

    let avg_complexity = if total_functions > 0 {
        total_complexity as f64 / total_functions as f64
    } else {
        0.0
    };

    HealthReport {
        total_files,
        files_by_language,
        total_lines,
        avg_complexity,
        max_complexity,
        high_risk_functions,
        total_functions,
        large_files,
    }
}
