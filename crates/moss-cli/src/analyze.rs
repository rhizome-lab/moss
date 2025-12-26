//! Unified analysis command combining health, complexity, and security.
//!
//! `moss analyze [path]` with flags:
//! - `--health` - codebase health metrics
//! - `--complexity` - cyclomatic complexity analysis
//! - `--security` - security vulnerability scanning
//! - (no flags) - run all analyses

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use crate::complexity::{ComplexityAnalyzer, ComplexityReport};
use crate::filter::Filter;
use crate::health::{analyze_health, HealthReport};
use crate::path_resolve;

/// Severity levels for security findings
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Low => "low",
            Severity::Medium => "medium",
            Severity::High => "high",
            Severity::Critical => "critical",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "critical" | "error" => Severity::Critical,
            "high" | "warning" => Severity::High,
            "medium" => Severity::Medium,
            _ => Severity::Low,
        }
    }
}

/// A security finding from analysis tools
#[derive(Debug, Clone)]
pub struct SecurityFinding {
    pub file: String,
    pub line: usize,
    pub severity: Severity,
    pub rule_id: String,
    pub message: String,
    pub tool: String,
}

/// Security analysis results
#[derive(Debug, Default)]
pub struct SecurityReport {
    pub findings: Vec<SecurityFinding>,
    pub tools_run: Vec<String>,
    pub tools_skipped: Vec<String>,
}

impl SecurityReport {
    pub fn count_by_severity(&self) -> HashMap<&'static str, usize> {
        let mut counts = HashMap::new();
        counts.insert("critical", 0);
        counts.insert("high", 0);
        counts.insert("medium", 0);
        counts.insert("low", 0);

        for f in &self.findings {
            *counts.get_mut(f.severity.as_str()).unwrap() += 1;
        }
        counts
    }

    pub fn format(&self) -> String {
        let mut lines = Vec::new();
        lines.push("# Security Analysis".to_string());
        lines.push(String::new());

        let counts = self.count_by_severity();
        lines.push(format!(
            "Findings: {} critical, {} high, {} medium, {} low",
            counts["critical"], counts["high"], counts["medium"], counts["low"]
        ));

        if !self.tools_run.is_empty() {
            lines.push(format!("Tools run: {}", self.tools_run.join(", ")));
        }
        if !self.tools_skipped.is_empty() {
            lines.push(format!(
                "Tools skipped (not installed): {}",
                self.tools_skipped.join(", ")
            ));
        }

        if !self.findings.is_empty() {
            lines.push(String::new());
            lines.push("## Findings".to_string());

            // Group by severity
            let mut by_severity: Vec<_> = self.findings.iter().collect();
            by_severity.sort_by(|a, b| b.severity.cmp(&a.severity));

            for finding in by_severity.iter().take(20) {
                lines.push(format!(
                    "  [{:8}] {}:{} - {} ({})",
                    finding.severity.as_str().to_uppercase(),
                    finding.file,
                    finding.line,
                    finding.message,
                    finding.rule_id
                ));
            }

            if self.findings.len() > 20 {
                lines.push(format!(
                    "  ... and {} more findings",
                    self.findings.len() - 20
                ));
            }
        }

        lines.join("\n")
    }
}

/// Combined analysis report
#[derive(Debug)]
pub struct AnalyzeReport {
    pub health: Option<HealthReport>,
    pub complexity: Option<ComplexityReport>,
    pub security: Option<SecurityReport>,
    pub target_path: String,
    pub skipped: Vec<String>,
}

