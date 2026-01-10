//! Call graph analysis - show callers/callees of symbols

use crate::index;
use crate::path_resolve;
use std::path::Path;

/// Show callers/callees of a symbol
pub fn cmd_call_graph(
    root: &Path,
    target: &str,
    show_callers: bool,
    show_callees: bool,
    case_insensitive: bool,
    json: bool,
) -> i32 {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(cmd_call_graph_async(
        root,
        target,
        show_callers,
        show_callees,
        case_insensitive,
        json,
    ))
}

async fn cmd_call_graph_async(
    root: &Path,
    target: &str,
    show_callers: bool,
    show_callees: bool,
    _case_insensitive: bool, // Index methods already have case-insensitive fallbacks
    json: bool,
) -> i32 {
    // Try to parse target as file:symbol or just symbol
    let (symbol, file_hint) = if let Some((sym, file)) = parse_file_symbol_string(target) {
        (sym, Some(file))
    } else {
        (target.to_string(), None)
    };

    // Try index first
    let idx = match index::FileIndex::open_if_enabled(root).await {
        Some(i) => i,
        None => {
            eprintln!("Indexing disabled or failed. Run: moss index rebuild --call-graph");
            return 1;
        }
    };

    let stats = idx.call_graph_stats().await.unwrap_or_default();
    if stats.calls == 0 {
        eprintln!("Call graph not indexed. Run: moss reindex --call-graph");
        return 1;
    }

    let mut results: Vec<(String, String, usize, &str)> = Vec::new(); // (file, symbol, line, direction)

    // Get callers if requested
    if show_callers {
        match idx.find_callers(&symbol).await {
            Ok(callers) => {
                for (file, sym, line) in callers {
                    results.push((file, sym, line, "caller"));
                }
            }
            Err(e) => {
                eprintln!("Error finding callers: {}", e);
            }
        }
    }

    // Get callees if requested
    if show_callees {
        // Need to find file for symbol first
        let file_path = if let Some(f) = &file_hint {
            let matches = path_resolve::resolve(f, root);
            matches
                .iter()
                .find(|m| m.kind == "file")
                .map(|m| m.path.clone())
        } else {
            idx.find_symbol(&symbol)
                .await
                .ok()
                .and_then(|syms| syms.first().map(|(f, _, _, _)| f.clone()))
        };

        if let Some(file_path) = file_path {
            match idx.find_callees(&file_path, &symbol).await {
                Ok(callees) => {
                    for (name, line) in callees {
                        results.push((file_path.clone(), name, line, "callee"));
                    }
                }
                Err(e) => {
                    eprintln!("Error finding callees: {}", e);
                }
            }
        }
    }

    if results.is_empty() {
        if json {
            println!("[]");
        } else {
            let direction = if show_callers && show_callees {
                "callers or callees"
            } else if show_callers {
                "callers"
            } else {
                "callees"
            };
            eprintln!("No {} found for: {}", direction, symbol);
        }
        return 1;
    }

    // Sort by file, then line
    results.sort_by(|a, b| (&a.0, a.2).cmp(&(&b.0, b.2)));

    if json {
        let output: Vec<_> = results
            .iter()
            .map(|(file, sym, line, direction)| {
                serde_json::json!({
                    "file": file,
                    "symbol": sym,
                    "line": line,
                    "direction": direction
                })
            })
            .collect();
        println!("{}", serde_json::to_string(&output).unwrap());
    } else {
        let header = if show_callers && show_callees {
            format!("Callers and callees of {}", symbol)
        } else if show_callers {
            format!("Callers of {}", symbol)
        } else {
            format!("Callees of {}", symbol)
        };
        println!("{}:", header);
        for (file, sym, line, _direction) in &results {
            println!("  {}:{}:{}", file, line, sym);
        }
    }

    0
}

/// Try various separators to parse file:symbol format
fn parse_file_symbol_string(s: &str) -> Option<(String, String)> {
    // Try various separators: #, ::, :
    for sep in ["#", "::", ":"] {
        if let Some(idx) = s.find(sep) {
            let (file, rest) = s.split_at(idx);
            let symbol = &rest[sep.len()..];
            if !file.is_empty() && !symbol.is_empty() && looks_like_file(file) {
                return Some((symbol.to_string(), file.to_string()));
            }
        }
    }
    None
}

/// Check if a string looks like a file path
fn looks_like_file(s: &str) -> bool {
    s.contains('.') || s.contains('/')
}
