//! Analyze command - run analysis on target.

mod args;
mod call_graph;
mod check_examples;
mod check_refs;
pub mod complexity;
mod docs;
mod duplicates;
mod files;
mod health;
mod hotspots;
pub mod length;
mod lint;
mod overview;
pub mod report;
pub mod security;
mod stale_docs;
mod trace;

use crate::commands::filter::detect_project_languages;
use crate::config::MossConfig;
use crate::daemon;
use crate::filter::Filter;
use crate::merge::Merge;
pub use args::{AnalyzeArgs, AnalyzeCommand};
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
    /// Exclude interface implementations from doc coverage (default: true)
    /// This excludes trait impl methods in Rust, @Override methods in Java, etc.
    pub exclude_interface_impls: Option<bool>,
    /// Patterns to exclude from hotspots analysis (e.g., generated code, lock files)
    #[serde(default)]
    pub hotspots_exclude: Vec<String>,
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

    pub fn exclude_interface_impls(&self) -> bool {
        self.exclude_interface_impls.unwrap_or(true)
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
        Some(AnalyzeCommand::Health { target }) => {
            let report = report::analyze(
                target.as_deref(),
                &effective_root,
                true,  // health
                false, // complexity
                false, // length
                false, // security
                None,
                None,
                filter.as_ref(),
            );
            print_report(&report, json, pretty)
        }

        Some(AnalyzeCommand::Overview { compact }) => health::cmd_overview(
            Some(&effective_root),
            compact || config.analyze.compact(),
            json,
        ),

        Some(AnalyzeCommand::Complexity {
            target,
            threshold,
            kind,
        }) => {
            let report = report::analyze(
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
            let report = report::analyze(
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
            let report = report::analyze(
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

        Some(AnalyzeCommand::Docs { limit }) => docs::cmd_docs(&effective_root, limit, json),

        Some(AnalyzeCommand::Files { limit }) => files::cmd_files(&effective_root, limit, json),

        Some(AnalyzeCommand::Trace {
            symbol,
            target,
            max_depth,
            recursive,
        }) => trace::cmd_trace(
            &symbol,
            target.as_deref(),
            &effective_root,
            max_depth,
            recursive,
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

        Some(AnalyzeCommand::Hotspots) => {
            hotspots::cmd_hotspots(&effective_root, &config.analyze.hotspots_exclude, json)
        }

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

        // No subcommand: default to health analysis
        None => {
            let report = report::analyze(
                None,
                &effective_root,
                true,  // health
                false, // complexity
                false, // length
                false, // security
                None,
                None,
                filter.as_ref(),
            );
            print_report(&report, json, pretty)
        }
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

    match Filter::new(exclude, only, &config.aliases, &lang_refs) {
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
fn print_report(report: &report::AnalyzeReport, json: bool, pretty: bool) -> i32 {
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
    let report = report::analyze(
        target, root, true, // health
        true, // complexity
        true, // length
        true, // security
        None, None, filter,
    );

    if let Some(ref complexity_report) = report.complexity {
        scores.push((complexity_report.score(), weights.complexity()));
    }
    if let Some(ref security_report) = report.security {
        scores.push((security_report.score(), weights.security()));
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
        let (grade, percentage) = report::calculate_grade(&scores);
        println!();
        println!("Overall Grade: {} ({:.0}%)", grade, percentage);
    }

    exit_code
}

/// Check if a path is a source file we can analyze.
pub(crate) fn is_source_file(path: &Path) -> bool {
    moss_languages::support_for_path(path).is_some()
}
