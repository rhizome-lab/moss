//! Analyze command - run analysis on target.

mod args;
mod call_graph;
mod check_examples;
mod check_refs;
mod duplicates;
mod health;
mod hotspots;
mod lint;
mod stale_docs;
mod trace;

pub use args::{AnalyzeArgs, AnalyzeCommand};

use crate::analysis_report;
use crate::analyze::complexity::ComplexityReport;
use crate::commands::filter::detect_project_languages;
use crate::config::MossConfig;
use crate::daemon;
use crate::filter::Filter;
use crate::merge::Merge;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Analyze command configuration.
#[derive(Debug, Clone, Deserialize, Default, Merge)]
#[serde(default)]
pub struct AnalyzeConfig {
    /// Default complexity threshold for filtering
    pub threshold: Option<usize>,
    /// Use compact output by default (for --overview)
    pub compact: Option<bool>,
    /// Run health analysis by default
    pub health: Option<bool>,
    /// Run complexity analysis by default
    pub complexity: Option<bool>,
    /// Run security analysis by default
    pub security: Option<bool>,
    /// Run duplicate function detection by default
    pub duplicate_functions: Option<bool>,
    /// Weights for final grade calculation
    pub weights: Option<AnalyzeWeights>,
}

/// Weights for each analysis pass (higher = more impact on grade).
#[derive(Debug, Clone, Deserialize, Default, Merge)]
#[serde(default)]
pub struct AnalyzeWeights {
    pub health: Option<f64>,
    pub complexity: Option<f64>,
    pub security: Option<f64>,
    pub duplicate_functions: Option<f64>,
}

impl AnalyzeWeights {
    pub fn health(&self) -> f64 {
        self.health.unwrap_or(1.0)
    }
    pub fn complexity(&self) -> f64 {
        self.complexity.unwrap_or(0.5)
    }
    pub fn security(&self) -> f64 {
        self.security.unwrap_or(2.0)
    }
    pub fn duplicate_functions(&self) -> f64 {
        self.duplicate_functions.unwrap_or(0.3)
    }
}

impl AnalyzeConfig {
    pub fn threshold(&self) -> Option<usize> {
        self.threshold
    }

    pub fn compact(&self) -> bool {
        self.compact.unwrap_or(false)
    }

    pub fn health(&self) -> bool {
        self.health.unwrap_or(true)
    }

    pub fn complexity(&self) -> bool {
        self.complexity.unwrap_or(true)
    }

    pub fn security(&self) -> bool {
        self.security.unwrap_or(true)
    }

    pub fn duplicate_functions(&self) -> bool {
        self.duplicate_functions.unwrap_or(false)
    }

    pub fn weights(&self) -> AnalyzeWeights {
        self.weights.clone().unwrap_or_default()
    }
}

