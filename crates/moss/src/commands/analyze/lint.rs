//! Linter analysis - run configured linting tools

use moss_tools::registry_with_custom;
use std::path::Path;

/// Run linter analysis on the codebase
pub fn cmd_lint_analyze(root: &Path, target: Option<&str>, json: bool) -> i32 {
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

    if total_errors > 0 { 1 } else { 0 }
}
