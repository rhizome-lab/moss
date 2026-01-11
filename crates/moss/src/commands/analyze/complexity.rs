//! Complexity analysis - find complex functions in codebase

use crate::analyze::complexity::{ComplexityAnalyzer, ComplexityReport};
use crate::filter::Filter;
use crate::path_resolve;
use rayon::prelude::*;
use std::path::Path;

/// Analyze complexity of a single file
pub fn analyze_file_complexity(file_path: &Path) -> Option<ComplexityReport> {
    let content = std::fs::read_to_string(file_path).ok()?;
    let analyzer = ComplexityAnalyzer::new();
    Some(analyzer.analyze(file_path, &content))
}

/// Analyze complexity across a codebase, returning top complex functions
pub fn analyze_codebase_complexity(
    root: &Path,
    limit: usize,
    threshold: Option<usize>,
    filter: Option<&Filter>,
    allowlist: &[String],
) -> ComplexityReport {
    let all_files = path_resolve::all_files(root);
    let code_files: Vec<_> = all_files
        .iter()
        .filter(|f| {
            f.kind == "file" && {
                let ext = std::path::Path::new(&f.path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");
                matches!(ext, "py" | "rs")
            }
        })
        .filter(|f| {
            filter
                .map(|flt| flt.matches(Path::new(&f.path)))
                .unwrap_or(true)
        })
        .collect();

    let all_functions: Vec<_> = code_files
        .par_iter()
        .filter_map(|file| {
            let path = root.join(&file.path);
            let content = std::fs::read_to_string(&path).ok()?;
            let analyzer = ComplexityAnalyzer::new();
            let report = analyzer.analyze(&path, &content);
            Some(
                report
                    .functions
                    .into_iter()
                    .map(|mut f| {
                        f.file_path = Some(file.path.clone());
                        f
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .flatten()
        .collect();

    // Filter by threshold
    let mut filtered: Vec<_> = if let Some(t) = threshold {
        all_functions
            .into_iter()
            .filter(|f| f.complexity >= t)
            .collect()
    } else {
        all_functions
    };

    // Filter by allowlist
    if !allowlist.is_empty() {
        filtered.retain(|f| {
            let key = f.qualified_name();
            !allowlist.iter().any(|a| key.contains(a))
        });
    }

    filtered.sort_by(|a, b| b.complexity.cmp(&a.complexity));
    filtered.truncate(limit);

    ComplexityReport {
        functions: filtered,
        file_path: root.to_string_lossy().to_string(),
    }
}
