//! Codebase health metrics.
//!
//! Quick overview of codebase health including file counts,
//! complexity summary, and structural metrics.

use glob::Pattern;
use std::collections::HashMap;
use std::path::Path;

use crate::commands::analyze::complexity::analyze_codebase_complexity;
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

/// Load patterns from an allow file (.moss/large-files-allow or similar)
fn load_allow_patterns(root: &Path, filename: &str) -> Vec<Pattern> {
    let path = root.join(".moss").join(filename);
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#')
        })
        .filter_map(|line| Pattern::new(line.trim()).ok())
        .collect()
}

/// Check if path matches any allow pattern
fn is_allowed(path: &str, patterns: &[Pattern]) -> bool {
    patterns.iter().any(|p| p.matches(path))
}

struct ComplexityStats {
    total_functions: usize,
    avg_complexity: f64,
    max_complexity: usize,
    high_risk_functions: usize,
}

fn compute_complexity_stats(root: &Path, allowlist: &[String]) -> ComplexityStats {
    let report = analyze_codebase_complexity(root, usize::MAX, None, None, allowlist);
    ComplexityStats {
        total_functions: report.functions.len(),
        avg_complexity: report.avg_complexity(),
        max_complexity: report.max_complexity(),
        high_risk_functions: report.high_risk_count() + report.critical_risk_count(),
    }
}

pub fn analyze_health(root: &Path) -> HealthReport {
    let allow_patterns = load_allow_patterns(root, "large-files-allow");

    // Compute complexity upfront (before entering async context to avoid nested runtime)
    let complexity = compute_complexity_stats(root, &[]);

    // Try index first for file/line stats, fall back to filesystem walk
    let rt = tokio::runtime::Runtime::new().unwrap();
    if let Some(mut index) = rt.block_on(FileIndex::open_if_enabled(root)) {
        return rt.block_on(analyze_health_indexed(
            root,
            &mut index,
            &allow_patterns,
            complexity,
        ));
    }
    analyze_health_unindexed(root, &allow_patterns, complexity)
}

async fn analyze_health_indexed(
    _root: &Path,
    index: &mut FileIndex,
    allow_patterns: &[Pattern],
    complexity: ComplexityStats,
) -> HealthReport {
    let _ = index.incremental_refresh().await;

    let conn = index.connection();

    // Get file counts by language
    let mut files_by_language: HashMap<String, usize> = HashMap::new();
    let mut total_files = 0;

    if let Ok(mut rows) = conn
        .query("SELECT path FROM files WHERE is_dir = 0", ())
        .await
    {
        while let Ok(Some(row)) = rows.next().await {
            if let Ok(path_result) = row.get::<String>(0) {
                total_files += 1;
                let path = std::path::Path::new(&path_result);
                if let Some(lang) = rhizome_moss_languages::support_for_path(path) {
                    *files_by_language
                        .entry(lang.name().to_string())
                        .or_insert(0) += 1;
                }
            }
        }
    }

    // Get line counts from index
    let mut total_lines = 0usize;
    let mut large_files = Vec::new();

    if let Ok(files) = index.all_files().await {
        for file in files {
            if file.is_dir {
                continue;
            }
            total_lines += file.lines;
            if file.lines >= LARGE_THRESHOLD
                && !is_lockfile(&file.path)
                && !is_allowed(&file.path, allow_patterns)
            {
                large_files.push(LargeFile {
                    path: file.path,
                    lines: file.lines,
                });
            }
        }
    }

    large_files.sort_by(|a, b| b.lines.cmp(&a.lines));

    HealthReport {
        total_files,
        files_by_language,
        total_lines,
        avg_complexity: complexity.avg_complexity,
        max_complexity: complexity.max_complexity,
        high_risk_functions: complexity.high_risk_functions,
        total_functions: complexity.total_functions,
        large_files,
    }
}

/// Analyze health by walking the filesystem (no index available)
fn analyze_health_unindexed(
    root: &Path,
    allow_patterns: &[Pattern],
    complexity: ComplexityStats,
) -> HealthReport {
    use ignore::WalkBuilder;

    let mut files_by_language: HashMap<String, usize> = HashMap::new();
    let mut total_files = 0;
    let mut total_lines = 0;
    let mut large_files = Vec::new();

    let walker = WalkBuilder::new(root).hidden(true).git_ignore(true).build();

    for entry in walker.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        total_files += 1;

        if let Some(lang) = rhizome_moss_languages::support_for_path(path) {
            *files_by_language
                .entry(lang.name().to_string())
                .or_insert(0) += 1;
        }

        if let Ok(content) = std::fs::read_to_string(path) {
            let lines = content.lines().count();
            total_lines += lines;

            let rel_path = path.strip_prefix(root).unwrap_or(path);
            let rel_str = rel_path.to_string_lossy();
            if lines >= LARGE_THRESHOLD
                && !is_lockfile(&rel_str)
                && !is_allowed(&rel_str, allow_patterns)
            {
                large_files.push(LargeFile {
                    path: rel_str.to_string(),
                    lines,
                });
            }
        }
    }

    large_files.sort_by(|a, b| b.lines.cmp(&a.lines));

    HealthReport {
        total_files,
        files_by_language,
        total_lines,
        avg_complexity: complexity.avg_complexity,
        max_complexity: complexity.max_complexity,
        high_risk_functions: complexity.high_risk_functions,
        total_functions: complexity.total_functions,
        large_files,
    }
}
