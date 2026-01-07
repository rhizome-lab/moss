//! Syntax-based linting with tree-sitter queries.
//!
//! Rules are defined in `.moss/rules/*.scm` files with TOML frontmatter:
//!
//! ```scm
//! # ---
//! # id = "no-unwrap"
//! # severity = "warning"
//! # message = "Avoid unwrap() on user input"
//! # allow = ["**/tests/**"]
//! # ---
//!
//! (call_expression
//!   function: (field_expression
//!     field: (field_identifier) @method)
//!   (#eq? @method "unwrap")) @match
//! ```

use crate::parsers::grammar_loader;
use glob::Pattern;
use moss_languages::support_for_path;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use streaming_iterator::StreamingIterator;

/// Severity level for rule findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Severity {
    Error,
    #[default]
    Warning,
    Info,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
            Severity::Warning => write!(f, "warning"),
            Severity::Info => write!(f, "info"),
        }
    }
}

impl std::str::FromStr for Severity {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "error" => Ok(Severity::Error),
            "warning" | "warn" => Ok(Severity::Warning),
            "info" | "note" => Ok(Severity::Info),
            _ => Err(format!("unknown severity: {}", s)),
        }
    }
}

/// A syntax rule definition.
#[derive(Debug)]
pub struct Rule {
    /// Unique identifier for this rule.
    pub id: String,
    /// The tree-sitter query pattern.
    pub query_str: String,
    /// Severity level.
    pub severity: Severity,
    /// Message to display when the rule matches.
    pub message: String,
    /// Glob patterns for files where matches are allowed.
    pub allow: Vec<Pattern>,
    /// Source file path of this rule.
    pub source_path: PathBuf,
    /// Languages this rule applies to (inferred from query or explicit).
    pub languages: Vec<String>,
}

/// A finding from running a rule.
#[derive(Debug)]
pub struct Finding {
    pub rule_id: String,
    pub file: PathBuf,
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
    pub message: String,
    pub severity: Severity,
    pub matched_text: String,
}

/// Load rules from a directory.
pub fn load_rules(rules_dir: &Path) -> Vec<Rule> {
    let mut rules = Vec::new();

    if !rules_dir.exists() {
        return rules;
    }

    let entries = match std::fs::read_dir(rules_dir) {
        Ok(e) => e,
        Err(_) => return rules,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e == "scm").unwrap_or(false) {
            if let Some(rule) = parse_rule_file(&path) {
                rules.push(rule);
            }
        }
    }

    rules
}

/// Parse a rule file with TOML frontmatter.
fn parse_rule_file(path: &Path) -> Option<Rule> {
    let content = std::fs::read_to_string(path).ok()?;

    // Find frontmatter between # --- markers
    let lines: Vec<&str> = content.lines().collect();

    let mut in_frontmatter = false;
    let mut frontmatter_lines = Vec::new();
    let mut query_lines = Vec::new();

    for line in &lines {
        let trimmed = line.trim();
        if trimmed == "# ---" {
            if in_frontmatter {
                // End of frontmatter
                in_frontmatter = false;
            } else {
                // Start of frontmatter
                in_frontmatter = true;
            }
            continue;
        }

        if in_frontmatter {
            // Strip leading # and space from frontmatter lines
            let fm_line = line.strip_prefix('#').unwrap_or(line).trim_start();
            frontmatter_lines.push(fm_line);
        } else if !in_frontmatter && !frontmatter_lines.is_empty() {
            // After frontmatter, collect query
            query_lines.push(*line);
        } else if frontmatter_lines.is_empty() && !trimmed.is_empty() && !trimmed.starts_with('#') {
            // No frontmatter, just query
            query_lines.push(*line);
        }
    }

    // If no frontmatter found, the whole file is the query
    let (frontmatter_str, query_str) = if frontmatter_lines.is_empty() {
        (String::new(), content.clone())
    } else {
        (frontmatter_lines.join("\n"), query_lines.join("\n"))
    };

    // Parse frontmatter as TOML
    let frontmatter: toml::Value = if frontmatter_str.is_empty() {
        toml::Value::Table(toml::map::Map::new())
    } else {
        match toml::from_str(&frontmatter_str) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Warning: invalid frontmatter in {}: {}", path.display(), e);
                return None;
            }
        }
    };

    // Extract rule metadata
    let id = frontmatter
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string()
        });

    let severity = frontmatter
        .get("severity")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(Severity::Warning);

    let message = frontmatter
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("Rule violation")
        .to_string();

    let allow: Vec<Pattern> = frontmatter
        .get("allow")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .filter_map(|s| Pattern::new(s).ok())
                .collect()
        })
        .unwrap_or_default();

    let languages: Vec<String> = frontmatter
        .get("languages")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    Some(Rule {
        id,
        query_str: query_str.trim().to_string(),
        severity,
        message,
        allow,
        source_path: path.to_path_buf(),
        languages,
    })
}

