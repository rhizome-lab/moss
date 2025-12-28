//! Analyze command - run analysis on target.

use crate::analyze;
use crate::commands::filter::detect_project_languages;
use crate::config::MossConfig;
use crate::daemon;
use crate::filter::Filter;
use crate::index;
use crate::merge::Merge;
use crate::overview;
use crate::path_resolve;
use clap::Args;
use moss_tools::registry_with_custom;
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
}

impl AnalyzeConfig {
    pub fn threshold(&self) -> Option<usize> {
        self.threshold
    }

    pub fn compact(&self) -> bool {
        self.compact.unwrap_or(false)
    }
}

/// Analyze command arguments.
#[derive(Args, Debug)]
pub struct AnalyzeArgs {
    /// Target to analyze (path, file, or directory)
    pub target: Option<String>,

    /// Root directory (defaults to current directory)
    #[arg(short, long)]
    pub root: Option<PathBuf>,

    /// Run health analysis
    #[arg(long)]
    pub health: bool,

    /// Run complexity analysis
    #[arg(long)]
    pub complexity: bool,

    /// Run security analysis
    #[arg(long)]
    pub security: bool,

    /// Show comprehensive project overview
    #[arg(long)]
    pub overview: bool,

    /// Show storage usage
    #[arg(long)]
    pub storage: bool,

    /// Compact one-line output (for --overview)
    #[arg(short, long)]
    pub compact: bool,

    /// Complexity threshold - only show functions above this
    #[arg(short, long)]
    pub threshold: Option<usize>,

    /// Filter by symbol kind: function, method
    #[arg(long)]
    pub kind: Option<String>,

    /// Show what functions the target calls
    #[arg(long)]
    pub callees: bool,

    /// Show what functions call the target
    #[arg(long)]
    pub callers: bool,

    /// Run linters
    #[arg(long)]
    pub lint: bool,

    /// Show git history hotspots
    #[arg(long)]
    pub hotspots: bool,

    /// Check documentation references
    #[arg(long)]
    pub check_refs: bool,

    /// Find docs with stale code references
    #[arg(long)]
    pub stale_docs: bool,

    /// Check example references
    #[arg(long)]
    pub check_examples: bool,

    /// Detect code clones (duplicate functions/methods)
    #[arg(long)]
    pub clones: bool,

    /// Elide identifier names when detecting clones (default: true)
    #[arg(long, default_value = "true")]
    pub elide_identifiers: bool,

    /// Elide literal values when detecting clones (default: false)
    #[arg(long)]
    pub elide_literals: bool,

    /// Show source code for detected clones
    #[arg(long)]
    pub show_source: bool,

    /// Minimum lines for a function to be considered for clone detection
    #[arg(long, default_value = "1")]
    pub min_lines: usize,

    /// Exclude paths matching pattern or @alias
    #[arg(long, value_name = "PATTERN")]
    pub exclude: Vec<String>,

    /// Include only paths matching pattern or @alias
    #[arg(long, value_name = "PATTERN")]
    pub only: Vec<String>,
}

/// Run analyze command with args.
pub fn run(args: AnalyzeArgs, json: bool) -> i32 {
    let effective_root = args
        .root
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let config = MossConfig::load(&effective_root);

    cmd_analyze(
        args.target.as_deref(),
        args.root.as_deref(),
        args.health,
        args.complexity,
        args.security,
        args.overview,
        args.storage,
        args.compact || config.analyze.compact(),
        args.threshold.or(config.analyze.threshold()),
        args.kind.as_deref(),
        args.callees,
        args.callers,
        args.lint,
        args.hotspots,
        args.check_refs,
        args.stale_docs,
        args.check_examples,
        args.clones,
        args.elide_identifiers,
        args.elide_literals,
        args.show_source,
        args.min_lines,
        json,
        &args.exclude,
        &args.only,
    )
}

/// Run analysis on a target (file or directory)
#[allow(clippy::too_many_arguments)]
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
    callees: bool,
    callers: bool,
    lint: bool,
    hotspots: bool,
    check_refs: bool,
    stale_docs: bool,
    check_examples: bool,
    clones: bool,
    elide_identifiers: bool,
    elide_literals: bool,
    show_source: bool,
    min_lines: usize,
    json: bool,
    exclude: &[String],
    only: &[String],
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

    // Ensure daemon is running if configured
    daemon::maybe_start_daemon(&root);

    // Build filter for --exclude and --only
    let filter = if !exclude.is_empty() || !only.is_empty() {
        let config = MossConfig::load(&root);
        let languages = detect_project_languages(&root);
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
                return 1;
            }
        }
    } else {
        None
    };

    // --callees or --callers: show call graph info
    if callees || callers {
        let target = match target {
            Some(t) => t,
            None => {
                eprintln!("--callees and --callers require a target symbol");
                return 1;
            }
        };
        return cmd_call_graph(&root, target, callers, callees, json);
    }

    // --lint runs linter analysis
    if lint {
        return cmd_lint_analyze(&root, target, json);
    }

    // --hotspots runs git history hotspot analysis
    if hotspots {
        return cmd_hotspots(&root, json);
    }

    // --check-refs validates documentation references
    if check_refs {
        return cmd_check_refs(&root, json);
    }

    // --stale-docs finds docs where covered code has changed
    if stale_docs {
        return cmd_stale_docs(&root, json);
    }

    // --check-examples validates example references
    if check_examples {
        return cmd_check_examples(&root, json);
    }

    // --clones detects duplicate code
    if clones {
        return cmd_clones(
            &root,
            elide_identifiers,
            elide_literals,
            show_source,
            min_lines,
            json,
        );
    }

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
        filter.as_ref(),
    );

    if json {
        println!("{}", report.to_json());
    } else {
        println!("{}", report.format());
    }

    0
}

