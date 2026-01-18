//! Analyze command - run analysis on target.

mod args;
pub mod ast;
pub mod call_graph;
pub mod check_examples;
pub mod check_refs;
pub mod complexity;
pub mod docs;
pub mod duplicates;
pub mod files;
pub mod hotspots;
pub mod length;
pub mod query;
pub mod report;
pub mod rules_cmd;
mod sarif;
pub mod security;
pub mod stale_docs;
pub mod trace;

use crate::analyze::complexity::{ComplexityReport, RiskLevel};
use crate::analyze::function_length::LengthReport;
use crate::commands::aliases::detect_project_languages;
use crate::config::MossConfig;
use crate::daemon;
use crate::filter::Filter;
pub use args::{AnalyzeArgs, AnalyzeCommand};
use rhizome_moss_derive::Merge;
pub use rhizome_moss_rules::{RuleOverride, RulesConfig};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Analyze command configuration.
#[derive(Debug, Clone, Deserialize, Serialize, Default, Merge, schemars::JsonSchema)]
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
    /// Syntax rules configuration
    #[serde(default)]
    pub rules: RulesConfig,
    /// Default lines of context to show in query preview
    #[serde(rename = "query-context-lines")]
    pub query_context_lines: Option<usize>,
}

/// Weights for each analysis pass (higher = more impact on grade).
#[derive(Debug, Clone, Deserialize, Serialize, Default, Merge, schemars::JsonSchema)]
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

/// Load patterns from a .moss allow file (e.g., hotspots-allow, large-files-allow)
fn load_allow_file(root: &Path, filename: &str) -> Vec<String> {
    let path = root.join(".moss").join(filename);
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    content
        .lines()
        .filter_map(|line| {
            // Strip trailing comments
            let without_comment = line.split('#').next().unwrap_or(line);
            let trimmed = without_comment.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect()
}

/// Append a pattern to a .moss allow file
fn append_to_allow_file(root: &Path, filename: &str, pattern: &str, reason: Option<&str>) -> i32 {
    // Validate filename to prevent path traversal
    if filename.contains('/') || filename.contains('\\') || filename.contains("..") {
        eprintln!("Invalid filename: {}", filename);
        return 1;
    }

    let path = root.join(".moss").join(filename);

    // Ensure .moss directory exists
    if let Err(e) = std::fs::create_dir_all(root.join(".moss")) {
        eprintln!("Failed to create .moss directory: {}", e);
        return 1;
    }

    // Check if pattern already exists (strip comments when comparing)
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    for line in existing.lines() {
        let without_comment = line.split('#').next().unwrap_or(line);
        let trimmed = without_comment.trim();
        if trimmed == pattern {
            println!("Pattern already in {}: {}", filename, pattern);
            return 0;
        }
    }

    // Build entry with optional reason comment
    let entry = if let Some(r) = reason {
        format!("{}  # {}\n", pattern, r)
    } else {
        format!("{}\n", pattern)
    };

    // Append to file
    use std::io::Write;
    let mut file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to open {}: {}", filename, e);
            return 1;
        }
    };

    if let Err(e) = file.write_all(entry.as_bytes()) {
        eprintln!("Failed to write to {}: {}", filename, e);
        return 1;
    }

    println!("Added to {}: {}", filename, pattern);
    0
}

