//! Summarize command - generate module summary.

use crate::{path_resolve, summarize};
use std::path::Path;

/// Generate a summary of a module
pub fn cmd_summarize(file: &str, root: Option<&Path>, json: bool) -> i32 {
    let resolved = match path_resolve::resolve_and_read(file, root) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}", e);
            return 1;
        }
    };

    let summary = summarize::summarize_module(&resolved.abs_path, &resolved.content);

    if json {
        let exports: Vec<_> = summary
            .main_exports
            .iter()
            .map(|e| {
                serde_json::json!({
                    "name": e.name,
                    "kind": e.kind,
                    "signature": e.signature,
                    "docstring": e.docstring
                })
            })
            .collect();

        println!(
            "{}",
            serde_json::json!({
                "file": resolved.rel_path,
                "module_name": summary.module_name,
                "purpose": summary.purpose,
                "exports": exports,
                "dependencies": summary.dependencies,
                "line_count": summary.line_count
            })
        );
    } else {
        println!("{}", summary.format());
    }

    0
}
