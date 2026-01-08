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
use super::RulesConfig;
use super::builtin_rules::BUILTIN_RULES;
use super::rule_sources::{SourceContext, SourceRegistry, builtin_registry};
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
    /// Conditions that must be met for this rule to apply.
    /// Format: { "namespace.key" = "value" } or { "namespace.key" = ">=value" }
    pub requires: HashMap<String, String>,
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

/// Check if a line contains a moss-allow comment for the given rule.
/// Supports: `// moss-allow: rule-id` or `/* moss-allow: rule-id */`
fn line_has_allow_comment(line: &str, rule_id: &str) -> bool {
    // Look for moss-allow: followed by the rule ID
    // Pattern: moss-allow: rule-id (optionally followed by - reason)
    if let Some(pos) = line.find("moss-allow:") {
        let after = &line[pos + 11..]; // len("moss-allow:")
        let after = after.trim_start();
        // Check if rule_id matches (might be followed by space, dash, or end of comment)
        if after.starts_with(rule_id) {
            let rest = &after[rule_id.len()..];
            // Valid if followed by nothing, whitespace, dash (reason), or end of comment
            return rest.is_empty()
                || rest.starts_with(char::is_whitespace)
                || rest.starts_with('-')
                || rest.starts_with("*/");
        }
    }
    false
}

/// Check if a finding should be allowed based on inline comments.
/// Checks the line of the finding and the line before.
fn is_allowed_by_comment(content: &str, start_line: usize, rule_id: &str) -> bool {
    let lines: Vec<&str> = content.lines().collect();
    let line_idx = start_line.saturating_sub(1); // 0-indexed

    // Check the line itself
    if let Some(line) = lines.get(line_idx) {
        if line_has_allow_comment(line, rule_id) {
            return true;
        }
    }

    // Check the line before (for standalone comment)
    if line_idx > 0 {
        if let Some(line) = lines.get(line_idx - 1) {
            if line_has_allow_comment(line, rule_id) {
                return true;
            }
        }
    }

    false
}

/// Check if a rule's requires conditions are met for a given file context.
///
/// Supports operators:
/// - `value` - exact match
/// - `>=value` - greater or equal (for versions/editions)
/// - `<=value` - less or equal
/// - `!value` - not equal
fn check_requires(rule: &Rule, registry: &SourceRegistry, ctx: &SourceContext) -> bool {
    if rule.requires.is_empty() {
        return true;
    }

    for (key, expected) in &rule.requires {
        let actual = match registry.get(ctx, key) {
            Some(v) => v,
            None => return false, // Required source not available
        };

        // Parse operator prefix
        let matches = if let Some(rest) = expected.strip_prefix(">=") {
            actual >= rest.to_string()
        } else if let Some(rest) = expected.strip_prefix("<=") {
            actual <= rest.to_string()
        } else if let Some(rest) = expected.strip_prefix('!') {
            actual != rest
        } else {
            actual == *expected
        };

        if !matches {
            return false;
        }
    }

    true
}

