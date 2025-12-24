//! Search tree command - search codebase for files matching a query.

use crate::path_resolve;
use std::path::Path;

/// Search the codebase tree for files matching a query
pub fn cmd_search_tree(query: &str, root: Option<&Path>, limit: usize, json: bool) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // Use fuzzy matching to find all matches
    let matches = path_resolve::resolve(query, &root);
    let total = matches.len();

    // For extension patterns, use higher limit unless explicitly set
    let effective_limit = if query.starts_with('.') && limit == 20 {
        500 // Default higher limit for extension searches
    } else {
        limit
    };

    let limited: Vec<_> = matches.into_iter().take(effective_limit).collect();

    if json {
        let output: Vec<_> = limited
            .iter()
            .map(|m| serde_json::json!({"path": m.path, "kind": m.kind, "score": m.score}))
            .collect();
        println!("{}", serde_json::to_string(&output).unwrap());
    } else {
        for m in &limited {
            println!("{} ({})", m.path, m.kind);
        }
        if total > effective_limit {
            println!("... +{} more (use -l to show more)", total - effective_limit);
        }
    }

    0
}
