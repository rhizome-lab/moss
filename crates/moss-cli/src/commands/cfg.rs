//! CFG command - build and display control flow graphs.

use crate::{cfg, path_resolve};
use std::path::Path;

/// Build and display control flow graphs for functions
pub fn cmd_cfg(file: &str, root: Option<&Path>, function: Option<&str>, json: bool) -> i32 {
    let resolved = match path_resolve::resolve_and_read(file, root) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}", e);
            return 1;
        }
    };

    let mut builder = cfg::CfgBuilder::new();
    let result = builder.build(&resolved.abs_path, &resolved.content, function);

    if result.graphs.is_empty() {
        if let Some(func_name) = function {
            eprintln!("No function '{}' found in {}", func_name, file);
        } else {
            eprintln!("No functions found in {}", file);
        }
        return 1;
    }

    if json {
        let output: Vec<_> = result
            .graphs
            .iter()
            .map(|g| {
                let nodes: Vec<_> = g
                    .nodes
                    .iter()
                    .map(|n| {
                        serde_json::json!({
                            "id": n.id,
                            "type": n.node_type.as_str(),
                            "statement": n.statement,
                            "line": n.start_line
                        })
                    })
                    .collect();

                let edges: Vec<_> = g
                    .edges
                    .iter()
                    .map(|e| {
                        serde_json::json!({
                            "source": e.source,
                            "target": e.target,
                            "type": e.edge_type.as_str()
                        })
                    })
                    .collect();

                serde_json::json!({
                    "name": g.name,
                    "start_line": g.start_line,
                    "end_line": g.end_line,
                    "node_count": g.nodes.len(),
                    "edge_count": g.edges.len(),
                    "complexity": g.cyclomatic_complexity(),
                    "nodes": nodes,
                    "edges": edges
                })
            })
            .collect();

        println!(
            "{}",
            serde_json::json!({
                "file": resolved.rel_path,
                "graphs": output
            })
        );
    } else {
        println!("# {} - Control Flow Graphs\n", resolved.rel_path);

        for graph in &result.graphs {
            println!("{}\n", graph.format_text());
        }
    }

    0
}
