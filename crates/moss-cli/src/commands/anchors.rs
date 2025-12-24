//! Anchors command - show navigation points in a file.

use crate::{anchors, path_resolve};
use std::path::Path;

/// Show anchors (navigation points) in a file
pub fn cmd_anchors(file: &str, root: Option<&Path>, query: Option<&str>, json: bool) -> i32 {
    let resolved = match path_resolve::resolve_and_read(file, root) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}", e);
            return 1;
        }
    };

    let extractor = anchors::AnchorExtractor::new();

    let anchors_list = if let Some(q) = query {
        extractor.find_anchor(&resolved.abs_path, &resolved.content, q)
    } else {
        extractor.extract(&resolved.abs_path, &resolved.content).anchors
    };

    if json {
        let output: Vec<_> = anchors_list
            .iter()
            .map(|a| {
                serde_json::json!({
                    "name": a.name,
                    "type": a.anchor_type.as_str(),
                    "reference": a.reference(),
                    "context": a.context,
                    "start_line": a.start_line,
                    "end_line": a.end_line,
                    "signature": a.signature
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({
                "file": resolved.rel_path,
                "anchors": output
            })
        );
    } else {
        if anchors_list.is_empty() {
            println!("# {} (no anchors)", resolved.rel_path);
        } else {
            println!("# {} ({} anchors)", resolved.rel_path, anchors_list.len());
            for a in &anchors_list {
                let ctx = if let Some(c) = &a.context {
                    format!(" (in {})", c)
                } else {
                    String::new()
                };
                println!(
                    "  {}:{}-{} {} {}{}",
                    a.anchor_type.as_str(),
                    a.start_line,
                    a.end_line,
                    a.name,
                    a.signature.as_deref().unwrap_or(""),
                    ctx
                );
            }
        }
    }

    0
}
