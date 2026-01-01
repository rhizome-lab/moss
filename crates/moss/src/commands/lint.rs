//! Lint command - run linters, formatters, and type checkers.

use crate::output::{OutputFormat, OutputFormatter};
use moss_tools::{SarifReport, ToolCategory, ToolRegistry, registry_with_custom};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use nu_ansi_term::Color::{Blue, Red, Yellow};
use nu_ansi_term::Style;
use rayon::prelude::*;
use serde::Serialize;
use std::fmt::Write;
use std::path::Path;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

/// Tool info for lint list output
#[derive(Debug, Serialize)]
pub struct ToolListItem {
    pub name: String,
    pub category: String,
    pub available: bool,
    pub version: Option<String>,
    pub extensions: String,
    pub website: String,
}

/// Result of lint list command
#[derive(Debug, Serialize)]
pub struct LintListResult {
    pub tools: Vec<ToolListItem>,
}

impl OutputFormatter for LintListResult {
    fn format_text(&self) -> String {
        let mut out = String::from("Detected tools:\n\n");
        for tool in &self.tools {
            let status = if tool.available { "✓" } else { "✗" };
            let ver = tool.version.as_deref().unwrap_or("not installed");
            writeln!(
                out,
                "  {} {} ({}) - {}",
                status, tool.name, tool.category, ver
            )
            .unwrap();
            writeln!(out, "    Extensions: {}", tool.extensions).unwrap();
            writeln!(out, "    Website: {}", tool.website).unwrap();
            writeln!(out).unwrap();
        }
        out
    }
}

