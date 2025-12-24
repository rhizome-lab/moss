//! Analysis commands for moss CLI (without Profiler dependency).

use crate::{anchors, cfg, complexity, path_resolve, scopes};
use std::path::Path;

/// Show anchors (navigation points) in a file
pub fn cmd_anchors(file: &str, root: Option<&Path>, query: Option<&str>, json: bool) -> i32 {
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

    let extractor = anchors::AnchorExtractor::new();

    let anchors_list = if let Some(q) = query {
        extractor.find_anchor(&file_path, &content, q)
    } else {
        extractor.extract(&file_path, &content).anchors
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
                "file": file_match.path,
                "anchors": output
            })
        );
    } else {
        if anchors_list.is_empty() {
            println!("# {} (no anchors)", file_match.path);
        } else {
            println!("# {} ({} anchors)", file_match.path, anchors_list.len());
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

/// Analyze cyclomatic complexity of functions in a file
pub fn cmd_complexity(file: &str, root: Option<&Path>, threshold: Option<usize>, json: bool) -> i32 {
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

    let analyzer = complexity::ComplexityAnalyzer::new();
    let report = analyzer.analyze(&file_path, &content);

    // Filter by threshold if specified
    let functions: Vec<_> = if let Some(t) = threshold {
        report
            .functions
            .into_iter()
            .filter(|f| f.complexity >= t)
            .collect()
    } else {
        report.functions
    };

    if json {
        let output: Vec<_> = functions
            .iter()
            .map(|f| {
                serde_json::json!({
                    "name": f.name,
                    "qualified_name": f.qualified_name(),
                    "complexity": f.complexity,
                    "risk_level": f.risk_level(),
                    "start_line": f.start_line,
                    "end_line": f.end_line,
                    "parent": f.parent
                })
            })
            .collect();

        let avg: f64 = if functions.is_empty() {
            0.0
        } else {
            functions.iter().map(|f| f.complexity).sum::<usize>() as f64 / functions.len() as f64
        };
        let max = functions.iter().map(|f| f.complexity).max().unwrap_or(0);
        let high_risk = functions.iter().filter(|f| f.complexity > 10).count();

        println!(
            "{}",
            serde_json::json!({
                "file": file_match.path,
                "function_count": functions.len(),
                "avg_complexity": (avg * 10.0).round() / 10.0,
                "max_complexity": max,
                "high_risk_count": high_risk,
                "functions": output
            })
        );
    } else {
        println!("# {} - Complexity Analysis", file_match.path);

        if functions.is_empty() {
            println!(
                "\nNo functions found{}",
                threshold
                    .map(|t| format!(" above threshold {}", t))
                    .unwrap_or_default()
            );
        } else {
            let avg = functions.iter().map(|f| f.complexity).sum::<usize>() as f64
                / functions.len() as f64;
            let max = functions.iter().map(|f| f.complexity).max().unwrap_or(0);
            let high_risk = functions.iter().filter(|f| f.complexity > 10).count();

            println!("\n## Summary");
            println!("  Functions: {}", functions.len());
            println!("  Average complexity: {:.1}", avg);
            println!("  Maximum complexity: {}", max);
            println!("  High risk (>10): {}", high_risk);

            // Sort by complexity descending
            let mut sorted = functions;
            sorted.sort_by(|a, b| b.complexity.cmp(&a.complexity));

            println!("\n## Functions (by complexity)");
            for f in &sorted {
                let parent = f
                    .parent
                    .as_ref()
                    .map(|p| format!("{}.", p))
                    .unwrap_or_default();
                println!(
                    "  {:3} [{}] {}{} (lines {}-{})",
                    f.complexity,
                    f.risk_level(),
                    parent,
                    f.name,
                    f.start_line,
                    f.end_line
                );
            }
        }
    }

    0
}

/// Build and display control flow graphs for functions
pub fn cmd_cfg(file: &str, root: Option<&Path>, function: Option<&str>, json: bool) -> i32 {
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

    let mut builder = cfg::CfgBuilder::new();
    let result = builder.build(&file_path, &content, function);

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
                "file": file_match.path,
                "graphs": output
            })
        );
    } else {
        println!("# {} - Control Flow Graphs\n", file_match.path);

        for graph in &result.graphs {
            println!("{}\n", graph.format_text());
        }
    }

    0
}
