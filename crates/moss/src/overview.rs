//! Codebase overview - aggregated health metrics.
//!
//! Runs multiple checks and outputs combined results:
//! - Health metrics (files, lines, complexity)
//! - Documentation coverage
//! - Import/dependency summary
//! - TODO/FIXME counts

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use rayon::prelude::*;

use crate::analyze::complexity::ComplexityAnalyzer;
use crate::deps::DepsExtractor;
use crate::path_resolve;
use crate::skeleton::SkeletonExtractor;
use moss_languages::SymbolKind;

/// Overview report aggregating multiple checks
#[derive(Debug)]
pub struct OverviewReport {
    // Files
    pub total_files: usize,
    pub files_by_language: std::collections::HashMap<String, usize>,
    pub total_lines: usize,

    // Structure
    pub total_functions: usize,
    pub total_classes: usize,
    pub total_methods: usize,

    // Complexity
    pub avg_complexity: f64,
    pub max_complexity: usize,
    pub high_risk_functions: usize,

    // Documentation
    pub functions_with_docs: usize,
    pub doc_coverage: f64,

    // Dependencies
    pub total_imports: usize,
    pub unique_modules: usize,

    // TODOs
    pub todo_count: usize,
    pub fixme_count: usize,

    // Health score
    pub health_score: f64,
    pub grade: String,
}

impl OverviewReport {
    pub fn format(&self) -> String {
        let mut lines = Vec::new();

        lines.push(format!(
            "# Project Overview  [{}] ({:.0}%)",
            self.grade,
            self.health_score * 100.0
        ));
        lines.push(String::new());

        // Files section
        lines.push("## Files".to_string());
        let lang_breakdown: Vec<_> = self
            .files_by_language
            .iter()
            .filter(|(_, count)| **count > 0)
            .map(|(lang, count)| format!("{} {}", count, lang))
            .collect();
        if lang_breakdown.is_empty() {
            lines.push(format!("  {} files", self.total_files));
        } else {
            lines.push(format!(
                "  {} files ({})",
                self.total_files,
                lang_breakdown.join(", ")
            ));
        }
        lines.push(format!("  {} lines of code", self.total_lines));
        lines.push(String::new());

        // Structure section
        lines.push("## Structure".to_string());
        lines.push(format!(
            "  {} classes, {} functions, {} methods",
            self.total_classes, self.total_functions, self.total_methods
        ));
        lines.push(format!(
            "  Doc coverage: {:.0}% ({} of {} have docstrings)",
            self.doc_coverage * 100.0,
            self.functions_with_docs,
            self.total_functions + self.total_methods
        ));
        lines.push(String::new());

        // Complexity section
        lines.push("## Complexity".to_string());
        lines.push(format!(
            "  Average: {:.1}, Maximum: {}",
            self.avg_complexity, self.max_complexity
        ));
        if self.high_risk_functions > 0 {
            lines.push(format!(
                "  High risk (>10): {} functions",
                self.high_risk_functions
            ));
        }
        lines.push(String::new());

        // Dependencies section
        lines.push("## Dependencies".to_string());
        lines.push(format!(
            "  {} imports from {} unique modules",
            self.total_imports, self.unique_modules
        ));
        lines.push(String::new());

        // TODOs section
        if self.todo_count > 0 || self.fixme_count > 0 {
            lines.push("## Open Items".to_string());
            if self.todo_count > 0 {
                lines.push(format!("  {} TODOs", self.todo_count));
            }
            if self.fixme_count > 0 {
                lines.push(format!("  {} FIXMEs", self.fixme_count));
            }
        }

        lines.join("\n")
    }

    pub fn format_compact(&self) -> String {
        format!(
            "[{}] {:.0}% | {} files, {} LOC | {:.0}% docs | {:.1} avg complexity{}",
            self.grade,
            self.health_score * 100.0,
            self.total_files,
            self.total_lines,
            self.doc_coverage * 100.0,
            self.avg_complexity,
            if self.todo_count > 0 {
                format!(" | {} TODOs", self.todo_count)
            } else {
                String::new()
            }
        )
    }

    fn calculate_health_score(avg_complexity: f64, high_risk_ratio: f64, doc_coverage: f64) -> f64 {
        // Complexity score (40% weight)
        let complexity_score = if avg_complexity <= 3.0 {
            1.0
        } else if avg_complexity <= 5.0 {
            0.9
        } else if avg_complexity <= 7.0 {
            0.8
        } else if avg_complexity <= 10.0 {
            0.7
        } else if avg_complexity <= 15.0 {
            0.5
        } else {
            0.3
        };

        // Risk score (30% weight)
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

        // Doc coverage score (30% weight)
        let doc_score = doc_coverage;

        complexity_score * 0.4 + risk_score * 0.3 + doc_score * 0.3
    }

    fn grade_from_score(score: f64) -> String {
        if score >= 0.9 {
            "A".to_string()
        } else if score >= 0.8 {
            "B".to_string()
        } else if score >= 0.7 {
            "C".to_string()
        } else if score >= 0.6 {
            "D".to_string()
        } else {
            "F".to_string()
        }
    }
}

/// Per-file stats for parallel aggregation
struct FileStats {
    lines: usize,
    functions: usize,
    classes: usize,
    methods: usize,
    functions_with_docs: usize,
    complexity_sum: usize,
    max_complexity: usize,
    high_risk: usize,
    imports: usize,
    modules: Vec<String>,
    todos: usize,
    fixmes: usize,
}

