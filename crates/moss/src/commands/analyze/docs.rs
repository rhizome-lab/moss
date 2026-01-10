//! Documentation coverage analysis

use crate::filter::Filter;
use std::collections::HashMap;
use std::path::Path;

/// Doc coverage info for a single file
#[derive(Debug, Clone)]
pub struct FileDocCoverage {
    pub file_path: String,
    pub documented: usize,
    pub total: usize,
}

impl FileDocCoverage {
    /// Bayesian-adjusted coverage for sorting.
    /// Uses Beta(1,1) prior (add 1 success and 1 failure).
    pub fn bayesian_coverage(&self) -> f64 {
        (self.documented as f64 + 1.0) / (self.total as f64 + 2.0)
    }

    pub fn coverage_percent(&self) -> f64 {
        if self.total == 0 {
            100.0 // No callables = trivially 100% documented
        } else {
            100.0 * self.documented as f64 / self.total as f64
        }
    }
}

/// Documentation coverage report
pub struct DocCoverageReport {
    pub total_callables: usize,
    pub documented: usize,
    pub coverage_percent: f64,
    pub by_language: HashMap<String, (usize, usize)>, // (documented, total)
    pub worst_files: Vec<FileDocCoverage>,
}

impl DocCoverageReport {
    pub fn format(&self) -> String {
        let mut lines = Vec::new();

        lines.push("# Documentation Coverage".to_string());
        lines.push(String::new());

        // Overall stats
        lines.push(format!(
            "Overall: {:.0}% ({} of {} documented)",
            self.coverage_percent, self.documented, self.total_callables
        ));
        lines.push(String::new());

        // Per-language breakdown
        if !self.by_language.is_empty() {
            lines.push("## By Language".to_string());
            let mut langs: Vec<_> = self.by_language.iter().collect();
            langs.sort_by(|a, b| {
                let pct_a = if a.1.1 > 0 {
                    a.1.0 as f64 / a.1.1 as f64
                } else {
                    1.0
                };
                let pct_b = if b.1.1 > 0 {
                    b.1.0 as f64 / b.1.1 as f64
                } else {
                    1.0
                };
                pct_a.partial_cmp(&pct_b).unwrap()
            });
            for (lang, (documented, total)) in langs {
                if *total > 0 {
                    let pct = 100.0 * *documented as f64 / *total as f64;
                    lines.push(format!(
                        "  {:>3.0}% ({:>3}/{:>4}) {}",
                        pct, documented, total, lang
                    ));
                }
            }
            lines.push(String::new());
        }

        // Worst files
        if !self.worst_files.is_empty() {
            lines.push("## Worst Coverage".to_string());
            for fc in &self.worst_files {
                lines.push(format!(
                    "  {:>3.0}% ({:>3}/{:>4}) {}",
                    fc.coverage_percent(),
                    fc.documented,
                    fc.total,
                    fc.file_path
                ));
            }
        }

        lines.join("\n")
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "total_callables": self.total_callables,
            "documented": self.documented,
            "coverage_percent": (self.coverage_percent * 10.0).round() / 10.0,
            "by_language": self.by_language.iter().map(|(lang, (doc, total))| {
                (lang.clone(), serde_json::json!({
                    "documented": doc,
                    "total": total,
                    "percent": if *total > 0 { (1000.0 * *doc as f64 / *total as f64).round() / 10.0 } else { 0.0 }
                }))
            }).collect::<serde_json::Map<String, serde_json::Value>>(),
            "worst_files": self.worst_files.iter().map(|fc| {
                serde_json::json!({
                    "file": fc.file_path,
                    "documented": fc.documented,
                    "total": fc.total,
                    "percent": (fc.coverage_percent() * 10.0).round() / 10.0
                })
            }).collect::<Vec<_>>()
        })
    }
}

