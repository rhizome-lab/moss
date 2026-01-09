//! Debug utility to print the full tree-sitter AST for a code snippet.
//!
//! # Usage
//!
//! ```bash
//! cargo run --bin debug_ast -- <grammar> "<code>"
//! ```
//!
//! # Examples
//!
//! ```bash
//! # Parse Rust code
//! cargo run --bin debug_ast -- rust 'pub fn foo() {}'
//!
//! # Parse doc comment
//! cargo run --bin debug_ast -- rust $'/// Doc comment\npub fn foo() {}'
//!
//! # Parse Python
//! cargo run --bin debug_ast -- python 'def foo(): pass'
//!
//! # Parse TypeScript
//! cargo run --bin debug_ast -- typescript 'const x: number = 42'
//! ```
//!
//! # Output
//!
//! Prints each node with indentation showing hierarchy:
//! - Node kind (e.g., `function_item`, `identifier`)
//! - Byte range `[start..end]`
//! - Text preview (first 40 chars)
//!
//! Useful for understanding tree-sitter node kinds when implementing
//! syntax highlighting or AST-based analysis.

use rhizome_moss::parsers;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: debug_ast <grammar> <code>");
        eprintln!();
        eprintln!("Examples:");
        eprintln!("  debug_ast rust 'pub fn foo() {{}}'");
        eprintln!("  debug_ast python 'def foo(): pass'");
        eprintln!("  debug_ast typescript 'const x: number = 42'");
        std::process::exit(1);
    }

    let grammar = &args[1];
    let code = &args[2];

    match parsers::parse_with_grammar(grammar, code) {
        Some(tree) => {
            println!("Code: {:?}\n", code);
            print_tree(tree.root_node(), code.as_bytes(), 0);
        }
        None => {
            eprintln!("Failed to parse with grammar: {}", grammar);
            std::process::exit(1);
        }
    }
}

fn print_tree(node: tree_sitter::Node, source: &[u8], indent: usize) {
    let text = std::str::from_utf8(&source[node.start_byte()..node.end_byte()]).unwrap_or("");
    let is_leaf = node.child_count() == 0;

    // For leaf nodes, show full text (it's typically short)
    // For non-leaf nodes, show truncated preview
    let preview = if is_leaf {
        text.replace('\n', "\\n")
    } else if text.len() > 40 {
        format!("{}...", &text[..40].replace('\n', "\\n"))
    } else {
        text.replace('\n', "\\n")
    };

    // Mark leaf nodes with * for easy identification
    let marker = if is_leaf { "*" } else { "" };
    println!(
        "{}{}{} [{}..{}] {:?}",
        "  ".repeat(indent),
        node.kind(),
        marker,
        node.start_byte(),
        node.end_byte(),
        preview
    );

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        print_tree(child, source, indent + 1);
    }
}