/// Analyze codebase and produce overview report
pub fn analyze_overview(root: &Path) -> OverviewReport {
    let all_files = path_resolve::all_files(root);
    let files: Vec<_> = all_files.iter().filter(|f| f.kind == "file").collect();

    // Thread-safe language file counts
    let files_by_language: Mutex<HashMap<String, usize>> = Mutex::new(HashMap::new());

    // Process files in parallel
    let stats: Vec<FileStats> = files
        .par_iter()
        .filter_map(|file| {
            let path = root.join(&file.path);
            let lang = moss_languages::support_for_path(&path);

            // Count files by language
            if let Some(l) = lang {
                let mut counts = files_by_language.lock().unwrap();
                *counts.entry(l.name().to_string()).or_insert(0) += 1;
            }

            let content = std::fs::read_to_string(&path).ok()?;
            let lines = content.lines().count();

            // Count TODOs and FIXMEs
            let todos = content.matches("TODO").count();
            let fixmes = content.matches("FIXME").count();

            // Skip detailed analysis for files without language support
            if lang.is_none() || !lang.unwrap().has_symbols() {
                return Some(FileStats {
                    lines,
                    functions: 0,
                    classes: 0,
                    methods: 0,
                    functions_with_docs: 0,
                    complexity_sum: 0,
                    max_complexity: 0,
                    high_risk: 0,
                    imports: 0,
                    modules: Vec::new(),
                    todos,
                    fixmes,
                });
            }

            // Complexity analysis
            let complexity_analyzer = ComplexityAnalyzer::new();
            let complexity_report = complexity_analyzer.analyze(&path, &content);

            let mut functions = 0;
            let mut complexity_sum = 0;
            let mut max_complexity = 0;
            let mut high_risk = 0;

            for func in &complexity_report.functions {
                if func.parent.is_none() {
                    functions += 1;
                }
                complexity_sum += func.complexity;
                if func.complexity > max_complexity {
                    max_complexity = func.complexity;
                }
                if func.complexity > 10 {
                    high_risk += 1;
                }
            }

            // Skeleton analysis for structure and doc coverage
            let skeleton_extractor = SkeletonExtractor::new();
            let skeleton = skeleton_extractor.extract(&path, &content);

            let mut classes = 0;
            let mut methods = 0;
            let mut functions_with_docs = 0;

            fn count_symbols(
                symbols: &[crate::skeleton::SkeletonSymbol],
                classes: &mut usize,
                methods: &mut usize,
                functions_with_docs: &mut usize,
            ) {
                for sym in symbols {
                    match sym.kind {
                        SymbolKind::Class => *classes += 1,
                        SymbolKind::Method => {
                            *methods += 1;
                            if sym.docstring.is_some() {
                                *functions_with_docs += 1;
                            }
                        }
                        SymbolKind::Function => {
                            if sym.docstring.is_some() {
                                *functions_with_docs += 1;
                            }
                        }
                        _ => {}
                    }
                    count_symbols(&sym.children, classes, methods, functions_with_docs);
                }
            }

            count_symbols(
                &skeleton.symbols,
                &mut classes,
                &mut methods,
                &mut functions_with_docs,
            );

            // Dependencies analysis
            let deps_extractor = DepsExtractor::new();
            let deps = deps_extractor.extract(&path, &content);

            let imports = deps.imports.len();
            let modules: Vec<String> = deps.imports.iter().map(|i| i.module.clone()).collect();

            Some(FileStats {
                lines,
                functions,
                classes,
                methods,
                functions_with_docs,
                complexity_sum,
                max_complexity,
                high_risk,
                imports,
                modules,
                todos,
                fixmes,
            })
        })
        .collect();

    // Aggregate results
    let mut total_lines = 0;
    let mut total_functions = 0;
    let mut total_classes = 0;
    let mut total_methods = 0;
    let mut functions_with_docs = 0;
    let mut total_complexity = 0;
    let mut max_complexity = 0;
    let mut high_risk_functions = 0;
    let mut total_imports = 0;
    let mut all_modules: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut todo_count = 0;
    let mut fixme_count = 0;

    for stat in stats {
        total_lines += stat.lines;
        total_functions += stat.functions;
        total_classes += stat.classes;
        total_methods += stat.methods;
        functions_with_docs += stat.functions_with_docs;
        total_complexity += stat.complexity_sum;
        if stat.max_complexity > max_complexity {
            max_complexity = stat.max_complexity;
        }
        high_risk_functions += stat.high_risk;
        total_imports += stat.imports;
        for module in stat.modules {
            all_modules.insert(module);
        }
        todo_count += stat.todos;
        fixme_count += stat.fixmes;
    }

    let callable_count = total_functions + total_methods;
    let avg_complexity = if callable_count > 0 {
        total_complexity as f64 / callable_count as f64
    } else {
        0.0
    };

    let doc_coverage = if callable_count > 0 {
        functions_with_docs as f64 / callable_count as f64
    } else {
        0.0
    };

    let high_risk_ratio = if callable_count > 0 {
        high_risk_functions as f64 / callable_count as f64
    } else {
        0.0
    };

    let health_score =
        OverviewReport::calculate_health_score(avg_complexity, high_risk_ratio, doc_coverage);
    let grade = OverviewReport::grade_from_score(health_score);

    let lang_counts = files_by_language.into_inner().unwrap();

    OverviewReport {
        total_files: files.len(),
        files_by_language: lang_counts,
        total_lines,
        total_functions,
        total_classes,
        total_methods,
        avg_complexity,
        max_complexity,
        high_risk_functions,
        functions_with_docs,
        doc_coverage,
        total_imports,
        unique_modules: all_modules.len(),
        todo_count,
        fixme_count,
        health_score,
        grade,
    }
}