/// Run analyze command with args.
pub fn run(args: AnalyzeArgs, format: crate::output::OutputFormat) -> i32 {
    let effective_root = args
        .root
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let config = MossConfig::load(&effective_root);
    let json = args.json || args.jq.is_some() || format.is_json();
    let pretty = args.pretty || format.is_pretty();

    // Ensure daemon is running if configured
    daemon::maybe_start_daemon(&effective_root);

    // Build filter for --exclude and --only (returns None on error after printing message)
    let filter = if !args.exclude.is_empty() || !args.only.is_empty() {
        match build_filter(&effective_root, &args.exclude, &args.only) {
            Some(f) => Some(f),
            None => return 1, // Error already printed
        }
    } else {
        None
    };

    // Dispatch based on subcommand
    match args.command {
        Some(AnalyzeCommand::Health { compact }) => health::cmd_overview(
            Some(&effective_root),
            compact || config.analyze.compact(),
            json,
        ),

        Some(AnalyzeCommand::Complexity {
            target,
            threshold,
            kind,
        }) => {
            let report = analysis_report::analyze(
                target.as_deref(),
                &effective_root,
                false, // health
                true,  // complexity
                false, // length
                false, // security
                threshold.or(config.analyze.threshold()),
                kind.as_deref(),
                filter.as_ref(),
            );
            print_report(&report, json, pretty)
        }

        Some(AnalyzeCommand::Length { target }) => {
            let report = analysis_report::analyze(
                target.as_deref(),
                &effective_root,
                false, // health
                false, // complexity
                true,  // length
                false, // security
                None,
                None,
                filter.as_ref(),
            );
            print_report(&report, json, pretty)
        }

        Some(AnalyzeCommand::Security { target }) => {
            let report = analysis_report::analyze(
                target.as_deref(),
                &effective_root,
                false, // health
                false, // complexity
                false, // length
                true,  // security
                None,
                None,
                filter.as_ref(),
            );
            print_report(&report, json, pretty)
        }

        Some(AnalyzeCommand::Trace {
            symbol,
            target,
            max_depth,
        }) => trace::cmd_trace(
            &symbol,
            target.as_deref(),
            &effective_root,
            max_depth,
            json,
            pretty,
        ),

        Some(AnalyzeCommand::Callers { symbol }) => {
            call_graph::cmd_call_graph(&effective_root, &symbol, true, false, json)
        }

        Some(AnalyzeCommand::Callees { symbol }) => {
            call_graph::cmd_call_graph(&effective_root, &symbol, false, true, json)
        }

        Some(AnalyzeCommand::Lint { target }) => {
            lint::cmd_lint_analyze(&effective_root, target.as_deref(), json)
        }

        Some(AnalyzeCommand::Hotspots) => hotspots::cmd_hotspots(&effective_root, json),

        Some(AnalyzeCommand::CheckRefs) => check_refs::cmd_check_refs(&effective_root, json),

        Some(AnalyzeCommand::StaleDocs) => stale_docs::cmd_stale_docs(&effective_root, json),

        Some(AnalyzeCommand::CheckExamples) => {
            check_examples::cmd_check_examples(&effective_root, json)
        }

        Some(AnalyzeCommand::DuplicateFunctions {
            elide_identifiers,
            elide_literals,
            show_source,
            min_lines,
            allow,
            reason,
        }) => {
            if let Some(location) = allow {
                duplicates::cmd_allow_duplicate_function(
                    &effective_root,
                    &location,
                    reason.as_deref(),
                    elide_identifiers,
                    elide_literals,
                    min_lines,
                )
            } else {
                let (result, _) = duplicates::cmd_duplicate_functions_with_count(
                    &effective_root,
                    elide_identifiers,
                    elide_literals,
                    show_source,
                    min_lines,
                    json,
                );
                result
            }
        }

        Some(AnalyzeCommand::DuplicateTypes {
            target,
            min_overlap,
            allow,
            reason,
        }) => {
            if let Some(types) = allow {
                if types.len() == 2 {
                    duplicates::cmd_allow_duplicate_type(
                        &effective_root,
                        &types[0],
                        &types[1],
                        reason.as_deref(),
                    )
                } else {
                    eprintln!("--allow requires exactly two type names");
                    1
                }
            } else {
                let scan_root = target
                    .map(PathBuf::from)
                    .unwrap_or_else(|| effective_root.clone());
                duplicates::cmd_duplicate_types(&scan_root, &effective_root, min_overlap, json)
            }
        }

        Some(AnalyzeCommand::All { target }) => {
            let weights = config.analyze.weights();
            run_all_passes(
                target.as_deref(),
                &effective_root,
                &weights,
                filter.as_ref(),
                json,
                pretty,
            )
        }

        // No subcommand: default to health overview
        None => health::cmd_overview(Some(&effective_root), config.analyze.compact(), json),
    }
}

/// Build filter from exclude/only patterns
fn build_filter(root: &Path, exclude: &[String], only: &[String]) -> Option<Filter> {
    if exclude.is_empty() && only.is_empty() {
        return None;
    }

    let config = MossConfig::load(root);
    let languages = detect_project_languages(root);
    let lang_refs: Vec<&str> = languages.iter().map(|s| s.as_str()).collect();

    match Filter::new(exclude, only, &config.filter, &lang_refs) {
        Ok(f) => {
            for warning in f.warnings() {
                eprintln!("warning: {}", warning);
            }
            Some(f)
        }
        Err(e) => {
            eprintln!("error: {}", e);
            None
        }
    }
}

