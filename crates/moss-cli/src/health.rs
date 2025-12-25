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

/// Thresholds for file size severity
const LARGE_THRESHOLD: usize = 500;
const VERY_LARGE_THRESHOLD: usize = 1000;
const MASSIVE_THRESHOLD: usize = 2000;

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

        // Categorize files by severity
        let massive: Vec<_> = self
            .large_files
            .iter()
            .filter(|f| f.lines >= MASSIVE_THRESHOLD)
            .collect();
        let very_large: Vec<_> = self
            .large_files
            .iter()
            .filter(|f| f.lines >= VERY_LARGE_THRESHOLD && f.lines < MASSIVE_THRESHOLD)
            .collect();
        let large: Vec<_> = self
            .large_files
            .iter()
            .filter(|f| f.lines >= LARGE_THRESHOLD && f.lines < VERY_LARGE_THRESHOLD)
            .collect();

        if !massive.is_empty() {
            lines.push(String::new());
            lines.push(format!(
                "## CRITICAL: Massive Files (>{} lines) - {}",
                MASSIVE_THRESHOLD,
                massive.len()
            ));
            for lf in massive.iter().take(10) {
                lines.push(format!("  {} ({} lines)", lf.path, lf.lines));
            }
            if massive.len() > 10 {
                lines.push(format!("  ... and {} more", massive.len() - 10));
            }
        }

        if !very_large.is_empty() {
            lines.push(String::new());
            lines.push(format!(
                "## WARNING: Very Large Files (>{} lines) - {}",
                VERY_LARGE_THRESHOLD,
                very_large.len()
            ));
            for lf in very_large.iter().take(5) {
                lines.push(format!("  {} ({} lines)", lf.path, lf.lines));
            }
            if very_large.len() > 5 {
                lines.push(format!("  ... and {} more", very_large.len() - 5));
            }
        }

        if !large.is_empty() {
            lines.push(String::new());
            lines.push(format!(
                "## Large Files (>{} lines) - {}",
                LARGE_THRESHOLD,
                large.len()
            ));
            for lf in large.iter().take(5) {
                lines.push(format!("  {} ({} lines)", lf.path, lf.lines));
            }
            if large.len() > 5 {
                lines.push(format!("  ... and {} more", large.len() - 5));
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
        // Scoring based on complexity and file sizes
        // Lower average complexity = better
        // Lower high-risk ratio = better
        // Fewer/smaller large files = better

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

        // Large file penalty: massive files are a serious problem
        let massive_count = self
            .large_files
            .iter()
            .filter(|f| f.lines >= MASSIVE_THRESHOLD)
            .count();
        let very_large_count = self
            .large_files
            .iter()
            .filter(|f| f.lines >= VERY_LARGE_THRESHOLD && f.lines < MASSIVE_THRESHOLD)
            .count();

        let file_size_score = if massive_count > 0 {
            // Any massive file is a critical issue
            0.3_f64.max(0.5 - (massive_count as f64 * 0.1))
        } else if very_large_count > 5 {
            0.5
        } else if very_large_count > 0 {
            0.7
        } else {
            1.0
        };

        // Weight: complexity 30%, risk 30%, file sizes 40%
        // File sizes weighted higher because they're more actionable
        (complexity_score * 0.3) + (risk_score * 0.3) + (file_size_score * 0.4)
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

    // Ensure file index is up to date (incremental - only changed files)
    let _ = index.incremental_refresh();
    // Note: We don't call refresh_call_graph() here - it's slow.
    // Complexity is computed during symbol indexing, which happens via other commands.

    // Query complexity stats from the index
    let conn = index.connection();

    // Get file counts by language (using file extensions)
    let mut files_by_language: HashMap<String, usize> = HashMap::new();
    let mut total_files = 0;

    if let Ok(mut stmt) = conn.prepare("SELECT path FROM files WHERE is_dir = 0") {
        if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
            for path_result in rows.flatten() {
                total_files += 1;
                let path = std::path::Path::new(&path_result);
                if let Some(lang) = moss_languages::support_for_path(path) {
                    *files_by_language
                        .entry(lang.name().to_string())
                        .or_insert(0) += 1;
                }
            }
        }
    }

    // Get complexity stats from symbols table
    let mut total_functions = 0usize;
    let mut total_complexity = 0usize;
    let mut max_complexity = 0usize;
    let mut high_risk_functions = 0usize;

    if let Ok(mut stmt) =
        conn.prepare("SELECT complexity FROM symbols WHERE complexity IS NOT NULL")
    {
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

    // Use cached line counts from index
    let mut total_lines = 0usize;
    let mut large_files = Vec::new();

    if let Ok(files) = index.all_files() {
        for file in files {
            if file.is_dir {
                continue;
            }
            total_lines += file.lines;
            if file.lines >= LARGE_THRESHOLD && !is_lockfile(&file.path) {
                large_files.push(LargeFile {
                    path: file.path,
                    lines: file.lines,
                });
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
