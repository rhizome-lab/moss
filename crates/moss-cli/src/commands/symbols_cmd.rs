//! Symbols command - list symbols in a file.

use crate::{path_resolve, symbols};
use std::path::Path;

/// List symbols in a file
pub fn cmd_symbols(file: &str, root: Option<&Path>, json: bool) -> i32 {
    let resolved = match path_resolve::resolve_and_read(file, root) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}", e);
            return 1;
        }
    };

    let parser = symbols::SymbolParser::new();
    let symbols = parser.parse_file(&resolved.abs_path, &resolved.content);

    if json {
        let output: Vec<_> = symbols
            .iter()
            .map(|s| {
                serde_json::json!({
                    "name": s.name,
                    "kind": s.kind.as_str(),
                    "start_line": s.start_line,
                    "end_line": s.end_line,
                    "parent": s.parent
                })
            })
            .collect();
        println!("{}", serde_json::to_string(&output).unwrap());
    } else {
        for s in &symbols {
            let parent_str = s
                .parent
                .as_ref()
                .map(|p| format!(" (in {})", p))
                .unwrap_or_default();
            println!(
                "{}:{}-{} {} {}{}",
                resolved.rel_path,
                s.start_line,
                s.end_line,
                s.kind.as_str(),
                s.name,
                parent_str
            );
        }
    }

    0
}
