//! Line range viewing for view command.

use crate::{parsers, path_resolve, tree};
use rhizome_moss_languages::support_for_path;
use std::collections::HashSet;
use std::path::Path;

/// Parse a line target like "file.rs:30" or "file.rs:30-55".
/// Returns (file_path, start, end) where end is None for single line.
pub fn parse_line_target(target: &str) -> Option<(String, usize, Option<usize>)> {
    let colon_pos = target.rfind(':')?;
    let (path, range) = target.split_at(colon_pos);
    let range = &range[1..];

    if let Some((start_str, end_str)) = range.split_once('-') {
        let start: usize = start_str.parse().ok()?;
        let end: usize = end_str.parse().ok()?;
        if start == 0 || end == 0 || start > end {
            return None;
        }
        return Some((path.to_string(), start, Some(end)));
    }

    let line: usize = range.parse().ok()?;
    if line == 0 {
        return None;
    }
    Some((path.to_string(), line, None))
}

/// View a range of lines from a file.
#[allow(clippy::too_many_arguments)]
pub fn cmd_view_line_range(
    file_path: &str,
    start: usize,
    end: usize,
    root: &Path,
    show_docs: bool,
    json: bool,
    pretty: bool,
    use_colors: bool,
) -> i32 {
    let matches = path_resolve::resolve_unified_all(file_path, root);
    let resolved = match matches.len() {
        0 => {
            eprintln!("File not found: {}", file_path);
            return 1;
        }
        1 => &matches[0],
        _ => {
            eprintln!("Multiple matches for '{}' - be more specific:", file_path);
            for m in &matches {
                println!("  {}", m.file_path);
            }
            return 1;
        }
    };

    if resolved.is_directory {
        eprintln!("Cannot use line range with directory: {}", file_path);
        return 1;
    }

    let full_path = root.join(&resolved.file_path);
    let display_path = &resolved.file_path;

    let content = match std::fs::read_to_string(&full_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to read file: {}", e);
            return 1;
        }
    };

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    let actual_start = start.min(total_lines);
    let actual_end = end.min(total_lines);

    if actual_start > total_lines {
        eprintln!("Line {} is past end of file ({} lines)", start, total_lines);
        return 1;
    }

    let range_start = actual_start.saturating_sub(1);
    let range_end = actual_end.min(lines.len());

    let grammar = support_for_path(&full_path).map(|s| s.grammar_name().to_string());
    let source: String = if !show_docs {
        if let Some(ref g) = grammar {
            let doc_lines = find_doc_comment_lines(&content, g, actual_start, actual_end);
            lines[range_start..range_end]
                .iter()
                .enumerate()
                .filter(|(i, _)| !doc_lines.contains(&(actual_start + i)))
                .map(|(_, line)| *line)
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            lines[range_start..range_end].join("\n")
        }
    } else {
        lines[range_start..range_end].join("\n")
    };

    if json {
        println!(
            "{}",
            serde_json::json!({
                "file": display_path,
                "start": actual_start,
                "end": actual_end,
                "content": source
            })
        );
        return 0;
    }

    println!("# {}:{}-{}", display_path, actual_start, actual_end);
    println!();

    let output = if pretty {
        if let Some(ref g) = grammar {
            tree::highlight_source(&source, g, use_colors)
        } else {
            source
        }
    } else {
        source
    };
    print!("{}", output);
    if !output.ends_with('\n') {
        println!();
    }

    0
}

/// Find line numbers that contain doc comments within a range.
fn find_doc_comment_lines(
    content: &str,
    grammar: &str,
    start_line: usize,
    end_line: usize,
) -> HashSet<usize> {
    let mut doc_lines = HashSet::new();

    if let Some(tree) = parsers::parse_with_grammar(grammar, content) {
        let mut cursor = tree.walk();
        collect_doc_comment_lines(&mut cursor, start_line, end_line, &mut doc_lines);
    }

    doc_lines
}

/// Recursively collect doc comment line numbers.
fn collect_doc_comment_lines(
    cursor: &mut tree_sitter::TreeCursor,
    start_line: usize,
    end_line: usize,
    doc_lines: &mut HashSet<usize>,
) {
    loop {
        let node = cursor.node();
        let kind = node.kind();

        let is_comment = kind == "comment"
            || kind == "line_comment"
            || kind == "block_comment"
            || kind == "doc_comment"
            || kind.contains("comment");

        if is_comment {
            let node_start = node.start_position().row + 1;
            let end_pos = node.end_position();
            let node_end = if end_pos.column == 0 {
                end_pos.row
            } else {
                end_pos.row + 1
            };

            for line in node_start..=node_end {
                if line >= start_line && line <= end_line {
                    doc_lines.insert(line);
                }
            }
        }

        if cursor.goto_first_child() {
            collect_doc_comment_lines(cursor, start_line, end_line, doc_lines);
            cursor.goto_parent();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
}
