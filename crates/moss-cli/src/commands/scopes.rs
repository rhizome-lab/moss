//! Scopes command - analyze scopes and bindings in a file.

use crate::{path_resolve, scopes};
use std::path::Path;

/// Analyze scopes and bindings in a file
pub fn cmd_scopes(
    file: &str,
    root: Option<&Path>,
    line: Option<usize>,
    find: Option<&str>,
    json: bool,
) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // Resolve the file
    let matches = path_resolve::resolve(file, &root);
    let file_match = match matches.iter().find(|m| m.kind == "file") {
        Some(m) => m,
        None => {
            eprintln!("File not found: {}", file);
            return 1;
        }
    };

    let file_path = root.join(&file_match.path);
    let content = match std::fs::read_to_string(&file_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file: {}", e);
            return 1;
        }
    };

    let analyzer = scopes::ScopeAnalyzer::new();
    let result = analyzer.analyze(&file_path, &content);

    // Find mode: find where a name is defined at a line
    if let (Some(name), Some(ln)) = (find, line) {
        if let Some(binding) = result.find_definition(name, ln) {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "name": binding.name,
                        "kind": binding.kind.as_str(),
                        "line": binding.line,
                        "column": binding.column,
                        "inferred_type": binding.inferred_type
                    })
                );
            } else {
                let type_str = binding
                    .inferred_type
                    .as_ref()
                    .map(|t| format!(" (type: {})", t))
                    .unwrap_or_default();
                println!(
                    "{} {} defined at line {} column {}{}",
                    binding.kind.as_str(),
                    binding.name,
                    binding.line,
                    binding.column,
                    type_str
                );
            }
        } else {
            eprintln!("'{}' not found in scope at line {}", name, ln);
            return 1;
        }
        return 0;
    }

    // Line mode: show all bindings visible at a line
    if let Some(ln) = line {
        let bindings = result.bindings_at_line(ln);
        if json {
            let output: Vec<_> = bindings
                .iter()
                .map(|b| {
                    serde_json::json!({
                        "name": b.name,
                        "kind": b.kind.as_str(),
                        "line": b.line,
                        "column": b.column,
                        "inferred_type": b.inferred_type
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        } else {
            println!("# Bindings visible at line {} in {}", ln, file_match.path);
            if bindings.is_empty() {
                println!("  (none)");
            } else {
                for b in &bindings {
                    let type_str = b
                        .inferred_type
                        .as_ref()
                        .map(|t| format!(": {}", t))
                        .unwrap_or_default();
                    println!(
                        "  {} {}{} (defined line {})",
                        b.kind.as_str(),
                        b.name,
                        type_str,
                        b.line
                    );
                }
            }
        }
        return 0;
    }

    // Default: show full scope tree
    if json {
        fn scope_to_json(scope: &scopes::Scope) -> serde_json::Value {
            serde_json::json!({
                "kind": scope.kind.as_str(),
                "name": scope.name,
                "start_line": scope.start_line,
                "end_line": scope.end_line,
                "bindings": scope.bindings.iter().map(|b| {
                    serde_json::json!({
                        "name": b.name,
                        "kind": b.kind.as_str(),
                        "line": b.line,
                        "column": b.column,
                        "inferred_type": b.inferred_type
                    })
                }).collect::<Vec<_>>(),
                "children": scope.children.iter().map(scope_to_json).collect::<Vec<_>>()
            })
        }
        println!("{}", serde_json::to_string_pretty(&scope_to_json(&result.root)).unwrap());
    } else {
        println!("{}", result.format());
    }

    0
}
