//! Function length analysis - find long functions in codebase

use crate::analyze::function_length::{FunctionLength, LengthAnalyzer, LengthReport};
use crate::filter::Filter;
use crate::path_resolve;
use rayon::prelude::*;
use std::path::Path;

/// Analyze function lengths in a single file
pub fn analyze_file_length(file_path: &Path) -> Option<LengthReport> {
    let content = std::fs::read_to_string(file_path).ok()?;
    let analyzer = LengthAnalyzer::new();
    Some(analyzer.analyze(file_path, &content))
}

/// Analyze function lengths across a codebase, returning longest functions
pub fn analyze_codebase_length(
    root: &Path,
    limit: usize,
    filter: Option<&Filter>,
    allowlist: &[String],
) -> LengthReport {
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

    let all_functions: Vec<FunctionLength> = code_files
        .par_iter()
        .filter_map(|file| {
            let path = root.join(&file.path);
            let content = std::fs::read_to_string(&path).ok()?;
            let analyzer = LengthAnalyzer::new();
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

    // Filter by allowlist
    let mut sorted: Vec<_> = if allowlist.is_empty() {
        all_functions
    } else {
        all_functions
            .into_iter()
            .filter(|f| {
                let key = f.qualified_name();
                !allowlist.iter().any(|a| key.contains(a))
            })
            .collect()
    };

    sorted.sort_by(|a, b| b.lines.cmp(&a.lines));
    sorted.truncate(limit);

    LengthReport {
        functions: sorted,
        file_path: root.to_string_lossy().to_string(),
    }
}
