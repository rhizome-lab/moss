//! Rule execution with combined query optimization.

use crate::sources::{SourceContext, SourceRegistry, builtin_registry};
use crate::{Rule, Severity};
use rhizome_moss_languages::{GrammarLoader, support_for_path};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use streaming_iterator::StreamingIterator;

/// A finding from running a rule.
#[derive(Debug)]
pub struct Finding {
    pub rule_id: String,
    pub file: PathBuf,
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
    pub start_byte: usize,
    pub end_byte: usize,
    pub message: String,
    pub severity: Severity,
    pub matched_text: String,
    /// Auto-fix template (None if no fix available).
    pub fix: Option<String>,
    /// Capture values from the query match (for fix substitution).
    pub captures: HashMap<String, String>,
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
    loader: &GrammarLoader,
    filter_rule: Option<&str>,
    debug: &DebugFlags,
) -> Vec<Finding> {
    let start = std::time::Instant::now();

    let mut findings = Vec::new();
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

                    // Collect all captures for fix substitution
                    let mut captures_map: HashMap<String, String> = HashMap::new();
                    for cap in m.captures {
                        let name = combined.query.capture_names()[cap.index as usize].to_string();
                        if let Ok(cap_text) = cap.node.utf8_text(content.as_bytes()) {
                            captures_map.insert(name, cap_text.to_string());
                        }
                    }

                    findings.push(Finding {
                        rule_id: rule.id.clone(),
                        file: file.clone(),
                        start_line,
                        start_col: node.start_position().column + 1,
                        end_line: node.end_position().row + 1,
                        end_col: node.end_position().column + 1,
                        start_byte: node.start_byte(),
                        end_byte: node.end_byte(),
                        message: rule.message.clone(),
                        severity: rule.severity,
                        matched_text: text.lines().next().unwrap_or("").to_string(),
                        fix: rule.fix.clone(),
                        captures: captures_map,
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

/// Expand a fix template by substituting capture names with their values.
/// Uses `$capture_name` syntax. `$match` is the full matched text.
pub fn expand_fix_template(template: &str, captures: &HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (name, value) in captures {
        let placeholder = format!("${}", name);
        result = result.replace(&placeholder, value);
    }
    result
}

/// Apply fixes to findings, returning the number of files modified.
/// Fixes are applied in reverse order within each file to preserve byte offsets.
pub fn apply_fixes(findings: &[Finding]) -> std::io::Result<usize> {
    // Group findings by file
    let mut by_file: HashMap<&PathBuf, Vec<&Finding>> = HashMap::new();
    for finding in findings {
        if finding.fix.is_some() {
            by_file.entry(&finding.file).or_default().push(finding);
        }
    }

    let mut files_modified = 0;

    for (file, mut file_findings) in by_file {
        // Sort by start_byte descending so we can apply fixes without shifting offsets
        file_findings.sort_by(|a, b| b.start_byte.cmp(&a.start_byte));

        let mut content = std::fs::read_to_string(file)?;

        for finding in file_findings {
            let fix_template = finding.fix.as_ref().unwrap();
            let replacement = expand_fix_template(fix_template, &finding.captures);

            // Replace the matched region with the fix
            let before = &content[..finding.start_byte];
            let after = &content[finding.end_byte..];
            content = format!("{}{}{}", before, replacement, after);
        }

        std::fs::write(file, &content)?;
        files_modified += 1;
    }

    Ok(files_modified)
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

#[cfg(test)]
mod tests {
    use super::*;
    use rhizome_moss_languages::GrammarLoader;
    use streaming_iterator::StreamingIterator;

    fn loader() -> GrammarLoader {
        GrammarLoader::new()
    }

    /// Test that combined queries correctly scope predicates per-pattern.
    #[test]
    fn test_combined_query_predicate_scoping() {
        let loader = loader();
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
        let loader = loader();
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
