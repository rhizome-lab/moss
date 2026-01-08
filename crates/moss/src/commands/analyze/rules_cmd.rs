//! CLI command handler for syntax rules.
//!
//! Uses moss_rules for core functionality, handles CLI output.

use crate::parsers::grammar_loader;
use moss_rules::{
    DebugFlags, Finding, Rule, RulesConfig, Severity, apply_fixes, load_all_rules, run_rules,
};
use std::path::Path;

/// Run the rules command.
pub fn cmd_rules(
    root: &Path,
    filter_rule: Option<&str>,
    list_only: bool,
    fix: bool,
    json: bool,
    sarif: bool,
    config: &RulesConfig,
    debug: &DebugFlags,
) -> i32 {
    // Load rules from all sources (builtins + user global + project)
    let rules = load_all_rules(root, config);

    if rules.is_empty() {
        if !list_only {
            eprintln!("No rules found.");
            eprintln!("Create .scm files with TOML frontmatter in .moss/rules/");
        }
        return 0;
    }

    if list_only {
        if json {
            let list: Vec<_> = rules
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "id": r.id,
                        "severity": r.severity.to_string(),
                        "message": r.message,
                        "builtin": r.builtin,
                        "source": if r.builtin { "builtin".to_string() } else { r.source_path.to_string_lossy().to_string() },
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&list).unwrap());
        } else {
            let builtin_count = rules.iter().filter(|r| r.builtin).count();
            let project_count = rules.len() - builtin_count;
            println!(
                "Available rules ({} builtin, {} project):",
                builtin_count, project_count
            );
            println!();
            for rule in &rules {
                let source = if rule.builtin { "builtin" } else { "project" };
                println!(
                    "  {} ({}, {}) - {}",
                    rule.id, rule.severity, source, rule.message
                );
            }
        }
        return 0;
    }

    // Run rules with the global grammar loader
    let loader = grammar_loader();
    let findings = run_rules(&rules, root, &loader, filter_rule, debug);

    // Apply fixes if requested
    if fix {
        let fixable: Vec<_> = findings.iter().filter(|f| f.fix.is_some()).collect();
        if fixable.is_empty() {
            eprintln!("No auto-fixable issues found.");
        } else {
            match apply_fixes(&findings) {
                Ok(files_modified) => {
                    if json {
                        println!(
                            "{}",
                            serde_json::json!({
                                "fixed": fixable.len(),
                                "files_modified": files_modified
                            })
                        );
                    } else {
                        println!(
                            "Fixed {} issue(s) in {} file(s).",
                            fixable.len(),
                            files_modified
                        );
                    }
                }
                Err(e) => {
                    eprintln!("Error applying fixes: {}", e);
                    return 1;
                }
            }
        }
        return 0;
    }

    if sarif {
        print_sarif(&rules, &findings, root);
    } else if json {
        let output: Vec<_> = findings
            .iter()
            .map(|f| {
                serde_json::json!({
                    "rule": f.rule_id,
                    "file": f.file.to_string_lossy(),
                    "start": {
                        "line": f.start_line,
                        "column": f.start_col
                    },
                    "end": {
                        "line": f.end_line,
                        "column": f.end_col
                    },
                    "severity": f.severity.to_string(),
                    "message": f.message,
                    "text": f.matched_text
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        if findings.is_empty() {
            println!("No issues found.");
            return 0;
        }

        println!("{} issues found:", findings.len());
        println!();

        for finding in &findings {
            let rel_path = finding.file.strip_prefix(root).unwrap_or(&finding.file);

            println!(
                "  {}:{}:{}: {} [{}]",
                rel_path.display(),
                finding.start_line,
                finding.start_col,
                finding.message,
                finding.rule_id
            );
            if !finding.matched_text.is_empty() {
                println!("    {}", finding.matched_text);
            }
        }
    }

    if findings.iter().any(|f| f.severity == Severity::Error) {
        1
    } else {
        0
    }
}

/// Output findings in SARIF 2.1.0 format for IDE integration.
fn print_sarif(rules: &[Rule], findings: &[Finding], root: &Path) {
    // Build rules array for the tool driver
    let sarif_rules: Vec<_> = rules
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "shortDescription": { "text": r.message },
                "defaultConfiguration": {
                    "level": severity_to_sarif_level(r.severity)
                }
            })
        })
        .collect();

    // Build results array
    let results: Vec<_> = findings
        .iter()
        .map(|f| {
            let uri = f
                .file
                .canonicalize()
                .ok()
                .map(|p| format!("file://{}", p.display()))
                .unwrap_or_else(|| {
                    let rel = f.file.strip_prefix(root).unwrap_or(&f.file);
                    rel.display().to_string()
                });

            serde_json::json!({
                "ruleId": f.rule_id,
                "level": severity_to_sarif_level(f.severity),
                "message": { "text": f.message },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": { "uri": uri },
                        "region": {
                            "startLine": f.start_line,
                            "startColumn": f.start_col,
                            "endLine": f.end_line,
                            "endColumn": f.end_col
                        }
                    }
                }]
            })
        })
        .collect();

    let sarif = serde_json::json!({
        "version": "2.1.0",
        "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "moss",
                    "version": env!("CARGO_PKG_VERSION"),
                    "informationUri": "https://github.com/pterror/moss",
                    "rules": sarif_rules
                }
            },
            "results": results
        }]
    });

    println!("{}", serde_json::to_string_pretty(&sarif).unwrap());
}

/// Convert moss severity to SARIF level.
fn severity_to_sarif_level(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "note",
    }
}