/// Run analyze command with args.
pub fn run(args: AnalyzeArgs, format: crate::output::OutputFormat) -> i32 {
    let effective_root = args
        .root
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let config = MossConfig::load(&effective_root);
    let json = format.is_json();
    let pretty = format.is_pretty();

    // Ensure daemon is running if configured
    daemon::maybe_start_daemon(&effective_root);

    // Get files from --diff if specified
    let diff_files = if let Some(ref base) = args.diff {
        // If base is empty, detect default branch
        let effective_base = if base.is_empty() {
            match detect_default_branch(&effective_root) {
                Some(branch) => branch,
                None => {
                    eprintln!(
                        "error: Could not detect default branch. Specify explicitly: --diff main"
                    );
                    return 1;
                }
            }
        } else {
            base.clone()
        };

        match get_diff_files(&effective_root, &effective_base) {
            Ok(files) => {
                if files.is_empty() {
                    eprintln!("No changed files found relative to {}", effective_base);
                    return 0;
                }
                eprintln!(
                    "Analyzing {} changed files (vs {})",
                    files.len(),
                    effective_base
                );
                files
            }
            Err(e) => {
                eprintln!("error: {}", e);
                return 1;
            }
        }
    } else {
        Vec::new()
    };

    // Merge diff files into only patterns
    let mut only_patterns = args.only.clone();
    for file in &diff_files {
        // Add as exact path pattern (leading / means root-relative)
        only_patterns.push(format!("/{}", file));
    }

    // Build filter for --exclude and --only (returns None on error after printing message)
    let filter = if !args.exclude.is_empty() || !only_patterns.is_empty() {
        match build_filter(&effective_root, &args.exclude, &only_patterns) {
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

        Some(AnalyzeCommand::Complexity {
            target,
            threshold,
            limit,
            kind,
            sarif,
            allow,
            reason,
        }) => {
            // Handle --allow: append to allowlist and exit
            if let Some(pattern) = &allow {
                return append_to_allow_file(
                    &effective_root,
                    "complexity-allow",
                    pattern,
                    reason.as_deref(),
                );
            }

            // Load allowlist for filtering
            let allowlist = load_allow_file(&effective_root, "complexity-allow");

            // Use 0 to mean "no limit"
            let effective_limit = if limit == 0 { usize::MAX } else { limit };
            let effective_threshold = threshold.or(config.analyze.threshold());

            if sarif {
                // Run complexity analysis and output in SARIF format
                let report = complexity::analyze_codebase_complexity(
                    &effective_root,
                    effective_limit,
                    effective_threshold,
                    filter.as_ref(),
                    &allowlist,
                );
                sarif::print_complexity_sarif(&report.functions, &effective_root);
                0
            } else {
                // For custom limit, call complexity directly to avoid hardcoded limit in report
                let analysis_root = target
                    .as_ref()
                    .map(|t| effective_root.join(t))
                    .unwrap_or_else(|| effective_root.clone());

                let report = complexity::analyze_codebase_complexity(
                    &analysis_root,
                    effective_limit,
                    effective_threshold,
                    filter.as_ref(),
                    &allowlist,
                );

                // Note: kind filter not applicable to complexity (no kind field)
                let _ = kind;

                if json {
                    println!("{}", serde_json::to_string(&report).unwrap_or_default());
                } else if pretty {
                    print_complexity_report_pretty(&report);
                } else {
                    print_complexity_report(&report);
                }
                0
            }
        }

        Some(AnalyzeCommand::Length {
            target,
            sarif,
            allow,
            reason,
        }) => {
            // Handle --allow: append to allowlist and exit
            if let Some(pattern) = &allow {
                return append_to_allow_file(
                    &effective_root,
                    "length-allow",
                    pattern,
                    reason.as_deref(),
                );
            }

            // Load allowlist for filtering
            let allowlist = load_allow_file(&effective_root, "length-allow");

            if sarif {
                // Run length analysis and output in SARIF format
                let report = length::analyze_codebase_length(
                    &effective_root,
                    usize::MAX, // no limit for SARIF
                    filter.as_ref(),
                    &allowlist,
                );
                sarif::print_length_sarif(&report.functions, &effective_root);
                0
            } else {
                // Use direct length analysis instead of report to support allowlist
                let analysis_root = target
                    .as_ref()
                    .map(|t| effective_root.join(t))
                    .unwrap_or_else(|| effective_root.clone());

                let report = length::analyze_codebase_length(
                    &analysis_root,
                    20, // default limit for length
                    filter.as_ref(),
                    &allowlist,
                );

                if json {
                    println!("{}", serde_json::to_string(&report).unwrap_or_default());
                } else if pretty {
                    print_length_report_pretty(&report);
                } else {
                    print_length_report(&report);
                }
                0
            }
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

        Some(AnalyzeCommand::Docs { limit }) => {
            docs::cmd_docs(&effective_root, limit, json, filter.as_ref())
        }

        Some(AnalyzeCommand::Files {
            limit,
            allow,
            reason,
        }) => {
            if let Some(pattern) = allow {
                append_to_allow_file(
                    &effective_root,
                    "large-files-allow",
                    &pattern,
                    reason.as_deref(),
                )
            } else {
                let excludes = load_allow_file(&effective_root, "large-files-allow");
                files::cmd_files(&effective_root, limit, &excludes, json)
            }
        }

        Some(AnalyzeCommand::Trace {
            symbol,
            target,
            max_depth,
            recursive,
            case_insensitive,
        }) => trace::cmd_trace(
            &symbol,
            target.as_deref(),
            &effective_root,
            max_depth,
            recursive,
            case_insensitive,
            json,
            pretty,
        ),

        Some(AnalyzeCommand::Callers {
            symbol,
            case_insensitive,
        }) => call_graph::cmd_call_graph(
            &effective_root,
            &symbol,
            true,
            false,
            case_insensitive,
            json,
        ),

        Some(AnalyzeCommand::Callees {
            symbol,
            case_insensitive,
        }) => call_graph::cmd_call_graph(
            &effective_root,
            &symbol,
            false,
            true,
            case_insensitive,
            json,
        ),

        Some(AnalyzeCommand::Hotspots { allow, reason }) => {
            if let Some(pattern) = allow {
                append_to_allow_file(
                    &effective_root,
                    "hotspots-allow",
                    &pattern,
                    reason.as_deref(),
                )
            } else {
                let mut excludes = config.analyze.hotspots_exclude.clone();
                excludes.extend(load_allow_file(&effective_root, "hotspots-allow"));
                hotspots::cmd_hotspots(&effective_root, &excludes, json)
            }
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
                let result = duplicates::cmd_duplicate_functions_with_count(
                    &effective_root,
                    elide_identifiers,
                    elide_literals,
                    show_source,
                    min_lines,
                    json,
                    filter.as_ref(),
                );
                result.exit_code
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

        Some(AnalyzeCommand::Ast { file, at, sexp }) => ast::cmd_ast(&file, at, sexp, json),

        Some(AnalyzeCommand::Query {
            pattern,
            path,
            show_source,
            context,
        }) => {
            let context_lines = context.or(config.analyze.query_context_lines).unwrap_or(10);
            query::cmd_query(
                &pattern,
                path.as_deref(),
                filter.as_ref(),
                show_source,
                context_lines,
                &format,
            )
        }

        Some(AnalyzeCommand::Rules {
            rule,
            list,
            fix,
            sarif,
            target,
            debug,
        }) => {
            let target_root = target
                .as_ref()
                .map(PathBuf::from)
                .unwrap_or_else(|| effective_root.clone());
            let debug_flags = rhizome_moss_rules::DebugFlags::from_args(&debug);
            rules_cmd::cmd_rules(
                &target_root,
                rule.as_deref(),
                list,
                fix,
                json,
                sarif,
                &config.analyze.rules,
                &debug_flags,
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

/// Detect the default remote (usually "origin")
fn detect_default_remote(root: &Path) -> Option<String> {
    // Check if current branch has an upstream
    let upstream = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "@{upstream}"])
        .current_dir(root)
        .output()
        .ok()?;

    if upstream.status.success() {
        let upstream_ref = String::from_utf8_lossy(&upstream.stdout).trim().to_string();
        // origin/main -> origin
        if let Some(remote) = upstream_ref.split('/').next() {
            return Some(remote.to_string());
        }
    }

    // Fallback: check if origin exists
    let check = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(root)
        .output()
        .ok()?;

    if check.status.success() {
        return Some("origin".to_string());
    }

    // Last resort: first remote
    let remotes = Command::new("git")
        .args(["remote"])
        .current_dir(root)
        .output()
        .ok()?;

    if remotes.status.success() {
        let first = String::from_utf8_lossy(&remotes.stdout)
            .lines()
            .next()?
            .to_string();
        if !first.is_empty() {
            return Some(first);
        }
    }

    None
}

/// Detect the default branch from the default remote
fn detect_default_branch(root: &Path) -> Option<String> {
    let remote = detect_default_remote(root)?;

    // Try git symbolic-ref refs/remotes/{remote}/HEAD
    let output = Command::new("git")
        .args(["symbolic-ref", &format!("refs/remotes/{}/HEAD", remote)])
        .current_dir(root)
        .output()
        .ok()?;

    if output.status.success() {
        let full_ref = String::from_utf8_lossy(&output.stdout).trim().to_string();
        // refs/remotes/origin/main -> origin/main
        return full_ref
            .strip_prefix("refs/remotes/")
            .map(|s| s.to_string());
    }

    // Fallback: try common remote branch names
    for branch in ["main", "master"] {
        let full_branch = format!("{}/{}", remote, branch);
        let check = Command::new("git")
            .args(["rev-parse", "--verify", &full_branch])
            .current_dir(root)
            .output()
            .ok()?;
        if check.status.success() {
            return Some(full_branch);
        }
    }

    // Last fallback: try local branches
    for branch in ["main", "master"] {
        let check = Command::new("git")
            .args(["rev-parse", "--verify", branch])
            .current_dir(root)
            .output()
            .ok()?;
        if check.status.success() {
            return Some(branch.to_string());
        }
    }

    None
}

/// Get files changed relative to a base ref using git
fn get_diff_files(root: &Path, base: &str) -> Result<Vec<String>, String> {
    // Try merge-base first for branch comparisons
    let merge_base = Command::new("git")
        .args(["merge-base", base, "HEAD"])
        .current_dir(root)
        .output()
        .map_err(|e| format!("Failed to run git merge-base: {}", e))?;

    let base_ref = if merge_base.status.success() {
        String::from_utf8_lossy(&merge_base.stdout)
            .trim()
            .to_string()
    } else {
        // Fall back to using base directly (for HEAD~N style refs)
        base.to_string()
    };

    // Get changed files
    let output = Command::new("git")
        .args(["diff", "--name-only", &base_ref])
        .current_dir(root)
        .output()
        .map_err(|e| format!("Failed to run git diff: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "git diff failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let files: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect();

    Ok(files)
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

    // 1. Main analysis (health, complexity, length, security)
    if !json {
        eprintln!("Running: health, complexity, length, security...");
    }
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

    // 2. Duplicate functions
    if !json {
        eprintln!("Running: duplicate-functions...");
    }
    let dup_result = duplicates::cmd_duplicate_functions_with_count(
        root, true,  // elide_identifiers
        false, // elide_literals
        false, // show_source
        1,     // min_lines
        json, filter,
    );

    if dup_result.exit_code != 0 {
        exit_code = dup_result.exit_code;
    }

    let dup_score = if dup_result.group_count == 0 {
        100.0
    } else {
        (100.0 - (dup_result.group_count as f64 * 5.0)).max(0.0)
    };
    scores.push((dup_score, weights.duplicate_functions()));

    // 3. Duplicate types
    if !json {
        eprintln!("Running: duplicate-types...");
    }
    let dup_types_result = duplicates::cmd_duplicate_types(root, root, 70, json);
    if dup_types_result != 0 {
        exit_code = dup_types_result;
    }

    // 4. Documentation coverage
    if !json {
        eprintln!("Running: docs...");
    }
    let docs_result = docs::cmd_docs(root, 10, json, filter);
    if docs_result != 0 {
        exit_code = docs_result;
    }

    // 5. Longest files
    if !json {
        eprintln!("Running: files...");
    }
    let excludes = load_allow_file(root, "large-files-allow");
    let files_result = files::cmd_files(root, 20, &excludes, json);
    if files_result != 0 {
        exit_code = files_result;
    }

    // 6. Git hotspots
    if !json {
        eprintln!("Running: hotspots...");
    }
    let config = MossConfig::load(root);
    let mut hotspot_excludes = config.analyze.hotspots_exclude.clone();
    hotspot_excludes.extend(load_allow_file(root, "hotspots-allow"));
    let hotspots_result = hotspots::cmd_hotspots(root, &hotspot_excludes, json);
    if hotspots_result != 0 {
        exit_code = hotspots_result;
    }

    // 7. Documentation reference checks
    if !json {
        eprintln!("Running: check-refs...");
    }
    let refs_result = check_refs::cmd_check_refs(root, json);
    if refs_result != 0 {
        exit_code = refs_result;
    }

    // 8. Stale documentation
    if !json {
        eprintln!("Running: stale-docs...");
    }
    let stale_result = stale_docs::cmd_stale_docs(root, json);
    if stale_result != 0 {
        exit_code = stale_result;
    }

    // 9. Example references
    if !json {
        eprintln!("Running: check-examples...");
    }
    let examples_result = check_examples::cmd_check_examples(root, json);
    if examples_result != 0 {
        exit_code = examples_result;
    }

    // Print overall grade
    if !json && !scores.is_empty() {
        let grade = report::calculate_grade(&scores);
        println!();
        println!("Overall Grade: {} ({:.0}%)", grade.letter, grade.percentage);
    }

    exit_code
}

/// Check if a path is a source file we can analyze.
pub(crate) fn is_source_file(path: &Path) -> bool {
    rhizome_moss_languages::support_for_path(path).is_some()
}

/// Print complexity report in plain format
fn print_complexity_report(report: &ComplexityReport) {
    println!("# Complexity Analysis");
    println!();

    // Use full_stats if available (computed before truncation), otherwise use report methods
    if let Some(ref stats) = report.full_stats {
        let shown = report.functions.len();
        if stats.total_count > shown {
            println!("Functions: {} (showing {})", stats.total_count, shown);
        } else {
            println!("Functions: {}", stats.total_count);
        }
        println!("Average: {:.1}", stats.total_avg);
        println!("Maximum: {}", stats.total_max);

        if stats.critical_count > 0 {
            println!("Critical (>20): {}", stats.critical_count);
        }
        if stats.high_count > 0 || stats.critical_count == 0 {
            println!("High risk (11-20): {}", stats.high_count);
        }
    } else {
        println!("Functions: {}", report.functions.len());
        println!("Average: {:.1}", report.avg_complexity());
        println!("Maximum: {}", report.max_complexity());

        let crit = report.critical_risk_count();
        let high = report.high_risk_count();
        if crit > 0 {
            println!("Critical (>20): {}", crit);
        }
        if high > 0 || crit == 0 {
            println!("High risk (11-20): {}", high);
        }
    }

    if !report.functions.is_empty() {
        println!();
        println!("## Complex Functions");

        let mut current_risk: Option<RiskLevel> = None;
        for func in &report.functions {
            let risk = func.risk_level();
            if Some(risk) != current_risk {
                println!("### {}", risk.as_title());
                current_risk = Some(risk);
            }
            let display_name = if let Some(ref fp) = func.file_path {
                format!("{}:{}", fp, func.short_name())
            } else {
                func.short_name()
            };
            println!("{} {}", func.complexity, display_name);
        }
    }
}

/// Print complexity report in pretty format with colors
fn print_complexity_report_pretty(report: &ComplexityReport) {
    use nu_ansi_term::{Color, Style};

    println!("{}", Style::new().bold().paint("Complexity Analysis"));
    println!();

    // Use full_stats if available (computed before truncation), otherwise use report methods
    if let Some(ref stats) = report.full_stats {
        let shown = report.functions.len();
        if stats.total_count > shown {
            println!("Functions: {} (showing {})", stats.total_count, shown);
        } else {
            println!("Functions: {}", stats.total_count);
        }
        println!("Average: {:.1}", stats.total_avg);
        println!("Maximum: {}", stats.total_max);

        if stats.critical_count > 0 {
            println!(
                "{}: {}",
                Color::Red.paint("Critical (>20)"),
                stats.critical_count
            );
        }
        if stats.high_count > 0 || stats.critical_count == 0 {
            println!(
                "{}: {}",
                Color::Yellow.paint("High risk (11-20)"),
                stats.high_count
            );
        }
    } else {
        println!("Functions: {}", report.functions.len());
        println!("Average: {:.1}", report.avg_complexity());
        println!("Maximum: {}", report.max_complexity());

        let crit = report.critical_risk_count();
        let high = report.high_risk_count();
        if crit > 0 {
            println!("{}: {}", Color::Red.paint("Critical (>20)"), crit);
        }
        if high > 0 || crit == 0 {
            println!("{}: {}", Color::Yellow.paint("High risk (11-20)"), high);
        }
    }

    if !report.functions.is_empty() {
        println!();
        println!("{}", Style::new().bold().paint("Complex Functions"));

        let mut current_risk: Option<RiskLevel> = None;
        for func in &report.functions {
            let risk = func.risk_level();
            if Some(risk) != current_risk {
                let title = risk.as_title();
                let colored_title = match risk {
                    RiskLevel::Critical => Color::Red.paint(title),
                    RiskLevel::High => Color::Yellow.paint(title),
                    RiskLevel::Moderate => Color::Cyan.paint(title),
                    RiskLevel::Low => Color::Green.paint(title),
                };
                println!();
                println!("{}", colored_title);
                current_risk = Some(risk);
            }
            let display_name = if let Some(ref fp) = func.file_path {
                format!("{}:{}", fp, func.short_name())
            } else {
                func.short_name()
            };
            let complexity_str = format!("{:3}", func.complexity);
            let colored_complexity = match risk {
                RiskLevel::Critical => Color::Red.paint(&complexity_str),
                RiskLevel::High => Color::Yellow.paint(&complexity_str),
                RiskLevel::Moderate => Color::Cyan.paint(&complexity_str),
                RiskLevel::Low => Color::Green.paint(&complexity_str),
            };
            println!("  {} {}", colored_complexity, display_name);
        }
    }
}

/// Print length report in plain format
fn print_length_report(report: &LengthReport) {
    use crate::analyze::function_length::LengthCategory;

    println!("# Function Length Analysis");
    println!();

    // Use full_stats if available (computed before truncation), otherwise use report methods
    if let Some(ref stats) = report.full_stats {
        let shown = report.functions.len();
        if stats.total_count > shown {
            println!("Functions: {} (showing {})", stats.total_count, shown);
        } else {
            println!("Functions: {}", stats.total_count);
        }
        println!("Average: {:.1} lines", stats.total_avg);
        println!("Maximum: {} lines", stats.total_max);

        if stats.critical_count > 0 {
            println!("Too Long (>100): {}", stats.critical_count);
        }
        if stats.high_count > 0 || stats.critical_count == 0 {
            println!("Long (51-100): {}", stats.high_count);
        }
    } else {
        println!("Functions: {}", report.functions.len());
        println!("Average: {:.1} lines", report.avg_length());
        println!("Maximum: {} lines", report.max_length());

        let too_long = report.too_long_count();
        let long = report.long_count();
        if too_long > 0 {
            println!("Too Long (>100): {}", too_long);
        }
        if long > 0 || too_long == 0 {
            println!("Long (51-100): {}", long);
        }
    }

    if !report.functions.is_empty() {
        println!();
        println!("## Longest Functions");

        let mut current_cat: Option<LengthCategory> = None;
        for func in &report.functions {
            let cat = func.category();
            if Some(cat) != current_cat {
                println!("### {}", cat.as_title());
                current_cat = Some(cat);
            }
            let display_name = if let Some(ref fp) = func.file_path {
                format!("{}:{}", fp, func.short_name())
            } else {
                func.short_name()
            };
            println!("{} lines  {}", func.lines, display_name);
        }
    }
}

/// Print length report in pretty format with colors
fn print_length_report_pretty(report: &LengthReport) {
    use crate::analyze::function_length::LengthCategory;
    use nu_ansi_term::{Color, Style};

    println!("{}", Style::new().bold().paint("Function Length Analysis"));
    println!();

    // Use full_stats if available (computed before truncation), otherwise use report methods
    if let Some(ref stats) = report.full_stats {
        let shown = report.functions.len();
        if stats.total_count > shown {
            println!("Functions: {} (showing {})", stats.total_count, shown);
        } else {
            println!("Functions: {}", stats.total_count);
        }
        println!("Average: {:.1} lines", stats.total_avg);
        println!("Maximum: {} lines", stats.total_max);

        if stats.critical_count > 0 {
            println!(
                "{}: {}",
                Color::Red.paint("Too Long (>100)"),
                stats.critical_count
            );
        }
        if stats.high_count > 0 || stats.critical_count == 0 {
            println!(
                "{}: {}",
                Color::Yellow.paint("Long (51-100)"),
                stats.high_count
            );
        }
    } else {
        println!("Functions: {}", report.functions.len());
        println!("Average: {:.1} lines", report.avg_length());
        println!("Maximum: {} lines", report.max_length());

        let too_long = report.too_long_count();
        let long = report.long_count();
        if too_long > 0 {
            println!("{}: {}", Color::Red.paint("Too Long (>100)"), too_long);
        }
        if long > 0 || too_long == 0 {
            println!("{}: {}", Color::Yellow.paint("Long (51-100)"), long);
        }
    }

    if !report.functions.is_empty() {
        println!();
        println!("{}", Style::new().bold().paint("Longest Functions"));

        let mut current_cat: Option<LengthCategory> = None;
        for func in &report.functions {
            let cat = func.category();
            if Some(cat) != current_cat {
                let title = cat.as_title();
                let colored_title = match cat {
                    LengthCategory::TooLong => Color::Red.paint(title),
                    LengthCategory::Long => Color::Yellow.paint(title),
                    LengthCategory::Medium => Color::Cyan.paint(title),
                    LengthCategory::Short => Color::Green.paint(title),
                };
                println!();
                println!("{}", colored_title);
                current_cat = Some(cat);
            }
            let display_name = if let Some(ref fp) = func.file_path {
                format!("{}:{}", fp, func.short_name())
            } else {
                func.short_name()
            };
            let lines_str = format!("{:4} lines", func.lines);
            let colored_lines = match cat {
                LengthCategory::TooLong => Color::Red.paint(&lines_str),
                LengthCategory::Long => Color::Yellow.paint(&lines_str),
                LengthCategory::Medium => Color::Cyan.paint(&lines_str),
                LengthCategory::Short => Color::Green.paint(&lines_str),
            };
            println!("  {} {}", colored_lines, display_name);
        }
    }
}
