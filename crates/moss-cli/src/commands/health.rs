//! Health command - analyze codebase health.

use crate::health;
use std::path::Path;

/// Analyze codebase health
pub fn cmd_health(root: Option<&Path>, json: bool) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    let report = health::analyze_health(&root);

    if json {
        let large_files: Vec<_> = report
            .large_files
            .iter()
            .map(|lf| {
                serde_json::json!({
                    "path": lf.path,
                    "lines": lf.lines,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({
                "total_files": report.total_files,
                "files_by_language": report.files_by_language,
                "total_lines": report.total_lines,
                "total_functions": report.total_functions,
                "avg_complexity": (report.avg_complexity * 10.0).round() / 10.0,
                "max_complexity": report.max_complexity,
                "high_risk_functions": report.high_risk_functions,
                "large_files": large_files,
            })
        );
    } else {
        println!("{}", report.format());
    }

    0
}
