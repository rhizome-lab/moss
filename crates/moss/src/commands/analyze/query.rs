//! Tree-sitter query testing for syntax rule authoring.

use crate::parsers::grammar_loader;
use rhizome_moss_languages::support_for_path;
use std::path::Path;
use streaming_iterator::StreamingIterator;

use rhizome_moss_rules::evaluate_predicates;

/// Test a tree-sitter query against a file.
pub fn cmd_query(file: &Path, query_str: &str, show_source: bool, json: bool) -> i32 {
    // Read file
    let content = match std::fs::read_to_string(file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to read {}: {}", file.display(), e);
            return 1;
        }
    };

    // Detect language
    let Some(lang) = support_for_path(file) else {
        eprintln!("Unknown file type: {}", file.display());
        return 1;
    };

    // Load grammar
    let loader = grammar_loader();
    let Some(grammar) = loader.get(lang.grammar_name()) else {
        eprintln!("Failed to load grammar for {}", lang.name());
        return 1;
    };

    // Parse file
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&grammar).unwrap();
    let tree = match parser.parse(&content, None) {
        Some(t) => t,
        None => {
            eprintln!("Failed to parse file");
            return 1;
        }
    };

    // Compile query
    let query = match tree_sitter::Query::new(&grammar, query_str) {
        Ok(q) => q,
        Err(e) => {
            eprintln!("Invalid query: {}", e);
            return 1;
        }
    };

    // Run query - collect all matches
    let mut cursor = tree_sitter::QueryCursor::new();
    let mut matches_iter = cursor.matches(&query, tree.root_node(), content.as_bytes());

    // Collect match data since QueryMatches is a streaming iterator
    struct MatchData {
        captures: Vec<CaptureData>,
    }
    struct CaptureData {
        index: usize,
        kind: String,
        text: String,
        start_row: usize,
        start_col: usize,
        end_row: usize,
        end_col: usize,
    }

    let mut matches: Vec<MatchData> = Vec::new();
    while let Some(m) = matches_iter.next() {
        // Evaluate predicates
        if !evaluate_predicates(&query, m, content.as_bytes()) {
            continue;
        }

        let captures: Vec<CaptureData> = m
            .captures
            .iter()
            .map(|cap| {
                let node = cap.node;
                CaptureData {
                    index: cap.index as usize,
                    kind: node.kind().to_string(),
                    text: node.utf8_text(content.as_bytes()).unwrap_or("").to_string(),
                    start_row: node.start_position().row + 1,
                    start_col: node.start_position().column + 1,
                    end_row: node.end_position().row + 1,
                    end_col: node.end_position().column + 1,
                }
            })
            .collect();
        matches.push(MatchData { captures });
    }

    if json {
        let results: Vec<_> = matches
            .iter()
            .flat_map(|m| {
                m.captures.iter().map(|cap| {
                    serde_json::json!({
                        "capture": query.capture_names()[cap.index],
                        "kind": cap.kind,
                        "text": cap.text,
                        "start": {
                            "row": cap.start_row,
                            "column": cap.start_col
                        },
                        "end": {
                            "row": cap.end_row,
                            "column": cap.end_col
                        }
                    })
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&results).unwrap());
    } else {
        println!("{} matches:", matches.len());
        println!();

        for m in &matches {
            for cap in &m.captures {
                let capture_name = &query.capture_names()[cap.index];

                // Single line preview
                let preview = if cap.text.contains('\n') {
                    let first_line = cap.text.lines().next().unwrap_or("");
                    if first_line.len() > 60 {
                        format!("{}...", &first_line[..60])
                    } else {
                        format!("{}...", first_line)
                    }
                } else if cap.text.len() > 80 {
                    format!("{}...", &cap.text[..80])
                } else {
                    cap.text.clone()
                };

                println!(
                    "  @{}: {} [L{}:{}-L{}:{}]",
                    capture_name, cap.kind, cap.start_row, cap.start_col, cap.end_row, cap.end_col
                );
                println!("    {}", preview);

                if show_source && cap.text.contains('\n') {
                    println!("    ---");
                    for line in cap.text.lines() {
                        println!("    {}", line);
                    }
                    println!("    ---");
                }
                println!();
            }
        }
    }

    0
}