/// Run linting tools on the codebase.
pub fn cmd_lint_run(
    target: Option<&str>,
    root: Option<&Path>,
    fix: bool,
    tools: Option<&str>,
    category: Option<&str>,
    sarif: bool,
    format: crate::output::OutputFormat,
) -> i32 {
    let root = root.unwrap_or_else(|| Path::new("."));
    let use_colors = format.use_colors();
    let json = format.is_json();
    // Load built-in tools + custom tools from .moss/tools.toml
    let registry = registry_with_custom(root);

    // Parse category filter
    let category_filter: Option<ToolCategory> = category.and_then(|c| match c {
        "lint" | "linter" => Some(ToolCategory::Linter),
        "fmt" | "format" | "formatter" => Some(ToolCategory::Formatter),
        "type" | "typecheck" | "type-checker" => Some(ToolCategory::TypeChecker),
        _ => None,
    });

    // Get tools to run
    let tools_to_run: Vec<&dyn moss_tools::Tool> = if let Some(tool_names) = tools {
        // Run specific tools by name
        let names: Vec<&str> = tool_names.split(',').map(|s| s.trim()).collect();
        registry
            .tools()
            .iter()
            .filter(|t| names.contains(&t.info().name))
            .map(|t| t.as_ref())
            .collect()
    } else {
        // Auto-detect relevant tools
        let detected = registry.detect(root);
        detected
            .into_iter()
            .filter(|(t, _)| {
                if let Some(cat) = category_filter {
                    t.info().category == cat
                } else {
                    true
                }
            })
            .map(|(t, _)| t)
            .collect()
    };

    if tools_to_run.is_empty() {
        if json {
            println!("{{\"tools\": [], \"diagnostics\": []}}");
        } else {
            eprintln!("No relevant tools found for this project.");
            eprintln!("Use 'moss lint list' to see available tools.");
        }
        return 0;
    }

    // Prepare paths
    let paths: Vec<&Path> = target.map(|t| vec![Path::new(t)]).unwrap_or_default();

    // Run tools
    let mut all_results = Vec::new();
    let mut had_errors = false;

    for tool in &tools_to_run {
        let info = tool.info();

        if !tool.is_available() {
            if !json {
                eprintln!("{}: not installed", info.name);
            }
            continue;
        }

        if !json {
            let action = if fix && tool.can_fix() {
                "fixing"
            } else {
                "checking"
            };
            eprintln!("{}: {}...", info.name, action);
        }

        let result = if fix && tool.can_fix() {
            tool.fix(&paths.iter().copied().collect::<Vec<_>>(), root)
        } else {
            tool.run(&paths.iter().copied().collect::<Vec<_>>(), root)
        };

        match result {
            Ok(result) => {
                if !result.success {
                    had_errors = true;
                    if let Some(err) = &result.error {
                        if !json {
                            eprintln!("{}: {}", info.name, err);
                        }
                    }
                } else if result.error_count() > 0 {
                    had_errors = true;
                }
                all_results.push(result);
            }
            Err(e) => {
                had_errors = true;
                if !json {
                    eprintln!("{}: {}", info.name, e);
                }
            }
        }
    }

    // Output results
    if sarif {
        let diagnostics = ToolRegistry::collect_diagnostics(&all_results);
        let report = SarifReport::from_diagnostics(&diagnostics);
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else if json {
        let diagnostics = ToolRegistry::collect_diagnostics(&all_results);
        let output = serde_json::json!({
            "tools": tools_to_run.iter().map(|t| {
                let info = t.info();
                serde_json::json!({
                    "name": info.name,
                    "category": info.category.as_str(),
                    "available": t.is_available(),
                    "version": t.version(),
                })
            }).collect::<Vec<_>>(),
            "results": all_results.iter().map(|r| {
                serde_json::json!({
                    "tool": r.tool,
                    "success": r.success,
                    "error_count": r.error_count(),
                    "warning_count": r.warning_count(),
                    "error": r.error,
                })
            }).collect::<Vec<_>>(),
            "diagnostics": diagnostics,
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        // Print diagnostics
        for result in &all_results {
            for diag in &result.diagnostics {
                let severity_str = match diag.severity {
                    moss_tools::DiagnosticSeverity::Error => "error",
                    moss_tools::DiagnosticSeverity::Warning => "warning",
                    moss_tools::DiagnosticSeverity::Info => "info",
                    moss_tools::DiagnosticSeverity::Hint => "hint",
                };

                let severity_display = if use_colors {
                    match diag.severity {
                        moss_tools::DiagnosticSeverity::Error => {
                            Red.bold().paint(severity_str).to_string()
                        }
                        moss_tools::DiagnosticSeverity::Warning => {
                            Yellow.paint(severity_str).to_string()
                        }
                        moss_tools::DiagnosticSeverity::Info => {
                            Blue.paint(severity_str).to_string()
                        }
                        moss_tools::DiagnosticSeverity::Hint => {
                            Style::new().dimmed().paint(severity_str).to_string()
                        }
                    }
                } else {
                    severity_str.to_string()
                };

                println!(
                    "{}:{}:{}: {} [{}] {}",
                    diag.location.file.display(),
                    diag.location.line,
                    diag.location.column,
                    severity_display,
                    diag.rule_id,
                    diag.message
                );

                if let Some(url) = &diag.help_url {
                    println!("  help: {}", url);
                }
            }
        }

        // Summary
        let total_errors: usize = all_results.iter().map(|r| r.error_count()).sum();
        let total_warnings: usize = all_results.iter().map(|r| r.warning_count()).sum();

        if total_errors > 0 || total_warnings > 0 {
            eprintln!();
            eprintln!(
                "Found {} error(s) and {} warning(s)",
                total_errors, total_warnings
            );
        }
    }

    if had_errors { 1 } else { 0 }
}

/// List available linting tools.
pub fn cmd_lint_list(root: Option<&Path>, format: &OutputFormat) -> i32 {
    let root = root.unwrap_or_else(|| Path::new("."));
    let registry = registry_with_custom(root);

    // Only check tools relevant to this codebase (detected via config files, lockfiles, etc.)
    // Use version() to infer availability - avoids duplicate process spawns
    // Parallelize version checks since each spawns a subprocess
    let detected = registry.detect(root);
    let tools: Vec<ToolListItem> = detected
        .par_iter()
        .map(|(t, _)| {
            let info = t.info();
            let version = t.version();
            ToolListItem {
                name: info.name.to_string(),
                category: info.category.as_str().to_string(),
                available: version.is_some(),
                version,
                extensions: info.extensions.join(", "),
                website: info.website.to_string(),
            }
        })
        .collect();

    let result = LintListResult { tools };
    result.print(format);

    0
}

/// Watch mode for linters - re-run on file changes.
pub fn cmd_lint_watch(
    target: Option<&str>,
    root: Option<&Path>,
    fix: bool,
    tools: Option<&str>,
    category: Option<&str>,
    json: bool,
) -> i32 {
    let root = root.unwrap_or_else(|| Path::new("."));

    // Initial run
    eprintln!("Running initial lint check...");
    let _ = run_lint_once(target, root, fix, tools, category, json);
    eprintln!();
    eprintln!("Watching for changes... (Ctrl+C to stop)");

    // Set up file watcher
    let (tx, rx) = channel();
    let mut watcher = match RecommendedWatcher::new(tx, Config::default()) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("Failed to create file watcher: {}", e);
            return 1;
        }
    };

    if let Err(e) = watcher.watch(root, RecursiveMode::Recursive) {
        eprintln!("Failed to watch directory: {}", e);
        return 1;
    }

    // Debounce file changes
    let mut last_run = Instant::now();
    let debounce = Duration::from_millis(500);

    // Build list of extensions we care about
    let registry = registry_with_custom(root);
    let watch_extensions: std::collections::HashSet<&str> = registry
        .tools()
        .iter()
        .flat_map(|t| t.info().extensions.iter().copied())
        .collect();

    for res in rx {
        if let Ok(event) = res {
            // Skip hidden files and directories
            let dominated_by_hidden = event.paths.iter().all(|p| {
                p.components()
                    .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
            });
            if dominated_by_hidden {
                continue;
            }

            // Only trigger on files with relevant extensions
            let has_relevant_file = event.paths.iter().any(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| watch_extensions.contains(e))
                    .unwrap_or(false)
            });
            if !has_relevant_file {
                continue;
            }

            // Debounce: only run if enough time has passed
            if last_run.elapsed() >= debounce {
                eprintln!();
                eprintln!("File changed, re-running lint...");
                let _ = run_lint_once(target, root, fix, tools, category, json);
                last_run = Instant::now();
            }
        }
    }

    0
}