/// Run linter analysis on the codebase
fn cmd_lint_analyze(root: &Path, target: Option<&str>, json: bool) -> i32 {
    let registry = registry_with_custom(root);
    let detected = registry.detect(root);

    if detected.is_empty() {
        if json {
            println!("{{\"tools\": [], \"summary\": {{\"errors\": 0, \"warnings\": 0}}}}");
        } else {
            eprintln!("No relevant linting tools found for this project.");
        }
        return 0;
    }

    let paths: Vec<&Path> = target.map(|t| vec![Path::new(t)]).unwrap_or_default();
    let mut all_results = Vec::new();
    let mut tools_run = Vec::new();

    for (tool, _reason) in &detected {
        let info = tool.info();

        if !tool.is_available() {
            continue;
        }

        if !json {
            eprintln!("{}: checking...", info.name);
        }

        match tool.run(&paths.iter().copied().collect::<Vec<_>>(), root) {
            Ok(result) => {
                tools_run.push(info.name);
                all_results.push(result);
            }
            Err(e) => {
                if !json {
                    eprintln!("{}: {}", info.name, e);
                }
            }
        }
    }

    let total_errors: usize = all_results.iter().map(|r| r.error_count()).sum();
    let total_warnings: usize = all_results.iter().map(|r| r.warning_count()).sum();

    if json {
        let diagnostics = moss_tools::ToolRegistry::collect_diagnostics(&all_results);
        let output = serde_json::json!({
            "tools": tools_run,
            "summary": {
                "errors": total_errors,
                "warnings": total_warnings,
            },
            "results": all_results.iter().map(|r| {
                serde_json::json!({
                    "tool": r.tool,
                    "success": r.success,
                    "errors": r.error_count(),
                    "warnings": r.warning_count(),
                })
            }).collect::<Vec<_>>(),
            "diagnostics": diagnostics,
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        // Print diagnostics
        for result in &all_results {
            for diag in &result.diagnostics {
                let severity = match diag.severity {
                    moss_tools::DiagnosticSeverity::Error => "error",
                    moss_tools::DiagnosticSeverity::Warning => "warning",
                    moss_tools::DiagnosticSeverity::Info => "info",
                    moss_tools::DiagnosticSeverity::Hint => "hint",
                };

                println!(
                    "{}:{}:{}: {} [{}] {}",
                    diag.location.file.display(),
                    diag.location.line,
                    diag.location.column,
                    severity,
                    diag.rule_id,
                    diag.message
                );
            }
        }

        // Summary
        println!();
        println!("Lint Analysis");
        println!("  Tools: {}", tools_run.join(", "));
        println!("  Errors: {}", total_errors);
        println!("  Warnings: {}", total_warnings);

        if total_errors > 0 {
            println!();
            println!("Run 'moss lint --fix' to auto-fix issues where possible.");
        }
    }

    if total_errors > 0 {
        1
    } else {
        0
    }
}

/// Show callers/callees of a symbol
fn cmd_call_graph(
    root: &Path,
    target: &str,
    show_callers: bool,
    show_callees: bool,
    json: bool,
) -> i32 {
    // Try to parse target as file:symbol or just symbol
    let (symbol, file_hint) = if let Some((sym, file)) = parse_file_symbol_string(target) {
        (sym, Some(file))
    } else {
        (target.to_string(), None)
    };

    // Try index first
    let idx = match index::FileIndex::open_if_enabled(root) {
        Some(i) => i,
        None => {
            eprintln!("Indexing disabled or failed. Run: moss index rebuild --call-graph");
            return 1;
        }
    };

    let stats = idx.call_graph_stats().unwrap_or_default();
    if stats.calls == 0 {
        eprintln!("Call graph not indexed. Run: moss reindex --call-graph");
        return 1;
    }

    let mut results: Vec<(String, String, usize, &str)> = Vec::new(); // (file, symbol, line, direction)

    // Get callers if requested
    if show_callers {
        match idx.find_callers(&symbol) {
            Ok(callers) => {
                for (file, sym, line) in callers {
                    results.push((file, sym, line, "caller"));
                }
            }
            Err(e) => {
                eprintln!("Error finding callers: {}", e);
            }
        }
    }

    // Get callees if requested
    if show_callees {
        // Need to find file for symbol first
        let file_path = if let Some(f) = &file_hint {
            let matches = path_resolve::resolve(f, root);
            matches
                .iter()
                .find(|m| m.kind == "file")
                .map(|m| m.path.clone())
        } else {
            idx.find_symbol(&symbol)
                .ok()
                .and_then(|syms| syms.first().map(|(f, _, _, _)| f.clone()))
        };

        if let Some(file_path) = file_path {
            match idx.find_callees(&file_path, &symbol) {
                Ok(callees) => {
                    for (name, line) in callees {
                        results.push((file_path.clone(), name, line, "callee"));
                    }
                }
                Err(e) => {
                    eprintln!("Error finding callees: {}", e);
                }
            }
        }
    }

    if results.is_empty() {
        if json {
            println!("[]");
        } else {
            let direction = if show_callers && show_callees {
                "callers or callees"
            } else if show_callers {
                "callers"
            } else {
                "callees"
            };
            eprintln!("No {} found for: {}", direction, symbol);
        }
        return 1;
    }

    // Sort by file, then line
    results.sort_by(|a, b| (&a.0, a.2).cmp(&(&b.0, b.2)));

    if json {
        let output: Vec<_> = results
            .iter()
            .map(|(file, sym, line, direction)| {
                serde_json::json!({
                    "file": file,
                    "symbol": sym,
                    "line": line,
                    "direction": direction
                })
            })
            .collect();
        println!("{}", serde_json::to_string(&output).unwrap());
    } else {
        let header = if show_callers && show_callees {
            format!("Callers and callees of {}", symbol)
        } else if show_callers {
            format!("Callers of {}", symbol)
        } else {
            format!("Callees of {}", symbol)
        };
        println!("{}:", header);
        for (file, sym, line, _direction) in &results {
            println!("  {}:{}:{}", file, line, sym);
        }
    }

    0
}

/// Try various separators to parse file:symbol format
fn parse_file_symbol_string(s: &str) -> Option<(String, String)> {
    // Try various separators: #, ::, :
    for sep in ["#", "::", ":"] {
        if let Some(idx) = s.find(sep) {
            let (file, rest) = s.split_at(idx);
            let symbol = &rest[sep.len()..];
            if !file.is_empty() && !symbol.is_empty() && looks_like_file(file) {
                return Some((symbol.to_string(), file.to_string()));
            }
        }
    }
    None
}

/// Check if a string looks like a file path
fn looks_like_file(s: &str) -> bool {
    s.contains('.') || s.contains('/')
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
    let index_size = std::fs::metadata(&index_path).map(|m| m.len()).unwrap_or(0);

    // Package cache: ~/.cache/moss/packages/
    let cache_dir = get_cache_dir().map(|d| d.join("packages"));
    let cache_size = cache_dir.as_ref().map(|d| dir_size(d)).unwrap_or(0);

    // Global cache: ~/.cache/moss/ (total)
    let global_cache_dir = get_cache_dir();
    let global_size = global_cache_dir.as_ref().map(|d| dir_size(d)).unwrap_or(0);

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
        println!(
            "Project index:   {:>10}  {}",
            format_size(index_size),
            index_path.display()
        );
        if let Some(ref cache) = cache_dir {
            println!(
                "Package cache:   {:>10}  {}",
                format_size(cache_size),
                cache.display()
            );
        }
        if let Some(ref global) = global_cache_dir {
            println!(
                "Global cache:    {:>10}  {}",
                format_size(global_size),
                global.display()
            );
        }
        println!();
        println!(
            "Total:           {:>10}",
            format_size(index_size + global_size)
        );
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

/// Hotspot data for a file
#[derive(Debug)]
struct FileHotspot {
    path: String,
    commits: usize,
    lines_added: usize,
    lines_deleted: usize,
    score: f64,
}

/// Analyze git history hotspots
fn cmd_hotspots(root: &Path, json: bool) -> i32 {
    // Check if git repo
    let git_dir = root.join(".git");
    if !git_dir.exists() {
        eprintln!("Not a git repository");
        return 1;
    }

    // Get file commit counts and churn from git log
    let output = match std::process::Command::new("git")
        .args(["log", "--format=", "--numstat"])
        .current_dir(root)
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Failed to run git log: {}", e);
            return 1;
        }
    };

    if !output.status.success() {
        eprintln!("git log failed");
        return 1;
    }

    // Parse numstat output: added<TAB>deleted<TAB>path
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut file_stats: std::collections::HashMap<String, (usize, usize, usize)> =
        std::collections::HashMap::new();

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() == 3 {
            let added = parts[0].parse::<usize>().unwrap_or(0);
            let deleted = parts[1].parse::<usize>().unwrap_or(0);
            let path = parts[2].to_string();

            // Skip binary files (shown as -)
            if parts[0] == "-" || parts[1] == "-" {
                continue;
            }

            let entry = file_stats.entry(path).or_insert((0, 0, 0));
            entry.0 += 1; // commits
            entry.1 += added;
            entry.2 += deleted;
        }
    }

    // Get complexity from index
    let idx = match index::FileIndex::open_if_enabled(root) {
        Some(i) => i,
        None => {
            // No index, just use churn data
            let mut hotspots: Vec<FileHotspot> = file_stats
                .into_iter()
                .filter(|(path, _)| {
                    // Filter to source files only
                    let p = Path::new(path);
                    p.exists() && is_source_file(p)
                })
                .map(|(path, (commits, added, deleted))| {
                    let churn = added + deleted;
                    FileHotspot {
                        path,
                        commits,
                        lines_added: added,
                        lines_deleted: deleted,
                        score: (commits as f64) * (churn as f64).sqrt(),
                    }
                })
                .collect();

            hotspots.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
            hotspots.truncate(20);

            return print_hotspots(&hotspots, json);
        }
    };

    // Build hotspots from churn data (index is available but not used for complexity)
    let _ = idx; // Index available for future on-demand complexity computation
    let mut hotspots: Vec<FileHotspot> = Vec::new();

    for (path, (commits, added, deleted)) in file_stats {
        let p = Path::new(&path);
        if !p.exists() || !is_source_file(p) {
            continue;
        }

        let churn = added + deleted;
        // Score: commits * sqrt(churn)
        let score = (commits as f64) * (churn as f64).sqrt();

        hotspots.push(FileHotspot {
            path,
            commits,
            lines_added: added,
            lines_deleted: deleted,
            score,
        });
    }

    hotspots.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    hotspots.truncate(20);

    print_hotspots(&hotspots, json)
}