impl AnalyzeReport {
    pub fn format(&self) -> String {
        let mut sections = Vec::new();

        sections.push(format!("# Analysis: {}", self.target_path));
        sections.push(String::new());

        if let Some(ref health) = self.health {
            sections.push(health.format());
            sections.push(String::new());
        }

        if let Some(ref complexity) = self.complexity {
            sections.push("# Complexity Analysis".to_string());
            sections.push(String::new());
            sections.push(format!("Functions: {}", complexity.functions.len()));
            sections.push(format!("Average: {:.1}", complexity.avg_complexity()));
            sections.push(format!("Maximum: {}", complexity.max_complexity()));
            sections.push(format!("High risk (>10): {}", complexity.high_risk_count()));

            if !complexity.functions.is_empty() {
                sections.push(String::new());
                sections.push("## Top Complex Functions".to_string());
                let mut sorted: Vec<_> = complexity.functions.iter().collect();
                sorted.sort_by(|a, b| b.complexity.cmp(&a.complexity));
                for func in sorted.iter().take(10) {
                    // Use short_name for cleaner display (no file path prefix)
                    let display_name = if func.file_path.is_some() {
                        // For codebase-wide reports, show file:short_name
                        format!("{}:{}", func.file_path.as_ref().unwrap(), func.short_name())
                    } else {
                        func.short_name()
                    };
                    sections.push(format!(
                        "  {:3} {} ({})",
                        func.complexity,
                        display_name,
                        func.risk_level()
                    ));
                }
            }
            sections.push(String::new());
        }

        if let Some(ref security) = self.security {
            sections.push(security.format());
        }

        if !self.skipped.is_empty() {
            sections.push(String::new());
            sections.push("## Skipped".to_string());
            for s in &self.skipped {
                sections.push(format!("- {}", s));
            }
        }

        sections.join("\n")
    }

    pub fn to_json(&self) -> serde_json::Value {
        let mut obj = serde_json::Map::new();
        obj.insert(
            "target".to_string(),
            serde_json::Value::String(self.target_path.clone()),
        );

        if let Some(ref health) = self.health {
            let large_files: Vec<_> = health
                .large_files
                .iter()
                .map(|lf| {
                    serde_json::json!({
                        "path": lf.path,
                        "lines": lf.lines,
                    })
                })
                .collect();
            obj.insert(
                "health".to_string(),
                serde_json::json!({
                    "total_files": health.total_files,
                    "files_by_language": health.files_by_language,
                    "total_lines": health.total_lines,
                    "avg_complexity": health.avg_complexity,
                    "max_complexity": health.max_complexity,
                    "high_risk_functions": health.high_risk_functions,
                    "total_functions": health.total_functions,
                    "large_files": large_files,
                }),
            );
        }

        if let Some(ref complexity) = self.complexity {
            let functions: Vec<_> = complexity
                .functions
                .iter()
                .map(|f| {
                    serde_json::json!({
                        "name": f.name,
                        "parent": f.parent,
                        "short_name": f.short_name(),
                        "qualified_name": f.qualified_name(),
                        "complexity": f.complexity,
                        "line": f.start_line,
                        "risk_level": f.risk_level(),
                    })
                })
                .collect();

            obj.insert(
                "complexity".to_string(),
                serde_json::json!({
                    "file": complexity.file_path,
                    "functions": functions,
                    "avg_complexity": complexity.avg_complexity(),
                    "max_complexity": complexity.max_complexity(),
                    "high_risk_count": complexity.high_risk_count(),
                }),
            );
        }

        if let Some(ref security) = self.security {
            let findings: Vec<_> = security
                .findings
                .iter()
                .map(|f| {
                    serde_json::json!({
                        "file": f.file,
                        "line": f.line,
                        "severity": f.severity.as_str(),
                        "rule_id": f.rule_id,
                        "message": f.message,
                        "tool": f.tool,
                    })
                })
                .collect();

            obj.insert(
                "security".to_string(),
                serde_json::json!({
                    "findings": findings,
                    "counts": security.count_by_severity(),
                    "tools_run": security.tools_run,
                    "tools_skipped": security.tools_skipped,
                }),
            );
        }

        if !self.skipped.is_empty() {
            obj.insert(
                "skipped".to_string(),
                serde_json::Value::Array(
                    self.skipped
                        .iter()
                        .map(|s| serde_json::Value::String(s.clone()))
                        .collect(),
                ),
            );
        }

        serde_json::Value::Object(obj)
    }
}