/// Run lint once (used by both regular and watch modes).
fn run_lint_once(
    target: Option<&str>,
    root: &Path,
    fix: bool,
    tools: Option<&str>,
    category: Option<&str>,
    json: bool,
) -> i32 {
    let registry = registry_with_custom(root);

    // Parse category filter
    let category_filter: Option<ToolCategory> = category.and_then(|c| match c {
        "lint" | "linter" => Some(ToolCategory::Linter),
        "fmt" | "format" | "formatter" => Some(ToolCategory::Formatter),
        "type" | "typecheck" | "type-checker" => Some(ToolCategory::TypeChecker),
        _ => None,
    });

    // Get tools to run
    let tools_to_run: Vec<&dyn moss_tools::Tool> = if let Some(tool_names) = tools {
        let names: Vec<&str> = tool_names.split(',').map(|s| s.trim()).collect();
        registry
            .tools()
            .iter()
            .filter(|t| names.contains(&t.info().name))
            .map(|t| t.as_ref())
            .collect()
    } else {
        let detected = registry.detect(root);
        detected
            .into_iter()
            .filter(|(t, _)| {
                if let Some(cat) = category_filter {
                    t.info().category == cat
                } else {
                    true
                }
            })
            .map(|(t, _)| t)
            .collect()
    };

    if tools_to_run.is_empty() {
        if json {
            println!("{{\"tools\": [], \"diagnostics\": []}}");
        } else {
            eprintln!("No relevant tools found for this project.");
        }
        return 0;
    }

    let paths: Vec<&Path> = target.map(|t| vec![Path::new(t)]).unwrap_or_default();
    let mut all_results = Vec::new();
    let mut had_errors = false;

    for tool in &tools_to_run {
        let info = tool.info();

        if !tool.is_available() {
            if !json {
                eprintln!("{}: not installed", info.name);
            }
            continue;
        }

        if !json {
            let action = if fix && tool.can_fix() {
                "fixing"
            } else {
                "checking"
            };
            eprintln!("{}: {}...", info.name, action);
        }

        let result = if fix && tool.can_fix() {
            tool.fix(&paths.iter().copied().collect::<Vec<_>>(), root)
        } else {
            tool.run(&paths.iter().copied().collect::<Vec<_>>(), root)
        };

        match result {
            Ok(result) => {
                if !result.success {
                    had_errors = true;
                    if let Some(err) = &result.error {
                        if !json {
                            eprintln!("{}: {}", info.name, err);
                        }
                    }
                } else if result.error_count() > 0 {
                    had_errors = true;
                }
                all_results.push(result);
            }
            Err(e) => {
                had_errors = true;
                if !json {
                    eprintln!("{}: {}", info.name, e);
                }
            }
        }
    }

    // Output results
    if json {
        let diagnostics = ToolRegistry::collect_diagnostics(&all_results);
        let output = serde_json::json!({
            "tools": tools_to_run.iter().map(|t| {
                let info = t.info();
                serde_json::json!({
                    "name": info.name,
                    "category": info.category.as_str(),
                    "available": t.is_available(),
                    "version": t.version(),
                })
            }).collect::<Vec<_>>(),
            "diagnostics": diagnostics,
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
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

                if let Some(url) = &diag.help_url {
                    println!("  help: {}", url);
                }
            }
        }

        let total_errors: usize = all_results.iter().map(|r| r.error_count()).sum();
        let total_warnings: usize = all_results.iter().map(|r| r.warning_count()).sum();

        if total_errors > 0 || total_warnings > 0 {
            eprintln!();
            eprintln!(
                "Found {} error(s) and {} warning(s)",
                total_errors, total_warnings
            );
        }
    }

    if had_errors { 1 } else { 0 }
}
