//! Callees command - find what a symbol calls.

use crate::{index, path_resolve};
use std::path::Path;

/// Find what a symbol calls (callees)
pub fn cmd_callees(symbol: &str, file: Option<&str>, root: Option<&Path>, json: bool) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // Try index first (fast path)
    if let Ok(idx) = index::FileIndex::open(&root) {
        let (_, calls, _) = idx.call_graph_stats().unwrap_or((0, 0, 0));
        if calls > 0 {
            // Determine file path
            let file_path = if let Some(file) = file {
                // Resolve provided file
                let matches = path_resolve::resolve(file, &root);
                matches
                    .iter()
                    .find(|m| m.kind == "file")
                    .map(|m| m.path.clone())
            } else {
                // Find file from symbol
                idx.find_symbol(symbol)
                    .ok()
                    .and_then(|syms| syms.first().map(|(f, _, _, _)| f.clone()))
            };

            if let Some(file_path) = file_path {
                if let Ok(callees) = idx.find_callees(&file_path, symbol) {
                    if !callees.is_empty() {
                        if json {
                            let output: Vec<_> = callees
                                .iter()
                                .map(|(name, line)| serde_json::json!({"name": name, "file": file_path, "line": line}))
                                .collect();
                            println!("{}", serde_json::to_string(&output).unwrap());
                        } else {
                            println!("Callees of {}:", symbol);
                            for (name, line) in &callees {
                                println!("  {}:{}:{}", file_path, line, name);
                            }
                        }
                        return 0;
                    }
                }
            }
            eprintln!(
                "No callees found for: {} (index has {} calls)",
                symbol, calls
            );
            return 1;
        }
    }

    // Fallback to parsing (slower) - also auto-indexes like callers
    eprintln!("Call graph not indexed. Building now (one-time)...");

    if let Ok(mut idx) = index::FileIndex::open(&root) {
        if idx.needs_refresh() {
            if let Err(e) = idx.incremental_refresh() {
                eprintln!("Failed to refresh file index: {}", e);
                return 1;
            }
        }
        match idx.incremental_call_graph_refresh() {
            Ok((symbols, calls, imports)) => {
                if symbols > 0 || calls > 0 || imports > 0 {
                    eprintln!(
                        "Indexed {} symbols, {} calls, {} imports",
                        symbols, calls, imports
                    );
                }

                // Retry with index
                let file_path = if let Some(file) = file {
                    let matches = path_resolve::resolve(file, &root);
                    matches
                        .iter()
                        .find(|m| m.kind == "file")
                        .map(|m| m.path.clone())
                } else {
                    idx.find_symbol(symbol)
                        .ok()
                        .and_then(|syms| syms.first().map(|(f, _, _, _)| f.clone()))
                };

                if let Some(file_path) = file_path {
                    if let Ok(callees) = idx.find_callees(&file_path, symbol) {
                        if !callees.is_empty() {
                            if json {
                                let output: Vec<_> = callees
                                    .iter()
                                    .map(|(name, line)| serde_json::json!({"name": name, "file": file_path, "line": line}))
                                    .collect();
                                println!("{}", serde_json::to_string(&output).unwrap());
                            } else {
                                println!("Callees of {}:", symbol);
                                for (name, line) in &callees {
                                    println!("  {}:{}:{}", file_path, line, name);
                                }
                            }
                            return 0;
                        }
                    }
                }
                eprintln!("No callees found for: {}", symbol);
                return 1;
            }
            Err(e) => {
                eprintln!("Failed to build call graph: {}", e);
                return 1;
            }
        }
    }

    eprintln!("Failed to open index");
    1
}