/// Check if a command is available
fn command_available(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run bandit security scanner on Python code
fn run_bandit(root: &Path) -> Result<Vec<SecurityFinding>, String> {
    let output = Command::new("bandit")
        .args(["-r", "-f", "json", "-q"])
        .arg(root)
        .output()
        .map_err(|e| e.to_string())?;

    // Bandit returns exit code 1 when findings exist, which is fine
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.is_empty() {
        return Ok(Vec::new());
    }

    let json: serde_json::Value = serde_json::from_str(&stdout).map_err(|e| e.to_string())?;

    let mut findings = Vec::new();
    if let Some(results) = json.get("results").and_then(|r| r.as_array()) {
        for result in results {
            let file = result
                .get("filename")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let line = result
                .get("line_number")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            let severity_str = result
                .get("issue_severity")
                .and_then(|v| v.as_str())
                .unwrap_or("low");
            let rule_id = result
                .get("test_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let message = result
                .get("issue_text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            findings.push(SecurityFinding {
                file,
                line,
                severity: Severity::from_str(severity_str),
                rule_id,
                message,
                tool: "bandit".to_string(),
            });
        }
    }

    Ok(findings)
}

/// Run security analysis
pub fn analyze_security(root: &Path) -> SecurityReport {
    let mut report = SecurityReport::default();

    // Try bandit for Python
    if command_available("bandit") {
        match run_bandit(root) {
            Ok(findings) => {
                report.findings.extend(findings);
                report.tools_run.push("bandit".to_string());
            }
            Err(_) => {
                report.tools_skipped.push("bandit (error)".to_string());
            }
        }
    } else {
        report.tools_skipped.push("bandit".to_string());
    }

    // Could add semgrep, cargo-audit, etc. here

    report
}

/// Analyze complexity of a single file
pub fn analyze_file_complexity(file_path: &Path) -> Option<ComplexityReport> {
    let content = std::fs::read_to_string(file_path).ok()?;
    let analyzer = ComplexityAnalyzer::new();
    Some(analyzer.analyze(file_path, &content))
}

/// Analyze complexity across entire codebase, returning top N functions
pub fn analyze_codebase_complexity(
    root: &Path,
    limit: usize,
    threshold: Option<usize>,
    filter: Option<&Filter>,
) -> ComplexityReport {
    use crate::path_resolve;
    use rayon::prelude::*;

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
        // Apply filter if provided
        .filter(|f| {
            filter
                .map(|flt| flt.matches(Path::new(&f.path)))
                .unwrap_or(true)
        })
        .collect();

    // Collect all functions from all files in parallel
    let all_functions: Vec<_> = code_files
        .par_iter()
        .filter_map(|file| {
            let path = root.join(&file.path);
            let content = std::fs::read_to_string(&path).ok()?;
            let analyzer = ComplexityAnalyzer::new();
            let report = analyzer.analyze(&path, &content);
            Some(
                report
                    .functions
                    .into_iter()
                    .map(|mut f| {
                        // Include file path in the function info
                        f.file_path = Some(file.path.clone());
                        f
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .flatten()
        .collect();

    // Filter by threshold if specified
    let mut filtered: Vec<_> = if let Some(t) = threshold {
        all_functions
            .into_iter()
            .filter(|f| f.complexity >= t)
            .collect()
    } else {
        all_functions
    };

    // Sort by complexity descending and take top N
    filtered.sort_by(|a, b| b.complexity.cmp(&a.complexity));
    filtered.truncate(limit);

    ComplexityReport {
        functions: filtered,
        file_path: root.to_string_lossy().to_string(),
    }
}

/// Run unified analysis on a path
pub fn analyze(
    target: Option<&str>,
    root: &Path,
    run_health: bool,
    run_complexity: bool,
    run_security: bool,
    complexity_threshold: Option<usize>,
    kind_filter: Option<&str>,
    filter: Option<&Filter>,
) -> AnalyzeReport {
    let target_path = target.unwrap_or(".");

    // Normalize kind filter
    let kind = kind_filter.map(|k| match k.to_lowercase().as_str() {
        "function" | "functions" | "func" | "fn" => "function",
        "method" | "methods" => "method",
        _ => k,
    });

    // Use unified path resolution to handle file/symbol paths
    let (file_path, symbol_path, is_file) = if let Some(t) = target {
        if let Some(unified) = path_resolve::resolve_unified(t, root) {
            (
                Some(unified.file_path),
                unified.symbol_path,
                !unified.is_directory,
            )
        } else {
            // Fallback to plain resolve for backwards compat
            let resolved = path_resolve::resolve(t, root);
            let is_file = resolved.first().map(|f| f.kind == "file").unwrap_or(false);
            (resolved.first().map(|f| f.path.clone()), vec![], is_file)
        }
    } else {
        (None, vec![], false)
    };

    // Symbol targeting only makes sense for complexity
    let has_symbol_target = !symbol_path.is_empty();

    // Track skipped analyses for user feedback
    let skipped = Vec::new();

    let health = if run_health && !is_file && !has_symbol_target {
        // Health is codebase-wide, skip if targeting a symbol
        let analysis_root = if let Some(ref fp) = file_path {
            root.join(fp)
        } else {
            root.to_path_buf()
        };
        if analysis_root.is_dir() {
            Some(analyze_health(&analysis_root))
        } else {
            None
        }
    } else {
        None
    };

    let complexity = if run_complexity {
        if !is_file {
            // Codebase-wide complexity: show top 10 most complex functions
            let analysis_root = if let Some(ref fp) = file_path {
                root.join(fp)
            } else {
                root.to_path_buf()
            };
            if analysis_root.is_dir() {
                Some(analyze_codebase_complexity(
                    &analysis_root,
                    10,
                    complexity_threshold,
                    filter,
                ))
            } else {
                None
            }
        } else if let Some(ref fp) = file_path {
            let full_path = root.join(fp);
            let mut report = analyze_file_complexity(&full_path);

            // Apply symbol filter if targeting a specific symbol
            if let Some(ref mut r) = report {
                if has_symbol_target {
                    let target_name = symbol_path.last().unwrap();
                    let target_parent = if symbol_path.len() > 1 {
                        Some(symbol_path[symbol_path.len() - 2].as_str())
                    } else {
                        None
                    };

                    r.functions.retain(|f| {
                        // Match by name
                        if f.name != *target_name {
                            return false;
                        }
                        // If parent specified in path, match that too
                        if let Some(tp) = target_parent {
                            f.parent.as_ref().map(|p| p == tp).unwrap_or(false)
                        } else {
                            true
                        }
                    });
                }
            }

            // Apply threshold filter
            if let (Some(ref mut r), Some(threshold)) = (&mut report, complexity_threshold) {
                r.functions.retain(|f| f.complexity >= threshold);
            }

            // Apply kind filter (function = no parent, method = has parent)
            if let (Some(ref mut r), Some(k)) = (&mut report, &kind) {
                match *k {
                    "function" => r.functions.retain(|f| f.parent.is_none()),
                    "method" => r.functions.retain(|f| f.parent.is_some()),
                    _ => {} // Unknown kind, don't filter
                }
            }

            report
        } else {
            None
        }
    } else {
        None
    };

    let security = if run_security && !has_symbol_target {
        // Security doesn't apply to single symbols
        let analysis_root = if let Some(ref fp) = file_path {
            root.join(fp)
        } else {
            root.to_path_buf()
        };
        Some(analyze_security(&analysis_root))
    } else {
        None
    };

    AnalyzeReport {
        health,
        complexity,
        security,
        target_path: target_path.to_string(),
        skipped,
    }
}
