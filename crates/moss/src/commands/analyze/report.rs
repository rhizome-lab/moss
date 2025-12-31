//! Analysis report types and unified analysis orchestration.
//!
//! Provides report structs for each analysis type and the `analyze()` function
//! that orchestrates running multiple analyses based on flags.

use std::collections::HashMap;
use std::path::Path;

use crate::analyze::complexity::{ComplexityReport, RiskLevel};
use crate::analyze::function_length::{LengthCategory, LengthReport};
use crate::filter::Filter;
use crate::health::{analyze_health, HealthReport};
use crate::path_resolve;

use super::{complexity, length, security};

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

    /// Calculate security score (0-100).
    /// 100 if no findings, penalized by severity.
    pub fn score(&self) -> f64 {
        let counts = self.count_by_severity();
        let penalty = counts["critical"] * 40
            + counts["high"] * 20
            + counts["medium"] * 10
            + counts["low"] * 5;
        (100.0 - penalty as f64).max(0.0)
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
    pub length: Option<LengthReport>,
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
            let crit = complexity.critical_risk_count();
            let high = complexity.high_risk_count();
            if crit > 0 {
                sections.push(format!("Critical (>20): {}", crit));
            }
            if high > 0 || crit == 0 {
                sections.push(format!("High risk (11-20): {}", high));
            }

            if !complexity.functions.is_empty() {
                sections.push(String::new());
                sections.push("## Top Complex Functions".to_string());

                // Group by risk level (minimal format)
                let mut sorted: Vec<_> = complexity.functions.iter().collect();
                sorted.sort_by(|a, b| b.complexity.cmp(&a.complexity));
                let top_funcs: Vec<_> = sorted.iter().take(10).collect();

                let mut current_risk: Option<RiskLevel> = None;
                for func in top_funcs {
                    let risk = func.risk_level();
                    if Some(risk) != current_risk {
                        sections.push(format!("### {}", risk.as_title()));
                        current_risk = Some(risk);
                    }
                    let display_name = if func.file_path.is_some() {
                        format!("{}:{}", func.file_path.as_ref().unwrap(), func.short_name())
                    } else {
                        func.short_name()
                    };
                    sections.push(format!("{} {}", func.complexity, display_name));
                }
            }
            sections.push(String::new());
        }

        if let Some(ref length) = self.length {
            sections.push("# Function Length Analysis".to_string());
            sections.push(String::new());
            sections.push(format!("Functions: {}", length.functions.len()));
            sections.push(format!("Average: {:.1} lines", length.avg_length()));
            sections.push(format!("Maximum: {} lines", length.max_length()));
            let too_long = length.too_long_count();
            let long = length.long_count();
            if too_long > 0 {
                sections.push(format!("Too Long (>100): {}", too_long));
            }
            if long > 0 || too_long == 0 {
                sections.push(format!("Long (51-100): {}", long));
            }

            if !length.functions.is_empty() {
                sections.push(String::new());
                sections.push("## Longest Functions".to_string());

                let mut sorted: Vec<_> = length.functions.iter().collect();
                sorted.sort_by(|a, b| b.lines.cmp(&a.lines));
                let top_funcs: Vec<_> = sorted.iter().take(10).collect();

                let mut current_cat: Option<LengthCategory> = None;
                for func in top_funcs {
                    let cat = func.category();
                    if Some(cat) != current_cat {
                        sections.push(format!("### {}", cat.as_title()));
                        current_cat = Some(cat);
                    }
                    let display_name = if func.file_path.is_some() {
                        format!("{}:{}", func.file_path.as_ref().unwrap(), func.short_name())
                    } else {
                        func.short_name()
                    };
                    sections.push(format!("{} {}", func.lines, display_name));
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
                        "risk_level": f.risk_level().as_str(),
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

        if let Some(ref length) = self.length {
            let functions: Vec<_> = length
                .functions
                .iter()
                .map(|f| {
                    serde_json::json!({
                        "name": f.name,
                        "parent": f.parent,
                        "short_name": f.short_name(),
                        "lines": f.lines,
                        "start_line": f.start_line,
                        "end_line": f.end_line,
                        "category": f.category().as_str(),
                    })
                })
                .collect();

            obj.insert(
                "length".to_string(),
                serde_json::json!({
                    "file": length.file_path,
                    "functions": functions,
                    "avg_length": length.avg_length(),
                    "max_length": length.max_length(),
                    "long_count": length.long_count(),
                    "too_long_count": length.too_long_count(),
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

    /// Format as pretty text with colors (human-friendly).
    pub fn format_pretty(&self) -> String {
        use nu_ansi_term::Color::{Red, Yellow};

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
            let crit = complexity.critical_risk_count();
            let high = complexity.high_risk_count();
            if crit > 0 {
                sections.push(format!("Critical (>20): {}", crit));
            }
            if high > 0 || crit == 0 {
                sections.push(format!("High risk (11-20): {}", high));
            }

            if !complexity.functions.is_empty() {
                sections.push(String::new());
                sections.push("## Top Complex Functions".to_string());

                let mut sorted: Vec<_> = complexity.functions.iter().collect();
                sorted.sort_by(|a, b| b.complexity.cmp(&a.complexity));

                for func in sorted.iter().take(10) {
                    let display_name = if func.file_path.is_some() {
                        format!("{}:{}", func.file_path.as_ref().unwrap(), func.short_name())
                    } else {
                        func.short_name()
                    };

                    let risk = func.risk_level();
                    let risk_colored = match risk {
                        RiskLevel::Critical => Red.bold().paint("CRIT").to_string(),
                        RiskLevel::High => Red.paint("HIGH").to_string(),
                        RiskLevel::Moderate => Yellow.paint(" MOD").to_string(),
                        RiskLevel::Low => "    ".to_string(),
                    };
                    sections.push(format!(
                        "{}  {:3}  {}",
                        risk_colored, func.complexity, display_name
                    ));
                }
            }
            sections.push(String::new());
        }

        if let Some(ref length) = self.length {
            use nu_ansi_term::Color::Cyan;

            sections.push("# Function Length Analysis".to_string());
            sections.push(String::new());
            sections.push(format!("Functions: {}", length.functions.len()));
            sections.push(format!("Average: {:.1} lines", length.avg_length()));
            sections.push(format!("Maximum: {} lines", length.max_length()));
            let too_long = length.too_long_count();
            let long = length.long_count();
            if too_long > 0 {
                sections.push(format!("Too Long (>100): {}", too_long));
            }
            if long > 0 || too_long == 0 {
                sections.push(format!("Long (51-100): {}", long));
            }

            if !length.functions.is_empty() {
                sections.push(String::new());
                sections.push("## Longest Functions".to_string());

                let mut sorted: Vec<_> = length.functions.iter().collect();
                sorted.sort_by(|a, b| b.lines.cmp(&a.lines));

                for func in sorted.iter().take(10) {
                    let display_name = if func.file_path.is_some() {
                        format!("{}:{}", func.file_path.as_ref().unwrap(), func.short_name())
                    } else {
                        func.short_name()
                    };

                    let cat = func.category();
                    let cat_colored = match cat {
                        LengthCategory::TooLong => Red.bold().paint("LONG").to_string(),
                        LengthCategory::Long => Yellow.paint("LONG").to_string(),
                        LengthCategory::Medium => Cyan.paint(" MED").to_string(),
                        LengthCategory::Short => "    ".to_string(),
                    };
                    sections.push(format!(
                        "{}  {:3}  {}",
                        cat_colored, func.lines, display_name
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
}

/// Run unified analysis on a path
pub fn analyze(
    target: Option<&str>,
    root: &Path,
    run_health: bool,
    run_complexity: bool,
    run_length: bool,
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
                Some(complexity::analyze_codebase_complexity(
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
            let mut report = complexity::analyze_file_complexity(&full_path);

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

    let length = if run_length {
        if !is_file {
            // Codebase-wide length: show top 10 longest functions
            let analysis_root = if let Some(ref fp) = file_path {
                root.join(fp)
            } else {
                root.to_path_buf()
            };
            if analysis_root.is_dir() {
                Some(length::analyze_codebase_length(&analysis_root, 10, filter))
            } else {
                None
            }
        } else if let Some(ref fp) = file_path {
            let full_path = root.join(fp);
            length::analyze_file_length(&full_path)
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
        Some(security::analyze_security(&analysis_root))
    } else {
        None
    };

    AnalyzeReport {
        health,
        complexity,
        length,
        security,
        target_path: target_path.to_string(),
        skipped,
    }
}

/// Calculate weighted average grade from scores.
/// Each score is (value, weight) where value is 0-100.
pub fn calculate_grade(scores: &[(f64, f64)]) -> (&'static str, f64) {
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