/// Print analysis report in appropriate format
fn print_report(report: &analysis_report::AnalyzeReport, json: bool, pretty: bool) -> i32 {
    if json {
        println!("{}", report.to_json());
    } else if pretty {
        println!("{}", report.format_pretty());
    } else {
        println!("{}", report.format());
    }
    0
}

/// Run all analysis passes
fn run_all_passes(
    target: Option<&str>,
    root: &Path,
    weights: &AnalyzeWeights,
    filter: Option<&Filter>,
    json: bool,
    pretty: bool,
) -> i32 {
    let mut exit_code = 0;
    let mut scores: Vec<(f64, f64)> = Vec::new();

    // Run main analysis
    let report = analysis_report::analyze(
        target, root, true, // health
        true, // complexity
        true, // length
        true, // security
        None, None, filter,
    );

    if let Some(ref complexity_report) = report.complexity {
        let score = score_complexity(complexity_report);
        scores.push((score, weights.complexity()));
    }
    if let Some(ref security_report) = report.security {
        let score = score_security(security_report);
        scores.push((score, weights.security()));
    }

    if json {
        println!("{}", report.to_json());
    } else if pretty {
        println!("{}", report.format_pretty());
    } else {
        println!("{}", report.format());
    }

    // Run duplicate function detection
    let (dup_result, dup_count) = duplicates::cmd_duplicate_functions_with_count(
        root, true,  // elide_identifiers
        false, // elide_literals
        false, // show_source
        1,     // min_lines
        json,
    );

    if dup_result != 0 {
        exit_code = dup_result;
    }

    // Score duplicates
    let dup_score = if dup_count == 0 {
        100.0
    } else {
        (100.0 - (dup_count as f64 * 5.0)).max(0.0)
    };
    scores.push((dup_score, weights.duplicate_functions()));

    // Print overall grade
    if !json && !scores.is_empty() {
        let (grade, percentage) = calculate_grade(&scores);
        println!();
        println!("Overall Grade: {} ({:.0}%)", grade, percentage);
    }

    exit_code
}

/// Score complexity: 100 if no high-risk functions, decreases with complex code
fn score_complexity(report: &ComplexityReport) -> f64 {
    let high_risk = report.high_risk_count();
    let total = report.functions.len();
    if total == 0 {
        return 100.0;
    }
    let ratio = high_risk as f64 / total as f64;
    (100.0 * (1.0 - ratio)).max(0.0)
}

/// Score security: 100 if no findings, penalized by severity
fn score_security(report: &analysis_report::SecurityReport) -> f64 {
    let counts = report.count_by_severity();
    let penalty =
        counts["critical"] * 40 + counts["high"] * 20 + counts["medium"] * 10 + counts["low"] * 5;
    (100.0 - penalty as f64).max(0.0)
}

/// Calculate weighted average grade from scores
fn calculate_grade(scores: &[(f64, f64)]) -> (&'static str, f64) {
    let total_weight: f64 = scores.iter().map(|(_, w)| w).sum();
    if total_weight == 0.0 {
        return ("N/A", 0.0);
    }
    let weighted_sum: f64 = scores.iter().map(|(s, w)| s * w).sum();
    let percentage = weighted_sum / total_weight;

    let grade = match percentage as u32 {
        90..=100 => "A",
        80..=89 => "B",
        70..=79 => "C",
        60..=69 => "D",
        _ => "F",
    };
    (grade, percentage)
}