/// Run rules against files in a directory.
pub fn run_rules(rules: &[Rule], root: &Path, filter_rule: Option<&str>) -> Vec<Finding> {
    let mut findings = Vec::new();
    let loader = grammar_loader();

    // Collect all source files
    let files = collect_source_files(root);

    // Group files by language/grammar
    let mut files_by_grammar: HashMap<String, Vec<PathBuf>> = HashMap::new();
    for file in files {
        if let Some(lang) = support_for_path(&file) {
            let grammar_name = lang.grammar_name().to_string();
            files_by_grammar.entry(grammar_name).or_default().push(file);
        }
    }

    // Run each rule
    for rule in rules {
        // Filter by rule ID if specified
        if let Some(filter) = filter_rule {
            if rule.id != filter {
                continue;
            }
        }

        // Determine which grammars this rule applies to
        let target_grammars: Vec<&String> = if rule.languages.is_empty() {
            // Try to infer from query - for now, try all grammars
            files_by_grammar.keys().collect()
        } else {
            rule.languages.iter().collect()
        };

        for grammar_name in target_grammars {
            let Some(grammar) = loader.get(grammar_name) else {
                continue;
            };

            // Compile query for this grammar
            let query = match tree_sitter::Query::new(&grammar, &rule.query_str) {
                Ok(q) => q,
                Err(_) => {
                    // Query doesn't apply to this grammar
                    continue;
                }
            };

            // Find the @match capture index
            let match_idx = query
                .capture_names()
                .iter()
                .position(|n| *n == "match")
                .unwrap_or(0);

            // Get files for this grammar
            let Some(files) = files_by_grammar.get(grammar_name) else {
                continue;
            };

            // Run query on each file
            for file in files {
                // Check if file is in allowlist
                let rel_path = file.strip_prefix(root).unwrap_or(file);
                let rel_path_str = rel_path.to_string_lossy();
                if rule.allow.iter().any(|p| p.matches(&rel_path_str)) {
                    continue;
                }

                let content = match std::fs::read_to_string(file) {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                // Parse file
                let mut parser = tree_sitter::Parser::new();
                if parser.set_language(&grammar).is_err() {
                    continue;
                }

                let tree = match parser.parse(&content, None) {
                    Some(t) => t,
                    None => continue,
                };

                // Run query
                let mut cursor = tree_sitter::QueryCursor::new();
                let mut matches = cursor.matches(&query, tree.root_node(), content.as_bytes());

                while let Some(m) = matches.next() {
                    // Evaluate predicates
                    if !evaluate_predicates(&query, m, content.as_bytes()) {
                        continue;
                    }

                    // Find the @match capture
                    let capture = m.captures.iter().find(|c| c.index as usize == match_idx);

                    if let Some(cap) = capture {
                        let node = cap.node;
                        let text = node.utf8_text(content.as_bytes()).unwrap_or("");

                        findings.push(Finding {
                            rule_id: rule.id.clone(),
                            file: file.clone(),
                            start_line: node.start_position().row + 1,
                            start_col: node.start_position().column + 1,
                            end_line: node.end_position().row + 1,
                            end_col: node.end_position().column + 1,
                            message: rule.message.clone(),
                            severity: rule.severity,
                            matched_text: text.lines().next().unwrap_or("").to_string(),
                        });
                    }
                }
            }
        }
    }

    findings
}

/// Evaluate predicates for a match.
pub fn evaluate_predicates(
    query: &tree_sitter::Query,
    match_: &tree_sitter::QueryMatch,
    source: &[u8],
) -> bool {
    let predicates = query.general_predicates(match_.pattern_index);
    // Remove debug output
    for predicate in predicates {
        let name = &predicate.operator;
        let args = &predicate.args;

        match name.as_ref() {
            "eq?" | "not-eq?" => {
                if args.len() < 2 {
                    continue;
                }

                // Get first capture's text
                let first_text = match &args[0] {
                    tree_sitter::QueryPredicateArg::Capture(idx) => match_
                        .captures
                        .iter()
                        .find(|c| c.index == *idx)
                        .and_then(|c| c.node.utf8_text(source).ok())
                        .unwrap_or(""),
                    tree_sitter::QueryPredicateArg::String(s) => s.as_ref(),
                };

                // Get second value (capture or string)
                let second_text = match &args[1] {
                    tree_sitter::QueryPredicateArg::Capture(idx) => match_
                        .captures
                        .iter()
                        .find(|c| c.index == *idx)
                        .and_then(|c| c.node.utf8_text(source).ok())
                        .unwrap_or(""),
                    tree_sitter::QueryPredicateArg::String(s) => s.as_ref(),
                };

                let equal = first_text == second_text;
                if name.as_ref() == "eq?" && !equal {
                    return false;
                }
                if name.as_ref() == "not-eq?" && equal {
                    return false;
                }
            }
            "match?" | "not-match?" => {
                if args.len() < 2 {
                    continue;
                }

                // Get capture's text
                let capture_text = match &args[0] {
                    tree_sitter::QueryPredicateArg::Capture(idx) => match_
                        .captures
                        .iter()
                        .find(|c| c.index == *idx)
                        .and_then(|c| c.node.utf8_text(source).ok())
                        .unwrap_or(""),
                    _ => continue,
                };

                // Get regex pattern
                let pattern = match &args[1] {
                    tree_sitter::QueryPredicateArg::String(s) => s.as_ref(),
                    _ => continue,
                };

                // Compile and match regex
                let regex = match regex::Regex::new(pattern) {
                    Ok(r) => r,
                    Err(_) => continue,
                };

                let matches = regex.is_match(capture_text);
                if name.as_ref() == "match?" && !matches {
                    return false;
                }
                if name.as_ref() == "not-match?" && matches {
                    return false;
                }
            }
            "any-of?" => {
                if args.len() < 2 {
                    continue;
                }

                // Get capture's text
                let capture_text = match &args[0] {
                    tree_sitter::QueryPredicateArg::Capture(idx) => match_
                        .captures
                        .iter()
                        .find(|c| c.index == *idx)
                        .and_then(|c| c.node.utf8_text(source).ok())
                        .unwrap_or(""),
                    _ => continue,
                };

                // Check if any of the remaining args match
                let any_match = args[1..].iter().any(|arg| match arg {
                    tree_sitter::QueryPredicateArg::String(s) => s.as_ref() == capture_text,
                    _ => false,
                });

                if !any_match {
                    return false;
                }
            }
            _ => {
                // Unknown predicate - ignore
            }
        }
    }
    true
}

/// Collect source files from a directory.
fn collect_source_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();

    let walker = ignore::WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .build();

    for entry in walker.flatten() {
        let path = entry.path();
        if path.is_file() && support_for_path(path).is_some() {
            files.push(path.to_path_buf());
        }
    }

    files
}

/// Run the rules command.
pub fn cmd_rules(root: &Path, filter_rule: Option<&str>, list_only: bool, json: bool) -> i32 {
    // Load rules from .moss/rules/
    let rules_dir = root.join(".moss").join("rules");
    let rules = load_rules(&rules_dir);

    if rules.is_empty() {
        if !list_only {
            eprintln!("No rules found in {}", rules_dir.display());
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
                        "source": r.source_path.to_string_lossy(),
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&list).unwrap());
        } else {
            println!("Available rules ({}):", rules.len());
            println!();
            for rule in &rules {
                println!("  {} ({}) - {}", rule.id, rule.severity, rule.message);
            }
        }
        return 0;
    }

    // Run rules
    let findings = run_rules(&rules, root, filter_rule);

    if json {
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