/// Load all rules from all sources, merged by ID.
/// Order: builtins → ~/.config/moss/rules/ → .moss/rules/
/// Then applies config overrides (severity, disable).
pub fn load_all_rules(project_root: &Path, config: &RulesConfig) -> Vec<Rule> {
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

    // 4. Apply config overrides
    for (rule_id, override_cfg) in &config.0 {
        if let Some(rule) = rules_by_id.get_mut(rule_id) {
            if let Some(ref severity_str) = override_cfg.severity {
                if let Ok(severity) = severity_str.parse() {
                    rule.severity = severity;
                }
            }
            if let Some(enabled) = override_cfg.enabled {
                rule.enabled = enabled;
            }
            // Merge additional allow patterns from config
            for pattern_str in &override_cfg.allow {
                if let Ok(pattern) = Pattern::new(pattern_str) {
                    rule.allow.push(pattern);
                }
            }
        }
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

    let requires: HashMap<String, String> = frontmatter
        .get("requires")
        .and_then(|v| v.as_table())
        .map(|tbl| {
            tbl.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();

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
        requires,
    })
}

/// Combined query for a grammar with pattern-to-rule mapping.
struct CombinedQuery<'a> {
    query: tree_sitter::Query,
    /// Maps pattern_index to (rule, match_capture_index_in_combined_query)
    pattern_to_rule: Vec<(&'a Rule, usize)>,
}

/// Run rules against files in a directory.
/// Optimized: combines all rules into single query per grammar for single-traversal matching.
pub fn run_rules(
    rules: &[Rule],
    root: &Path,
    filter_rule: Option<&str>,
    debug: &DebugFlags,
) -> Vec<Finding> {
    let start = std::time::Instant::now();

    let mut findings = Vec::new();
    let loader = grammar_loader();
    let source_registry = builtin_registry();

    // Filter rules first
    let active_rules: Vec<&Rule> = rules
        .iter()
        .filter(|r| filter_rule.map_or(true, |f| r.id == f))
        .collect();

    if active_rules.is_empty() {
        return findings;
    }

    // Collect all source files and group by grammar
    let files = collect_source_files(root);
    let mut files_by_grammar: HashMap<String, Vec<PathBuf>> = HashMap::new();
    for file in files {
        if let Some(lang) = support_for_path(&file) {
            let grammar_name = lang.grammar_name().to_string();
            files_by_grammar.entry(grammar_name).or_default().push(file);
        }
    }

    if debug.timing {
        eprintln!("[timing] file collection: {:?}", start.elapsed());
    }
    let compile_start = std::time::Instant::now();

    // Separate rules: language-specific vs cross-language (need per-grammar validation)
    let (specific_rules, global_rules): (Vec<&&Rule>, Vec<&&Rule>) =
        active_rules.iter().partition(|r| !r.languages.is_empty());

    // Build combined queries: one per grammar
    let mut combined_by_grammar: HashMap<String, CombinedQuery> = HashMap::new();

    for grammar_name in files_by_grammar.keys() {
        let Some(grammar) = loader.get(grammar_name) else {
            continue;
        };

        let mut compiled_rules: Vec<(&Rule, tree_sitter::Query)> = Vec::new();

        // Pass 1: Language-specific rules - compile directly (trust the author)
        for rule in &specific_rules {
            if rule.languages.iter().any(|l| l == grammar_name) {
                if let Ok(q) = tree_sitter::Query::new(&grammar, &rule.query_str) {
                    compiled_rules.push((rule, q));
                }
            }
        }

        // Pass 2: Cross-language rules - validate each one
        for rule in &global_rules {
            if let Ok(q) = tree_sitter::Query::new(&grammar, &rule.query_str) {
                compiled_rules.push((rule, q));
            }
        }

        if compiled_rules.is_empty() {
            continue;
        }

        // Combine all into one query
        let combined_str = compiled_rules
            .iter()
            .map(|(r, _)| r.query_str.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");

        let query = match tree_sitter::Query::new(&grammar, &combined_str) {
            Ok(q) => q,
            Err(e) => {
                eprintln!("Warning: combined query failed for {}: {}", grammar_name, e);
                continue;
            }
        };

        // Map pattern indices to rules
        let mut pattern_to_rule: Vec<(&Rule, usize)> = Vec::new();
        let combined_match_idx = query
            .capture_names()
            .iter()
            .position(|n| *n == "match")
            .unwrap_or(0);

        for (rule, individual_query) in &compiled_rules {
            for _ in 0..individual_query.pattern_count() {
                pattern_to_rule.push((*rule, combined_match_idx));
            }
        }

        combined_by_grammar.insert(
            grammar_name.clone(),
            CombinedQuery {
                query,
                pattern_to_rule,
            },
        );
    }

    if debug.timing {
        eprintln!(
            "[timing] query compilation: {:?} ({} grammars)",
            compile_start.elapsed(),
            combined_by_grammar.len()
        );
    }
    let process_start = std::time::Instant::now();

    // Process files: single query execution per file
    for (grammar_name, files) in &files_by_grammar {
        let Some(combined) = combined_by_grammar.get(grammar_name) else {
            continue;
        };

        let Some(grammar) = loader.get(grammar_name) else {
            continue;
        };

        let mut parser = tree_sitter::Parser::new();
        if parser.set_language(&grammar).is_err() {
            continue;
        }

        for file in files {
            let rel_path = file.strip_prefix(root).unwrap_or(file);
            let rel_path_str = rel_path.to_string_lossy();

            // Build source context for this file (used for requires evaluation)
            let source_ctx = SourceContext {
                file_path: file,
                rel_path: &rel_path_str,
                project_root: root,
            };

            let content = match std::fs::read_to_string(file) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let tree = match parser.parse(&content, None) {
                Some(t) => t,
                None => continue,
            };

            // Single query execution - one traversal for all rules
            let mut cursor = tree_sitter::QueryCursor::new();
            let mut matches = cursor.matches(&combined.query, tree.root_node(), content.as_bytes());

            while let Some(m) = matches.next() {
                // Look up which rule this pattern belongs to
                let Some((rule, match_idx)) = combined.pattern_to_rule.get(m.pattern_index) else {
                    continue;
                };

                // Check allow patterns for this specific rule
                if rule.allow.iter().any(|p| p.matches(&rel_path_str)) {
                    continue;
                }

                // Check requires conditions
                if !check_requires(rule, &source_registry, &source_ctx) {
                    continue;
                }

                if !evaluate_predicates(&combined.query, m, content.as_bytes()) {
                    continue;
                }

                let capture = m.captures.iter().find(|c| c.index as usize == *match_idx);

                if let Some(cap) = capture {
                    let node = cap.node;
                    let start_line = node.start_position().row + 1;

                    if is_allowed_by_comment(&content, start_line, &rule.id) {
                        continue;
                    }

                    let text = node.utf8_text(content.as_bytes()).unwrap_or("");

                    findings.push(Finding {
                        rule_id: rule.id.clone(),
                        file: file.clone(),
                        start_line,
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

    if debug.timing {
        eprintln!(
            "[timing] file processing: {:?} ({} findings)",
            process_start.elapsed(),
            findings.len()
        );
        eprintln!("[timing] total: {:?}", start.elapsed());
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

/// Debug output categories.
#[derive(Default)]
pub struct DebugFlags {
    pub timing: bool,
}

impl DebugFlags {
    pub fn from_args(args: &[String]) -> Self {
        let all = args.iter().any(|s| s == "all");
        Self {
            timing: all || args.iter().any(|s| s == "timing"),
        }
    }
}

/// Run the rules command.
pub fn cmd_rules(
    root: &Path,
    filter_rule: Option<&str>,
    list_only: bool,
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

    // Run rules
    let findings = run_rules(&rules, root, filter_rule, debug);

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

#[cfg(test)]
mod tests {
    use super::*;
    use streaming_iterator::StreamingIterator;

    /// Test that combined queries correctly scope predicates per-pattern.
    #[test]
    fn test_combined_query_predicate_scoping() {
        let loader = grammar_loader();
        let grammar = loader.get("rust").expect("rust grammar");

        // Two patterns with same capture name but different predicate values
        let combined_query = r#"
; Pattern 0: matches unwrap
((call_expression
  function: (field_expression field: (field_identifier) @_method)
  (#eq? @_method "unwrap")) @match)

; Pattern 1: matches expect
((call_expression
  function: (field_expression field: (field_identifier) @_method)
  (#eq? @_method "expect")) @match)
"#;

        let query = tree_sitter::Query::new(&grammar, combined_query)
            .expect("combined query should compile");

        assert_eq!(query.pattern_count(), 2, "should have 2 patterns");

        let test_code = r#"
fn main() {
    let x = Some(5);
    x.unwrap();      // line 4 - should match pattern 0
    x.expect("msg"); // line 5 - should match pattern 1
    x.map(|v| v);    // line 6 - should NOT match
}
"#;

        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&grammar).unwrap();
        let tree = parser.parse(test_code, None).unwrap();

        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), test_code.as_bytes());

        let mut results: Vec<(usize, String)> = Vec::new();
        while let Some(m) = matches.next() {
            // Check predicates - this is what we're testing
            if !evaluate_predicates(&query, m, test_code.as_bytes()) {
                continue;
            }

            let match_capture = m
                .captures
                .iter()
                .find(|c| query.capture_names()[c.index as usize] == "match");

            if let Some(cap) = match_capture {
                let text = cap.node.utf8_text(test_code.as_bytes()).unwrap();
                results.push((m.pattern_index, text.to_string()));
            }
        }

        // Should have exactly 2 matches
        assert_eq!(results.len(), 2, "should have 2 matches, got {:?}", results);

        // Pattern 0 should match unwrap
        assert!(
            results
                .iter()
                .any(|(idx, text)| *idx == 0 && text.contains("unwrap")),
            "pattern 0 should match unwrap, got {:?}",
            results
        );

        // Pattern 1 should match expect
        assert!(
            results
                .iter()
                .any(|(idx, text)| *idx == 1 && text.contains("expect")),
            "pattern 1 should match expect, got {:?}",
            results
        );
    }

    /// Test that multiple rules can be combined into single query.
    #[test]
    fn test_combined_rules_single_traversal() {
        let loader = grammar_loader();
        let grammar = loader.get("rust").expect("rust grammar");

        // Simulate combining multiple rule queries
        let rules_queries = vec![
            (
                "unwrap-rule",
                r#"((call_expression function: (field_expression field: (field_identifier) @_m) (#eq? @_m "unwrap")) @match)"#,
            ),
            (
                "dbg-rule",
                r#"((macro_invocation macro: (identifier) @_name (#eq? @_name "dbg")) @match)"#,
            ),
        ];

        // Combine into single query
        let combined = rules_queries
            .iter()
            .map(|(_, q)| *q)
            .collect::<Vec<_>>()
            .join("\n\n");

        let query =
            tree_sitter::Query::new(&grammar, &combined).expect("combined query should compile");

        let test_code = r#"
fn main() {
    let x = Some(5);
    dbg!(x);        // should match pattern 1 (dbg-rule)
    x.unwrap();     // should match pattern 0 (unwrap-rule)
}
"#;

        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&grammar).unwrap();
        let tree = parser.parse(test_code, None).unwrap();

        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), test_code.as_bytes());

        let mut pattern_indices: Vec<usize> = Vec::new();
        while let Some(m) = matches.next() {
            if evaluate_predicates(&query, m, test_code.as_bytes()) {
                pattern_indices.push(m.pattern_index);
            }
        }

        // Should match both patterns
        assert!(
            pattern_indices.contains(&0),
            "should match pattern 0 (unwrap)"
        );
        assert!(pattern_indices.contains(&1), "should match pattern 1 (dbg)");
    }
}
