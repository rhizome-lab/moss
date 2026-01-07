//! Syntax-based linting with tree-sitter queries.
//!
//! Rules are loaded from multiple sources (later overrides earlier by `id`):
//! 1. Embedded builtins (compiled into moss)
//! 2. User global rules (`~/.config/moss/rules/*.scm`)
//! 3. Project rules (`.moss/rules/*.scm`)
//!
//! Rule file format:
//!
//! ```scm
//! # ---
//! # id = "no-unwrap"
//! # severity = "warning"
//! # message = "Avoid unwrap() on user input"
//! # allow = ["**/tests/**"]
//! # enabled = true  # set to false to disable a builtin
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

// Embedded builtin rules
use super::builtin_rules::BUILTIN_RULES;
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
    /// Source file path of this rule (empty for builtins).
    pub source_path: PathBuf,
    /// Languages this rule applies to (inferred from query or explicit).
    pub languages: Vec<String>,
    /// Whether this rule is enabled.
    pub enabled: bool,
    /// Whether this is a builtin rule.
    pub builtin: bool,
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

/// Load all rules from all sources, merged by ID.
/// Order: builtins → ~/.config/moss/rules/ → .moss/rules/
pub fn load_all_rules(project_root: &Path) -> Vec<Rule> {
    let mut rules_by_id: HashMap<String, Rule> = HashMap::new();

    // 1. Load embedded builtins
    for builtin in BUILTIN_RULES {
        if let Some(rule) = parse_rule_content(builtin.content, builtin.id, true) {
            rules_by_id.insert(rule.id.clone(), rule);
        }
    }

    // 2. Load user global rules (~/.config/moss/rules/)
    if let Some(config_dir) = dirs::config_dir() {
        let user_rules_dir = config_dir.join("moss").join("rules");
        for rule in load_rules_from_dir(&user_rules_dir) {
            rules_by_id.insert(rule.id.clone(), rule);
        }
    }

    // 3. Load project rules (.moss/rules/)
    let project_rules_dir = project_root.join(".moss").join("rules");
    for rule in load_rules_from_dir(&project_rules_dir) {
        rules_by_id.insert(rule.id.clone(), rule);
    }

    // Filter out disabled rules
    rules_by_id.into_values().filter(|r| r.enabled).collect()
}

/// Load rules from a directory.
fn load_rules_from_dir(rules_dir: &Path) -> Vec<Rule> {
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
    let default_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    let mut rule = parse_rule_content(&content, default_id, false)?;
    rule.source_path = path.to_path_buf();
    Some(rule)
}

/// Parse rule content string with TOML frontmatter.
fn parse_rule_content(content: &str, default_id: &str, is_builtin: bool) -> Option<Rule> {
    let lines: Vec<&str> = content.lines().collect();

    let mut in_frontmatter = false;
    let mut frontmatter_lines = Vec::new();
    let mut query_lines = Vec::new();

    for line in &lines {
        let trimmed = line.trim();
        if trimmed == "# ---" {
            if in_frontmatter {
                in_frontmatter = false;
            } else {
                in_frontmatter = true;
            }
            continue;
        }

        if in_frontmatter {
            let fm_line = line.strip_prefix('#').unwrap_or(line).trim_start();
            frontmatter_lines.push(fm_line);
        } else if !in_frontmatter && !frontmatter_lines.is_empty() {
            query_lines.push(*line);
        } else if frontmatter_lines.is_empty() && !trimmed.is_empty() && !trimmed.starts_with('#') {
            query_lines.push(*line);
        }
    }

    let (frontmatter_str, query_str) = if frontmatter_lines.is_empty() {
        (String::new(), content.to_string())
    } else {
        (frontmatter_lines.join("\n"), query_lines.join("\n"))
    };

    let frontmatter: toml::Value = if frontmatter_str.is_empty() {
        toml::Value::Table(toml::map::Map::new())
    } else {
        match toml::from_str(&frontmatter_str) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Warning: invalid frontmatter: {}", e);
                return None;
            }
        }
    };

    let id = frontmatter
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| default_id.to_string());

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

    let enabled = frontmatter
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    Some(Rule {
        id,
        query_str: query_str.trim().to_string(),
        severity,
        message,
        allow,
        source_path: PathBuf::new(),
        languages,
        enabled,
        builtin: is_builtin,
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
    // Load rules from all sources (builtins + user global + project)
    let rules = load_all_rules(root);

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
