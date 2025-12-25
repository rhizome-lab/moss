//! Codebase health metrics.
//!
//! Quick overview of codebase health including file counts,
//! complexity summary, and structural metrics.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use rayon::prelude::*;

use crate::complexity::ComplexityAnalyzer;
use crate::path_resolve;

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

/// Per-file analysis result for parallel aggregation
struct FileStats {
    path: String,
    lines: usize,
    functions: usize,
    complexity_sum: usize,
    max_complexity: usize,
    high_risk: usize,
}

pub fn analyze_health(root: &Path) -> HealthReport {
    let all_files = path_resolve::all_files(root);
    let files: Vec<_> = all_files.iter().filter(|f| f.kind == "file").collect();

    // Thread-safe language file counts
    let files_by_language: Mutex<HashMap<String, usize>> = Mutex::new(HashMap::new());

    // Shared analyzer - GrammarStore is expensive to create, share it across threads
    let analyzer = ComplexityAnalyzer::new();

    // Process only code files (files with language support) in parallel
    let stats: Vec<FileStats> = files
        .par_iter()
        .filter_map(|file| {
            let path = root.join(&file.path);
            let lang = moss_languages::support_for_path(&path)?;

            // Count files by language
            {
                let mut counts = files_by_language.lock().unwrap();
                *counts.entry(lang.name().to_string()).or_insert(0) += 1;
            }

            let content = std::fs::read_to_string(&path).ok()?;
            let lines = content.lines().count();

            // Skip complexity analysis for files without symbol support
            if !lang.has_symbols() {
                return Some(FileStats {
                    path: file.path.clone(),
                    lines,
                    functions: 0,
                    complexity_sum: 0,
                    max_complexity: 0,
                    high_risk: 0,
                });
            }

            let report = analyzer.analyze(&path, &content);

            let mut functions = 0;
            let mut complexity_sum = 0;
            let mut max_complexity = 0;
            let mut high_risk = 0;

            for func in &report.functions {
                functions += 1;
                complexity_sum += func.complexity;
                if func.complexity > max_complexity {
                    max_complexity = func.complexity;
                }
                if func.complexity > 10 {
                    high_risk += 1;
                }
            }

            Some(FileStats {
                path: file.path.clone(),
                lines,
                functions,
                complexity_sum,
                max_complexity,
                high_risk,
            })
        })
        .collect();

    // Aggregate results
    let mut total_lines = 0;
    let mut total_functions = 0;
    let mut total_complexity = 0;
    let mut max_complexity = 0;
    let mut high_risk_functions = 0;
    let mut large_files = Vec::new();

    for stat in stats {
        total_lines += stat.lines;
        total_functions += stat.functions;
        total_complexity += stat.complexity_sum;
        if stat.max_complexity > max_complexity {
            max_complexity = stat.max_complexity;
        }
        high_risk_functions += stat.high_risk;
        if stat.lines >= LARGE_FILE_THRESHOLD && !is_lockfile(&stat.path) {
            large_files.push(LargeFile {
                path: stat.path,
                lines: stat.lines,
            });
        }
    }

    // Sort large files by line count descending
    large_files.sort_by(|a, b| b.lines.cmp(&a.lines));

    let avg_complexity = if total_functions > 0 {
        total_complexity as f64 / total_functions as f64
    } else {
        0.0
    };

    let lang_counts = files_by_language.into_inner().unwrap();

    HealthReport {
        total_files: files.len(),
        files_by_language: lang_counts,
        total_lines,
        avg_complexity,
        max_complexity,
        high_risk_functions,
        total_functions,
        large_files,
    }
}