/// Check if a path is a source file we care about
fn is_source_file(path: &Path) -> bool {
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

/// Print hotspots report
fn print_hotspots(hotspots: &[FileHotspot], json: bool) -> i32 {
    if hotspots.is_empty() {
        if json {
            println!("[]");
        } else {
            println!("No hotspots found (no git history or source files)");
        }
        return 0;
    }

    if json {
        let output: Vec<_> = hotspots
            .iter()
            .map(|h| {
                serde_json::json!({
                    "path": h.path,
                    "commits": h.commits,
                    "lines_added": h.lines_added,
                    "lines_deleted": h.lines_deleted,
                    "churn": h.lines_added + h.lines_deleted,
                    "score": (h.score * 10.0).round() / 10.0,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!("Git Hotspots (high churn)");
        println!();
        println!(
            "{:<50} {:>8} {:>8} {:>8}",
            "File", "Commits", "Churn", "Score"
        );
        println!("{}", "-".repeat(80));

        for h in hotspots {
            let churn = h.lines_added + h.lines_deleted;
            let display_path = if h.path.len() > 48 {
                format!("...{}", &h.path[h.path.len() - 45..])
            } else {
                h.path.clone()
            };
            println!(
                "{:<50} {:>8} {:>8} {:>8.0}",
                display_path, h.commits, churn, h.score
            );
        }

        println!();
        println!("Score = commits × √churn");
        println!("High scores indicate bug-prone files that change often.");
    }

    0
}

/// A broken reference found in documentation
#[derive(Debug)]
struct BrokenRef {
    file: String,
    line: usize,
    reference: String,
    context: String,
}

/// Check documentation references for broken links
fn cmd_check_refs(root: &Path, json: bool) -> i32 {
    use regex::Regex;

    // Open index to get known symbols
    let idx = match index::FileIndex::open_if_enabled(root) {
        Some(i) => i,
        None => {
            eprintln!("Indexing disabled or failed. Run: moss index rebuild --call-graph");
            return 1;
        }
    };

    // Get all symbol names from index
    let all_symbols = idx.all_symbol_names().unwrap_or_default();

    if all_symbols.is_empty() {
        eprintln!("No symbols indexed. Run: moss index rebuild --call-graph");
        return 1;
    }

    // Find markdown files
    let md_files: Vec<_> = walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().and_then(|s| s.to_str()) == Some("md")
                && !e
                    .path()
                    .components()
                    .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
        })
        .map(|e| e.path().to_path_buf())
        .collect();

    if md_files.is_empty() {
        if json {
            println!(
                "{{\"broken_refs\": [], \"files_checked\": 0, \"symbols_indexed\": {}}}",
                all_symbols.len()
            );
        } else {
            println!("No markdown files found to check.");
        }
        return 0;
    }

    // Regex for code references: `identifier` or `Module::method` or `Module.method`
    let code_ref_re =
        Regex::new(r"`([A-Z][a-zA-Z0-9_]*(?:[:\.][a-zA-Z_][a-zA-Z0-9_]*)*)`").unwrap();

    let mut broken_refs: Vec<BrokenRef> = Vec::new();

    for md_file in &md_files {
        let content = match std::fs::read_to_string(md_file) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let rel_path = md_file
            .strip_prefix(root)
            .unwrap_or(md_file)
            .display()
            .to_string();

        for (line_num, line) in content.lines().enumerate() {
            for cap in code_ref_re.captures_iter(line) {
                let reference = &cap[1];

                // Extract symbol name (last part after :: or .)
                let symbol_name = reference
                    .rsplit(|c| c == ':' || c == '.')
                    .next()
                    .unwrap_or(reference);

                // Skip common non-symbol patterns
                if is_common_non_symbol(symbol_name) {
                    continue;
                }

                // Check if symbol exists
                if !all_symbols.contains(symbol_name) {
                    // Also check the full reference
                    let full_name = reference.replace("::", ".").replace(".", "::");
                    if !all_symbols.contains(&full_name) && !all_symbols.contains(reference) {
                        broken_refs.push(BrokenRef {
                            file: rel_path.clone(),
                            line: line_num + 1,
                            reference: reference.to_string(),
                            context: line.trim().to_string(),
                        });
                    }
                }
            }
        }
    }

    if json {
        let output = serde_json::json!({
            "broken_refs": broken_refs.iter().map(|r| {
                serde_json::json!({
                    "file": r.file,
                    "line": r.line,
                    "reference": r.reference,
                    "context": r.context,
                })
            }).collect::<Vec<_>>(),
            "files_checked": md_files.len(),
            "symbols_indexed": all_symbols.len(),
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!("Documentation Reference Check");
        println!();
        println!("Files checked: {}", md_files.len());
        println!("Symbols indexed: {}", all_symbols.len());
        println!();

        if broken_refs.is_empty() {
            println!("No broken references found.");
        } else {
            println!("Broken references ({}):", broken_refs.len());
            println!();
            for r in &broken_refs {
                println!("  {}:{}: `{}`", r.file, r.line, r.reference);
                if r.context.len() <= 80 {
                    println!("    {}", r.context);
                }
            }
        }
    }

    if broken_refs.is_empty() {
        0
    } else {
        1
    }
}

/// Check if a string is a common non-symbol pattern (command, path, etc.)
fn is_common_non_symbol(s: &str) -> bool {
    // Skip common patterns that aren't symbols
    matches!(
        s,
        "TODO"
            | "FIXME"
            | "NOTE"
            | "HACK"
            | "XXX"
            | "BUG"
            | "OK"
            | "Err"
            | "Ok"
            | "None"
            | "Some"
            | "True"
            | "False"
            | "String"
            | "Vec"
            | "Option"
            | "Result"
            | "Box"
            | "Arc"
            | "Rc"
            | "HashMap"
            | "HashSet"
            | "BTreeMap"
            | "BTreeSet"
            | "PathBuf"
            | "Path"
            | "File"
            | "Read"
            | "Write"
            | "Debug"
            | "Clone"
            | "Copy"
            | "Default"
            | "Send"
            | "Sync"
            | "Serialize"
            | "Deserialize"
    ) || s.len() < 2
        || s.chars().all(|c| c.is_uppercase() || c == '_') // ALL_CAPS constants
}

/// A doc file with stale code coverage
#[derive(Debug)]
struct StaleDoc {
    doc_path: String,
    doc_modified: u64,
    stale_covers: Vec<StaleCover>,
}

/// A stale coverage declaration
#[derive(Debug)]
struct StaleCover {
    pattern: String,
    code_modified: u64,
    matching_files: Vec<String>,
}

/// Find docs with stale code coverage
fn cmd_stale_docs(root: &Path, json: bool) -> i32 {
    use regex::Regex;

    // Find markdown files with <!-- covers: ... --> declarations
    let covers_re = Regex::new(r"<!--\s*covers:\s*(.+?)\s*-->").unwrap();

    let md_files: Vec<_> = walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().and_then(|s| s.to_str()) == Some("md")
                && !e
                    .path()
                    .components()
                    .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
        })
        .map(|e| e.path().to_path_buf())
        .collect();

    if md_files.is_empty() {
        if json {
            println!("{{\"stale_docs\": [], \"files_checked\": 0}}");
        } else {
            println!("No markdown files found.");
        }
        return 0;
    }

    let mut stale_docs: Vec<StaleDoc> = Vec::new();
    let mut files_with_covers = 0;

    for md_file in &md_files {
        let content = match std::fs::read_to_string(md_file) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Find all covers declarations
        let covers: Vec<String> = covers_re
            .captures_iter(&content)
            .map(|cap| cap[1].to_string())
            .collect();

        if covers.is_empty() {
            continue;
        }

        files_with_covers += 1;

        let rel_path = md_file
            .strip_prefix(root)
            .unwrap_or(md_file)
            .display()
            .to_string();

        // Get doc modification time
        let doc_modified = std::fs::metadata(md_file)
            .and_then(|m| m.modified())
            .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs())
            .unwrap_or(0);

        let mut stale_covers: Vec<StaleCover> = Vec::new();

        for cover_pattern in covers {
            // Parse comma-separated patterns
            for pattern in cover_pattern.split(',').map(|s| s.trim()) {
                if pattern.is_empty() {
                    continue;
                }

                // Find matching files using glob
                let matching = find_covered_files(root, pattern);

                if matching.is_empty() {
                    continue;
                }

                // Check if any matching file was modified after the doc
                let code_modified = matching
                    .iter()
                    .filter_map(|f| {
                        std::fs::metadata(root.join(f))
                            .and_then(|m| m.modified())
                            .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs())
                            .ok()
                    })
                    .max()
                    .unwrap_or(0);

                if code_modified > doc_modified {
                    stale_covers.push(StaleCover {
                        pattern: pattern.to_string(),
                        code_modified,
                        matching_files: matching,
                    });
                }
            }
        }

        if !stale_covers.is_empty() {
            stale_docs.push(StaleDoc {
                doc_path: rel_path,
                doc_modified,
                stale_covers,
            });
        }
    }

    if json {
        let output = serde_json::json!({
            "stale_docs": stale_docs.iter().map(|d| {
                serde_json::json!({
                    "doc": d.doc_path,
                    "doc_modified": d.doc_modified,
                    "stale_covers": d.stale_covers.iter().map(|c| {
                        serde_json::json!({
                            "pattern": c.pattern,
                            "code_modified": c.code_modified,
                            "files": c.matching_files,
                        })
                    }).collect::<Vec<_>>(),
                })
            }).collect::<Vec<_>>(),
            "files_checked": md_files.len(),
            "files_with_covers": files_with_covers,
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!("Stale Documentation Check");
        println!();
        println!("Files checked: {}", md_files.len());
        println!("Files with covers: {}", files_with_covers);
        println!();

        if stale_docs.is_empty() {
            println!("No stale docs found. All covered code is older than docs.");
        } else {
            println!("Stale docs ({}):", stale_docs.len());
            println!();
            for doc in &stale_docs {
                println!("  {}", doc.doc_path);
                for cover in &doc.stale_covers {
                    let days_stale = (cover.code_modified - doc.doc_modified) / 86400;
                    println!(
                        "    {} ({} files, ~{} days stale)",
                        cover.pattern,
                        cover.matching_files.len(),
                        days_stale
                    );
                }
            }
        }
    }

    if stale_docs.is_empty() {
        0
    } else {
        1
    }
}

/// Find files matching a cover pattern (glob or path prefix)
fn find_covered_files(root: &Path, pattern: &str) -> Vec<String> {
    // Check if it's a glob pattern
    if pattern.contains('*') {
        // Use glob matching
        let full_pattern = root.join(pattern);
        glob::glob(full_pattern.to_str().unwrap_or(""))
            .ok()
            .map(|paths| {
                paths
                    .filter_map(|p| p.ok())
                    .filter(|p| p.is_file())
                    .filter_map(|p| p.strip_prefix(root).ok().map(|r| r.display().to_string()))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        // Treat as exact path or prefix
        let target = root.join(pattern);
        if target.is_file() {
            vec![pattern.to_string()]
        } else if target.is_dir() {
            // Find all files in directory
            walkdir::WalkDir::new(&target)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
                .filter_map(|e| {
                    e.path()
                        .strip_prefix(root)
                        .ok()
                        .map(|r| r.display().to_string())
                })
                .collect()
        } else {
            vec![]
        }
    }
}

/// A missing example reference
#[derive(Debug)]
struct MissingExample {
    doc_file: String,
    line: usize,
    reference: String, // path#name
}

/// Check that all example references have matching markers
fn cmd_check_examples(root: &Path, json: bool) -> i32 {
    use regex::Regex;
    use std::collections::HashSet;

    // Find all example markers in source files: // [example: name] ... // [/example]
    let marker_start_re = Regex::new(r"//\s*\[example:\s*([^\]]+)\]").unwrap();

    // Find all example references in docs: {{example: path#name}}
    let ref_re = Regex::new(r"\{\{example:\s*([^}]+)\}\}").unwrap();

    // Collect all defined examples: (file, name)
    let mut defined_examples: HashSet<String> = HashSet::new();

    // Walk source files
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let path = e.path();
            path.is_file()
                && !path
                    .components()
                    .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
        })
    {
        let path = entry.path();
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");

        // Only check source files (where we'd have // [example:] markers)
        if !matches!(
            ext,
            "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "go" | "java" | "c" | "cpp" | "rb"
        ) {
            continue;
        }

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let rel_path = path
            .strip_prefix(root)
            .unwrap_or(path)
            .display()
            .to_string();

        for cap in marker_start_re.captures_iter(&content) {
            let name = cap[1].trim();
            // Key: path#name
            let key = format!("{}#{}", rel_path, name);
            defined_examples.insert(key);
        }
    }

    // Find all references in markdown files
    let mut missing: Vec<MissingExample> = Vec::new();
    let mut refs_found = 0;

    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().and_then(|s| s.to_str()) == Some("md")
                && !e
                    .path()
                    .components()
                    .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
        })
    {
        let path = entry.path();
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let rel_path = path
            .strip_prefix(root)
            .unwrap_or(path)
            .display()
            .to_string();

        let mut in_code_block = false;
        for (line_num, line) in content.lines().enumerate() {
            // Track fenced code blocks
            if line.trim().starts_with("```") {
                in_code_block = !in_code_block;
                continue;
            }
            if in_code_block {
                continue;
            }

            for cap in ref_re.captures_iter(line) {
                // Skip if match is inside backticks (inline code)
                let match_start = cap.get(0).unwrap().start();
                let match_end = cap.get(0).unwrap().end();
                let before = &line[..match_start];
                let after = &line[match_end..];

                // Count backticks before match - odd count means we're inside inline code
                if before.chars().filter(|&c| c == '`').count() % 2 == 1 && after.contains('`') {
                    continue;
                }

                refs_found += 1;
                let reference = cap[1].trim();

                // Reference should be path#name
                if !defined_examples.contains(reference) {
                    missing.push(MissingExample {
                        doc_file: rel_path.clone(),
                        line: line_num + 1,
                        reference: reference.to_string(),
                    });
                }
            }
        }
    }

    if json {
        let output = serde_json::json!({
            "defined_examples": defined_examples.len(),
            "references_found": refs_found,
            "missing": missing.iter().map(|m| {
                serde_json::json!({
                    "doc": m.doc_file,
                    "line": m.line,
                    "reference": m.reference,
                })
            }).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!("Example Reference Check");
        println!();
        println!("Defined examples: {}", defined_examples.len());
        println!("References found: {}", refs_found);
        println!();

        if missing.is_empty() {
            println!("All example references are valid.");
        } else {
            println!("Missing examples ({}):", missing.len());
            println!();
            for m in &missing {
                println!("  {}:{}: {{{{{}}}}}", m.doc_file, m.line, m.reference);
            }
        }
    }

    if missing.is_empty() {
        0
    } else {
        1
    }
}

// ============================================================================
// Clone Detection
// ============================================================================

use crate::extract::Extractor;
use crate::parsers::Parsers;
use moss_languages::support_for_path;
use std::hash::{Hash, Hasher};

/// A detected code clone group
#[derive(Debug)]
struct CloneGroup {
    hash: u64,
    locations: Vec<CloneLocation>,
    line_count: usize,
}

/// Location of a clone instance
#[derive(Debug)]
struct CloneLocation {
    file: String,
    symbol: String,
    start_line: usize,
    end_line: usize,
}

/// Load allowed clone locations from .moss/clone-allow file
fn load_clone_allowlist(root: &Path) -> std::collections::HashSet<String> {
    let path = root.join(".moss/clone-allow");
    let mut allowed = std::collections::HashSet::new();
    if let Ok(content) = std::fs::read_to_string(&path) {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            allowed.insert(line.to_string());
        }
    }
    allowed
}

/// Detect code clones across the codebase
fn cmd_clones(
    root: &Path,
    elide_identifiers: bool,
    elide_literals: bool,
    show_source: bool,
    min_lines: usize,
    json: bool,
) -> i32 {
    let extractor = Extractor::new();
    let parsers = Parsers::new();
    let allowlist = load_clone_allowlist(root);

    // Collect function hashes: hash -> [(file, symbol, start, end)]
    let mut hash_groups: std::collections::HashMap<u64, Vec<CloneLocation>> =
        std::collections::HashMap::new();
    let mut files_scanned = 0;
    let mut functions_hashed = 0;

    // Walk source files, respecting .gitignore
    let walker = ignore::WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .build();

    for entry in walker.filter_map(|e| e.ok()).filter(|e| {
        let path = e.path();
        path.is_file() && is_source_file(path)
    }) {
        let path = entry.path();
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let support = match support_for_path(path) {
            Some(s) => s,
            None => continue,
        };

        // Parse the file
        let tree = match parsers.parse_with_grammar(support.grammar_name(), &content) {
            Some(t) => t,
            None => continue,
        };

        files_scanned += 1;

        // Extract symbols to find functions/methods
        let result = extractor.extract(path, &content);

        // Find and hash each function/method
        for sym in result.symbols.iter().flat_map(|s| flatten_symbols(s)) {
            let kind = sym.kind.as_str();
            if kind != "function" && kind != "method" {
                continue;
            }

            // Find the function node
            if let Some(node) = find_function_node(&tree, sym.start_line) {
                let line_count = sym.end_line.saturating_sub(sym.start_line) + 1;
                if line_count < min_lines {
                    continue;
                }

                let hash = compute_clone_hash(
                    &node,
                    content.as_bytes(),
                    elide_identifiers,
                    elide_literals,
                );
                functions_hashed += 1;

                let rel_path = path
                    .strip_prefix(root)
                    .unwrap_or(path)
                    .display()
                    .to_string();

                hash_groups.entry(hash).or_default().push(CloneLocation {
                    file: rel_path,
                    symbol: sym.name.clone(),
                    start_line: sym.start_line,
                    end_line: sym.end_line,
                });
            }
        }
    }

    // Filter to groups with 2+ instances (actual clones)
    // Skip groups where ALL locations are in the allowlist
    let mut clone_groups: Vec<CloneGroup> = hash_groups
        .into_iter()
        .filter(|(_, locs)| locs.len() >= 2)
        .filter(|(_, locs)| {
            // Keep if any location is NOT allowed
            locs.iter()
                .any(|loc| !allowlist.contains(&format!("{}:{}", loc.file, loc.symbol)))
        })
        .map(|(hash, locations)| {
            let line_count = locations
                .first()
                .map(|l| l.end_line - l.start_line + 1)
                .unwrap_or(0);
            CloneGroup {
                hash,
                locations,
                line_count,
            }
        })
        .collect();

    // Sort by line count (larger clones first), then by number of instances
    clone_groups.sort_by(|a, b| {
        b.line_count
            .cmp(&a.line_count)
            .then_with(|| b.locations.len().cmp(&a.locations.len()))
    });

    let total_clones: usize = clone_groups.iter().map(|g| g.locations.len()).sum();
    let clone_lines: usize = clone_groups
        .iter()
        .map(|g| g.line_count * g.locations.len())
        .sum();

    if json {
        let output = serde_json::json!({
            "files_scanned": files_scanned,
            "functions_hashed": functions_hashed,
            "clone_groups": clone_groups.len(),
            "total_clones": total_clones,
            "clone_lines": clone_lines,
            "elide_identifiers": elide_identifiers,
            "elide_literals": elide_literals,
            "groups": clone_groups.iter().map(|g| {
                serde_json::json!({
                    "hash": format!("{:016x}", g.hash),
                    "line_count": g.line_count,
                    "instances": g.locations.len(),
                    "locations": g.locations.iter().map(|l| {
                        serde_json::json!({
                            "file": l.file,
                            "symbol": l.symbol,
                            "start_line": l.start_line,
                            "end_line": l.end_line,
                        })
                    }).collect::<Vec<_>>(),
                })
            }).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!("Clone Detection");
        println!();
        println!("Files scanned: {}", files_scanned);
        println!("Functions hashed: {}", functions_hashed);
        println!("Clone groups: {}", clone_groups.len());
        println!("Total clones: {}", total_clones);
        println!("Duplicated lines: ~{}", clone_lines);
        println!();

        if clone_groups.is_empty() {
            println!("No code clones detected.");
        } else {
            println!("Clone Groups (sorted by size):");
            println!();

            for (i, group) in clone_groups.iter().take(20).enumerate() {
                println!(
                    "{}. {} lines, {} instances:",
                    i + 1,
                    group.line_count,
                    group.locations.len()
                );

                for loc in &group.locations {
                    println!(
                        "   {}:{}-{} ({})",
                        loc.file, loc.start_line, loc.end_line, loc.symbol
                    );

                    // Show source code if requested
                    if show_source {
                        let file_path = root.join(&loc.file);
                        if let Ok(content) = std::fs::read_to_string(&file_path) {
                            let lines: Vec<&str> = content.lines().collect();
                            let start = loc.start_line.saturating_sub(1);
                            let end = loc.end_line.min(lines.len());
                            for (j, line) in lines[start..end].iter().enumerate() {
                                println!("        {:4} │ {}", start + j + 1, line);
                            }
                            println!();
                        }
                    }
                }
                println!();
            }

            if clone_groups.len() > 20 {
                println!("... and {} more groups", clone_groups.len() - 20);
            }
        }
    }

    0
}

/// Flatten nested symbols into a flat list
fn flatten_symbols(sym: &moss_languages::Symbol) -> Vec<&moss_languages::Symbol> {
    let mut result = vec![sym];
    for child in &sym.children {
        result.extend(flatten_symbols(child));
    }
    result
}

/// Find a function node at a given line
fn find_function_node(
    tree: &tree_sitter::Tree,
    target_line: usize,
) -> Option<tree_sitter::Node<'_>> {
    let root = tree.root_node();
    let mut cursor = root.walk();
    find_node_at_line_recursive(&mut cursor, target_line)
}

fn find_node_at_line_recursive<'a>(
    cursor: &mut tree_sitter::TreeCursor<'a>,
    target_line: usize,
) -> Option<tree_sitter::Node<'a>> {
    loop {
        let node = cursor.node();
        let start = node.start_position().row + 1;

        if start == target_line {
            let kind = node.kind();
            if kind.contains("function")
                || kind.contains("method")
                || kind == "function_definition"
                || kind == "method_definition"
                || kind == "function_item"
                || kind == "function_declaration"
                || kind == "arrow_function"
                || kind == "generator_function"
            {
                return Some(node);
            }
        }

        if cursor.goto_first_child() {
            if let Some(found) = find_node_at_line_recursive(cursor, target_line) {
                return Some(found);
            }
            cursor.goto_parent();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
    None
}

/// Compute a normalized AST hash for clone detection.
fn compute_clone_hash(
    node: &tree_sitter::Node,
    content: &[u8],
    elide_identifiers: bool,
    elide_literals: bool,
) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    hash_node_recursive(
        node,
        content,
        &mut hasher,
        elide_identifiers,
        elide_literals,
    );
    hasher.finish()
}

/// Recursively hash a node and its children.
fn hash_node_recursive(
    node: &tree_sitter::Node,
    content: &[u8],
    hasher: &mut impl Hasher,
    elide_identifiers: bool,
    elide_literals: bool,
) {
    let kind = node.kind();

    // Hash the node kind (structure)
    kind.hash(hasher);

    // For leaf nodes, decide whether to hash content
    if node.child_count() == 0 {
        let should_hash = if is_identifier_kind(kind) {
            !elide_identifiers
        } else if is_literal_kind(kind) {
            !elide_literals
        } else {
            // Operators, keywords - their kind is sufficient
            false
        };

        if should_hash {
            let text = &content[node.start_byte()..node.end_byte()];
            text.hash(hasher);
        }
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        hash_node_recursive(&child, content, hasher, elide_identifiers, elide_literals);
    }
}

/// Check if a node kind represents an identifier.
fn is_identifier_kind(kind: &str) -> bool {
    kind == "identifier"
        || kind == "field_identifier"
        || kind == "type_identifier"
        || kind == "property_identifier"
        || kind.ends_with("_identifier")
}

/// Check if a node kind represents a literal value.
fn is_literal_kind(kind: &str) -> bool {
    kind.contains("string")
        || kind.contains("integer")
        || kind.contains("float")
        || kind.contains("number")
        || kind.contains("boolean")
        || kind == "true"
        || kind == "false"
        || kind == "nil"
        || kind == "null"
        || kind == "none"
}