/// Run documentation coverage analysis
pub fn cmd_docs(root: &Path, limit: usize, json: bool, filter: Option<&Filter>) -> i32 {
    let config = crate::config::MossConfig::load(root);
    let exclude_interface_impls = config.analyze.exclude_interface_impls();
    let report = analyze_docs(root, limit, exclude_interface_impls, filter);

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

/// Analyze documentation coverage
pub fn analyze_docs(
    root: &Path,
    limit: usize,
    exclude_interface_impls: bool,
    filter: Option<&Filter>,
) -> DocCoverageReport {
    use crate::extract::{IndexedResolver, InterfaceResolver, OnDemandResolver};
    use crate::index::FileIndex;
    use crate::path_resolve;

    let all_files = path_resolve::all_files(root);
    let files: Vec<_> = all_files
        .iter()
        .filter(|f| f.kind == "file")
        .filter(|f| !is_test_file(&f.path))
        .filter(|f| {
            if let Some(flt) = filter {
                flt.matches(Path::new(&f.path))
            } else {
                true
            }
        })
        .collect();

    // Try to load index for cross-file resolution, fall back to on-demand parsing
    let rt = tokio::runtime::Runtime::new().unwrap();
    let index = rt.block_on(FileIndex::open(root)).ok();
    let resolver: Box<dyn InterfaceResolver> = match &index {
        Some(idx) => Box::new(IndexedResolver::new(idx)),
        None => Box::new(OnDemandResolver::new(root)),
    };

    let mut by_language: HashMap<String, (usize, usize)> = HashMap::new();
    let mut file_coverages: Vec<FileDocCoverage> = Vec::new();

    // Process files sequentially
    for file in &files {
        process_file(
            file,
            root,
            exclude_interface_impls,
            &*resolver,
            &mut by_language,
            &mut file_coverages,
        );
    }

    // Sort by Bayesian coverage (worst first)
    file_coverages.sort_by(|a, b| {
        a.bayesian_coverage()
            .partial_cmp(&b.bayesian_coverage())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let worst_files: Vec<FileDocCoverage> = file_coverages.into_iter().take(limit).collect();

    // Calculate totals
    let total_callables: usize = by_language.values().map(|(_, t)| t).sum();
    let documented: usize = by_language.values().map(|(d, _)| d).sum();
    let coverage_percent = if total_callables > 0 {
        100.0 * documented as f64 / total_callables as f64
    } else {
        0.0
    };

    DocCoverageReport {
        total_callables,
        documented,
        coverage_percent,
        by_language,
        worst_files,
    }
}

fn process_file(
    file: &crate::path_resolve::PathMatch,
    root: &Path,
    exclude_interface_impls: bool,
    resolver: &dyn crate::extract::InterfaceResolver,
    by_language: &mut HashMap<String, (usize, usize)>,
    file_coverages: &mut Vec<FileDocCoverage>,
) {
    use crate::skeleton::SkeletonExtractor;
    use rhizome_moss_languages::SymbolKind;

    let path = root.join(&file.path);
    let lang = rhizome_moss_languages::support_for_path(&path);

    if lang.is_none() || !lang.unwrap().has_symbols() {
        return;
    }

    let lang = lang.unwrap();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let skeleton_extractor = SkeletonExtractor::new();
    let skeleton = skeleton_extractor
        .extract_with_resolver(&path, &content, Some(resolver))
        .filter_tests();

    let mut documented = 0;
    let mut total = 0;

    fn count_docs(
        symbols: &[crate::skeleton::SkeletonSymbol],
        documented: &mut usize,
        total: &mut usize,
        exclude_interface_impls: bool,
    ) {
        for sym in symbols {
            // Skip interface implementations if configured
            if exclude_interface_impls && sym.is_interface_impl {
                continue;
            }
            match sym.kind {
                SymbolKind::Function | SymbolKind::Method => {
                    *total += 1;
                    if sym.docstring.is_some() {
                        *documented += 1;
                    }
                }
                _ => {}
            }
            count_docs(&sym.children, documented, total, exclude_interface_impls);
        }
    }

    count_docs(
        &skeleton.symbols,
        &mut documented,
        &mut total,
        exclude_interface_impls,
    );

    if total > 0 {
        // Update language stats
        let entry = by_language.entry(lang.name().to_string()).or_insert((0, 0));
        entry.0 += documented;
        entry.1 += total;

        // Add file coverage
        file_coverages.push(FileDocCoverage {
            file_path: file.path.clone(),
            documented,
            total,
        });
    }
}

/// Check if a file is a test file (should be excluded from doc coverage)
fn is_test_file(path: &str) -> bool {
    let path_lower = path.to_lowercase();

    // Check path components for test directories
    if path_lower.contains("/tests/")
        || path_lower.contains("/test/")
        || path_lower.contains("/__tests__/")
    {
        return true;
    }

    // Get filename
    let filename = path.rsplit('/').next().unwrap_or(path);
    let name_lower = filename.to_lowercase();

    // Common test file patterns
    name_lower.ends_with("_test.rs")
        || name_lower.ends_with("_tests.rs")
        || name_lower.starts_with("test_")
        || name_lower.ends_with(".test.ts")
        || name_lower.ends_with(".test.js")
        || name_lower.ends_with(".spec.ts")
        || name_lower.ends_with(".spec.js")
}