/// Check if a path is a source file we care about
pub(crate) fn is_source_file(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => matches!(
            ext,
            "rs" | "py"
                | "js"
                | "ts"
                | "tsx"
                | "jsx"
                | "go"
                | "java"
                | "c"
                | "cpp"
                | "h"
                | "hpp"
                | "rb"
                | "php"
                | "swift"
                | "kt"
                | "scala"
                | "cs"
                | "ex"
                | "exs"
        ),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_load_duplicate_functions_allowlist_empty() {
        let tmp = tempdir().unwrap();
        let allowlist = load_duplicate_functions_allowlist(tmp.path());
        assert!(allowlist.is_empty());
    }

    #[test]
    fn test_load_duplicate_functions_allowlist_with_entries() {
        let tmp = tempdir().unwrap();
        let moss_dir = tmp.path().join(".moss");
        fs::create_dir_all(&moss_dir).unwrap();
        fs::write(
            moss_dir.join("duplicate-functions-allow"),
            "# Comment\nsrc/foo.rs:bar\nsrc/baz.rs:qux\n",
        )
        .unwrap();

        let allowlist = load_duplicate_functions_allowlist(tmp.path());
        assert_eq!(allowlist.len(), 2);
        assert!(allowlist.contains("src/foo.rs:bar"));
        assert!(allowlist.contains("src/baz.rs:qux"));
    }

    #[test]
    fn test_load_duplicate_functions_allowlist_ignores_comments() {
        let tmp = tempdir().unwrap();
        let moss_dir = tmp.path().join(".moss");
        fs::create_dir_all(&moss_dir).unwrap();
        fs::write(
            moss_dir.join("duplicate-functions-allow"),
            "# This is a comment\n# Another comment\nsrc/foo.rs:bar\n",
        )
        .unwrap();

        let allowlist = load_duplicate_functions_allowlist(tmp.path());
        assert_eq!(allowlist.len(), 1);
        assert!(allowlist.contains("src/foo.rs:bar"));
    }

    /// Helper to check if a duplicate function group is fully allowed
    fn is_group_allowed(
        locations: &[DuplicateFunctionLocation],
        allowlist: &std::collections::HashSet<String>,
    ) -> bool {
        locations
            .iter()
            .all(|loc| allowlist.contains(&format!("{}:{}", loc.file, loc.symbol)))
    }

    #[test]
    fn test_is_group_allowed_all_in_allowlist() {
        let mut allowlist = std::collections::HashSet::new();
        allowlist.insert("src/a.rs:foo".to_string());
        allowlist.insert("src/b.rs:bar".to_string());

        let locations = vec![
            DuplicateFunctionLocation {
                file: "src/a.rs".to_string(),
                symbol: "foo".to_string(),
                start_line: 1,
                end_line: 5,
            },
            DuplicateFunctionLocation {
                file: "src/b.rs".to_string(),
                symbol: "bar".to_string(),
                start_line: 10,
                end_line: 15,
            },
        ];

        assert!(is_group_allowed(&locations, &allowlist));
    }

    #[test]
    fn test_is_group_allowed_partial_not_allowed() {
        let mut allowlist = std::collections::HashSet::new();
        allowlist.insert("src/a.rs:foo".to_string());

        let locations = vec![
            DuplicateFunctionLocation {
                file: "src/a.rs".to_string(),
                symbol: "foo".to_string(),
                start_line: 1,
                end_line: 5,
            },
            DuplicateFunctionLocation {
                file: "src/b.rs".to_string(),
                symbol: "bar".to_string(),
                start_line: 10,
                end_line: 15,
            },
        ];

        assert!(!is_group_allowed(&locations, &allowlist));
    }

    #[test]
    fn test_calculate_grade_perfect() {
        // (score, weight) pairs - all 100%
        let scores = [(100.0, 1.0), (100.0, 0.5), (100.0, 2.0), (100.0, 0.3)];
        let (letter, percentage) = calculate_grade(&scores);
        assert_eq!(letter, "A");
        assert!((percentage - 100.0).abs() < 0.1);
    }

    #[test]
    fn test_calculate_grade_weights() {
        // Security weight is 2.0, so a security issue hurts more than complexity
        // 50% health (weight 1.0), 100% complexity (weight 0.5), 0% security (weight 2.0), 100% duplicate-functions
        let scores = [(50.0, 1.0), (100.0, 0.5), (0.0, 2.0), (100.0, 0.3)];
        let (_, percentage) = calculate_grade(&scores);
        // Expected: (50*1 + 100*0.5 + 0*2 + 100*0.3) / (1+0.5+2+0.3) = 130/3.8 ≈ 34.2%
        assert!(percentage < 50.0); // Security weight should drag it down
    }
}
