//! AST inspection for syntax rule authoring.

use crate::parsers::grammar_loader;
use rhizome_moss_languages::support_for_path;
use std::path::Path;

/// Show AST for a file.
pub fn cmd_ast(file: &Path, at_line: Option<usize>, sexp: bool, json: bool) -> i32 {
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

    // Parse
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&grammar).unwrap();
    let tree = match parser.parse(&content, None) {
        Some(t) => t,
        None => {
            eprintln!("Failed to parse file");
            return 1;
        }
    };

    let root = tree.root_node();

    if let Some(line) = at_line {
        // Show node at specific line
        print_node_at_line(&content, root, line, 0);
    } else if sexp {
        // Output as S-expression
        println!("{}", root.to_sexp());
    } else if json {
        // JSON output
        let ast = node_to_json(root, &content);
        println!("{}", serde_json::to_string_pretty(&ast).unwrap());
    } else {
        // Tree format (default)
        print_tree(&content, root, 0);
    }

    0
}

fn print_tree(source: &str, node: tree_sitter::Node, indent: usize) {
    let prefix = "  ".repeat(indent);
    let kind = node.kind();
    let start = node.start_position();
    let end = node.end_position();

    // Get field name if this node is a named field
    let field_info = if let Some(parent) = node.parent() {
        let mut cursor = parent.walk();
        let mut result = String::new();
        for (i, child) in parent.children(&mut cursor).enumerate() {
            if child.id() == node.id() {
                if let Some(field) = parent.field_name_for_child(i as u32) {
                    result = format!("{}: ", field);
                }
                break;
            }
        }
        result
    } else {
        String::new()
    };

    // For leaf nodes, show the text
    if node.child_count() == 0 {
        let text = node.utf8_text(source.as_bytes()).unwrap_or("");
        let text_preview = if text.len() > 40 {
            format!("{}...", &text[..40])
        } else {
            text.to_string()
        };
        println!(
            "{}{}({}) {:?} [L{}:{}]",
            prefix,
            field_info,
            kind,
            text_preview,
            start.row + 1,
            start.column + 1
        );
    } else {
        println!(
            "{}{}({}) [L{}-L{}]",
            prefix,
            field_info,
            kind,
            start.row + 1,
            end.row + 1
        );
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            print_tree(source, child, indent + 1);
        }
    }
}

fn print_node_at_line(source: &str, root: tree_sitter::Node, line: usize, _indent: usize) {
    // Find the deepest node containing this line
    let target_row = line.saturating_sub(1); // Convert to 0-indexed

    fn find_deepest_at_line<'a>(
        node: tree_sitter::Node<'a>,
        row: usize,
    ) -> Option<tree_sitter::Node<'a>> {
        if node.start_position().row <= row && node.end_position().row >= row {
            // Check children for a more specific match
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if let Some(deeper) = find_deepest_at_line(child, row) {
                    return Some(deeper);
                }
            }
            Some(node)
        } else {
            None
        }
    }

    if let Some(node) = find_deepest_at_line(root, target_row) {
        println!("Line {} is inside:", line);
        println!();

        // Walk up the tree showing context
        let mut current = Some(node);
        let mut depth = 0;
        let mut ancestors = Vec::new();

        while let Some(n) = current {
            ancestors.push(n);
            current = n.parent();
        }

        ancestors.reverse();

        for ancestor in &ancestors {
            let prefix = "  ".repeat(depth);
            let kind = ancestor.kind();
            let start = ancestor.start_position();
            let end = ancestor.end_position();

            if ancestor.child_count() == 0 {
                let text = ancestor.utf8_text(source.as_bytes()).unwrap_or("");
                println!(
                    "{}{} {:?} (L{}:{}-L{}:{})",
                    prefix,
                    kind,
                    text,
                    start.row + 1,
                    start.column + 1,
                    end.row + 1,
                    end.column + 1
                );
            } else {
                println!(
                    "{}{} (L{}:{}-L{}:{})",
                    prefix,
                    kind,
                    start.row + 1,
                    start.column + 1,
                    end.row + 1,
                    end.column + 1
                );
            }
            depth += 1;
        }
    } else {
        eprintln!("No node found at line {}", line);
    }
}

fn node_to_json(node: tree_sitter::Node, source: &str) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "kind": node.kind(),
        "start": {
            "row": node.start_position().row + 1,
            "column": node.start_position().column + 1
        },
        "end": {
            "row": node.end_position().row + 1,
            "column": node.end_position().column + 1
        }
    });

    if node.child_count() == 0 {
        obj["text"] = serde_json::json!(node.utf8_text(source.as_bytes()).unwrap_or(""));
    } else {
        let mut children = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            children.push(node_to_json(child, source));
        }
        obj["children"] = serde_json::json!(children);
    }

    obj
}
