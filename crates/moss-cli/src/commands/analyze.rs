//! Analyze command - run analysis on target.

use crate::analyze;
use crate::overview;
use std::path::{Path, PathBuf};

/// Run analysis on a target (file or directory)
pub fn cmd_analyze(
    target: Option<&str>,
    root: Option<&Path>,
    health: bool,
    complexity: bool,
    security: bool,
    show_overview: bool,
    show_storage: bool,
    compact: bool,
    threshold: Option<usize>,
    kind_filter: Option<&str>,
    json: bool,
) -> i32 {
    // --overview runs the overview report
    if show_overview {
        return cmd_overview(root, compact, json);
    }

    // --storage runs the storage usage report
    if show_storage {
        return cmd_storage(root, json);
    }

    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // If no specific flags, run all analyses
    let any_flag = health || complexity || security;
    let (run_health, run_complexity, run_security) = if !any_flag {
        (true, true, true)
    } else {
        (health, complexity, security)
    };

    let report = analyze::analyze(
        target,
        &root,
        run_health,
        run_complexity,
        run_security,
        threshold,
        kind_filter,
    );

    if json {
        println!("{}", report.to_json());
    } else {
        println!("{}", report.format());
    }

    0
}

/// Analyze codebase overview
fn cmd_overview(root: Option<&Path>, compact: bool, json: bool) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    let report = overview::analyze_overview(&root);

    if json {
        println!(
            "{}",
            serde_json::json!({
                "total_files": report.total_files,
                "files_by_language": report.files_by_language,
                "total_lines": report.total_lines,
                "total_functions": report.total_functions,
                "total_classes": report.total_classes,
                "total_methods": report.total_methods,
                "avg_complexity": (report.avg_complexity * 10.0).round() / 10.0,
                "max_complexity": report.max_complexity,
                "high_risk_functions": report.high_risk_functions,
                "functions_with_docs": report.functions_with_docs,
                "doc_coverage": (report.doc_coverage * 100.0).round() / 100.0,
                "total_imports": report.total_imports,
                "unique_modules": report.unique_modules,
                "todo_count": report.todo_count,
                "fixme_count": report.fixme_count,
                "health_score": (report.health_score * 100.0).round() / 100.0,
                "grade": report.grade
            })
        );
    } else if compact {
        println!("{}", report.format_compact());
    } else {
        println!("{}", report.format());
    }

    0
}

/// Show storage usage for index and caches
fn cmd_storage(root: Option<&Path>, json: bool) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // Project index: .moss/index.sqlite
    let index_path = root.join(".moss").join("index.sqlite");
    let index_size = std::fs::metadata(&index_path)
        .map(|m| m.len())
        .unwrap_or(0);

    // Package cache: ~/.cache/moss/packages/
    let cache_dir = get_cache_dir().map(|d| d.join("packages"));
    let cache_size = cache_dir
        .as_ref()
        .map(|d| dir_size(d))
        .unwrap_or(0);

    // Global cache: ~/.cache/moss/ (total)
    let global_cache_dir = get_cache_dir();
    let global_size = global_cache_dir
        .as_ref()
        .map(|d| dir_size(d))
        .unwrap_or(0);

    if json {
        println!(
            "{}",
            serde_json::json!({
                "index": {
                    "path": index_path.display().to_string(),
                    "bytes": index_size,
                    "human": format_size(index_size),
                },
                "package_cache": {
                    "path": cache_dir.as_ref().map(|d| d.display().to_string()),
                    "bytes": cache_size,
                    "human": format_size(cache_size),
                },
                "global_cache": {
                    "path": global_cache_dir.as_ref().map(|d| d.display().to_string()),
                    "bytes": global_size,
                    "human": format_size(global_size),
                },
                "total_bytes": index_size + global_size,
                "total_human": format_size(index_size + global_size),
            })
        );
    } else {
        println!("Storage Usage");
        println!();
        println!("Project index:   {:>10}  {}", format_size(index_size), index_path.display());
        if let Some(ref cache) = cache_dir {
            println!("Package cache:   {:>10}  {}", format_size(cache_size), cache.display());
        }
        if let Some(ref global) = global_cache_dir {
            println!("Global cache:    {:>10}  {}", format_size(global_size), global.display());
        }
        println!();
        println!("Total:           {:>10}", format_size(index_size + global_size));
    }

    0
}

/// Get cache directory: ~/.cache/moss
fn get_cache_dir() -> Option<PathBuf> {
    if let Ok(cache) = std::env::var("XDG_CACHE_HOME") {
        Some(PathBuf::from(cache).join("moss"))
    } else if let Ok(home) = std::env::var("HOME") {
        Some(PathBuf::from(home).join(".cache").join("moss"))
    } else if let Ok(home) = std::env::var("USERPROFILE") {
        Some(PathBuf::from(home).join(".cache").join("moss"))
    } else {
        None
    }
}

/// Calculate total size of a directory recursively
fn dir_size(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }

    let mut total = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                total += std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            } else if path.is_dir() {
                total += dir_size(&path);
            }
        }
    }
    total
}

/// Format bytes as human-readable size
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
