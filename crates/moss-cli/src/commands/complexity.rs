//! Complexity command - analyze cyclomatic complexity.

use crate::{complexity, path_resolve};
use std::path::Path;

/// Analyze cyclomatic complexity of functions in a file
pub fn cmd_complexity(file: &str, root: Option<&Path>, threshold: Option<usize>, json: bool) -> i32 {
    let resolved = match path_resolve::resolve_and_read(file, root) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}", e);
            return 1;
        }
    };

    let analyzer = complexity::ComplexityAnalyzer::new();
    let report = analyzer.analyze(&resolved.abs_path, &resolved.content);

    // Filter by threshold if specified
    let functions: Vec<_> = if let Some(t) = threshold {
        report
            .functions
            .into_iter()
            .filter(|f| f.complexity >= t)
            .collect()
    } else {
        report.functions
    };

    if json {
        let output: Vec<_> = functions
            .iter()
            .map(|f| {
                serde_json::json!({
                    "name": f.name,
                    "qualified_name": f.qualified_name(),
                    "complexity": f.complexity,
                    "risk_level": f.risk_level(),
                    "start_line": f.start_line,
                    "end_line": f.end_line,
                    "parent": f.parent
                })
            })
            .collect();

        let avg: f64 = if functions.is_empty() {
            0.0
        } else {
            functions.iter().map(|f| f.complexity).sum::<usize>() as f64 / functions.len() as f64
        };
        let max = functions.iter().map(|f| f.complexity).max().unwrap_or(0);
        let high_risk = functions.iter().filter(|f| f.complexity > 10).count();

        println!(
            "{}",
            serde_json::json!({
                "file": resolved.rel_path,
                "function_count": functions.len(),
                "avg_complexity": (avg * 10.0).round() / 10.0,
                "max_complexity": max,
                "high_risk_count": high_risk,
                "functions": output
            })
        );
    } else {
        println!("# {} - Complexity Analysis", resolved.rel_path);

        if functions.is_empty() {
            println!(
                "\nNo functions found{}",
                threshold
                    .map(|t| format!(" above threshold {}", t))
                    .unwrap_or_default()
            );
        } else {
            let avg = functions.iter().map(|f| f.complexity).sum::<usize>() as f64
                / functions.len() as f64;
            let max = functions.iter().map(|f| f.complexity).max().unwrap_or(0);
            let high_risk = functions.iter().filter(|f| f.complexity > 10).count();

            println!("\n## Summary");
            println!("  Functions: {}", functions.len());
            println!("  Average complexity: {:.1}", avg);
            println!("  Maximum complexity: {}", max);
            println!("  High risk (>10): {}", high_risk);

            // Sort by complexity descending
            let mut sorted = functions;
            sorted.sort_by(|a, b| b.complexity.cmp(&a.complexity));

            println!("\n## Functions (by complexity)");
            for f in &sorted {
                let parent = f
                    .parent
                    .as_ref()
                    .map(|p| format!("{}.", p))
                    .unwrap_or_default();
                println!(
                    "  {:3} [{}] {}{} (lines {}-{})",
                    f.complexity,
                    f.risk_level(),
                    parent,
                    f.name,
                    f.start_line,
                    f.end_line
                );
            }
        }
    }

    0
}
