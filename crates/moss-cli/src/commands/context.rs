//! Context command - show file skeleton with imports/exports.

use crate::{deps, path_resolve, skeleton};
use std::path::Path;

/// Show file context (skeleton + imports/exports)
pub fn cmd_context(file: &str, root: Option<&Path>, json: bool) -> i32 {
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

    let line_count = content.lines().count();

    // Extract skeleton
    let mut skeleton_extractor = skeleton::SkeletonExtractor::new();
    let skeleton_result = skeleton_extractor.extract(&file_path, &content);

    // Extract deps
    let deps_extractor = deps::DepsExtractor::new();
    let deps_result = deps_extractor.extract(&file_path, &content);

    // Count symbols recursively
    fn count_symbols(symbols: &[skeleton::SkeletonSymbol]) -> (usize, usize, usize) {
        let mut classes = 0;
        let mut functions = 0;
        let mut methods = 0;
        for s in symbols {
            match s.kind {
                "class" => classes += 1,
                "function" => functions += 1,
                "method" => methods += 1,
                _ => {}
            }
            let (c, f, m) = count_symbols(&s.children);
            classes += c;
            functions += f;
            methods += m;
        }
        (classes, functions, methods)
    }

    let (classes, functions, methods) = count_symbols(&skeleton_result.symbols);

    if json {
        fn symbol_to_json(sym: &skeleton::SkeletonSymbol) -> serde_json::Value {
            serde_json::json!({
                "name": sym.name,
                "kind": sym.kind,
                "signature": sym.signature,
                "docstring": sym.docstring,
                "start_line": sym.start_line,
                "end_line": sym.end_line,
                "children": sym.children.iter().map(symbol_to_json).collect::<Vec<_>>()
            })
        }

        let symbols: Vec<_> = skeleton_result.symbols.iter().map(symbol_to_json).collect();
        let imports: Vec<_> = deps_result
            .imports
            .iter()
            .map(|i| {
                serde_json::json!({
                    "module": i.module,
                    "names": i.names,
                    "line": i.line
                })
            })
            .collect();
        let exports: Vec<_> = deps_result
            .exports
            .iter()
            .map(|e| {
                serde_json::json!({
                    "name": e.name,
                    "type": e.kind,
                    "line": e.line
                })
            })
            .collect();

        println!(
            "{}",
            serde_json::json!({
                "file": file_match.path,
                "summary": {
                    "lines": line_count,
                    "classes": classes,
                    "functions": functions,
                    "methods": methods,
                    "imports": deps_result.imports.len(),
                    "exports": deps_result.exports.len()
                },
                "symbols": symbols,
                "imports": imports,
                "exports": exports
            })
        );
    } else {
        // Text output
        println!("# {}", file_match.path);
        println!("Lines: {}", line_count);
        println!(
            "Classes: {}, Functions: {}, Methods: {}",
            classes, functions, methods
        );
        println!(
            "Imports: {}, Exports: {}",
            deps_result.imports.len(),
            deps_result.exports.len()
        );
        println!();

        if !deps_result.imports.is_empty() {
            println!("## Imports");
            for imp in &deps_result.imports {
                if imp.names.is_empty() {
                    println!("import {}", imp.module);
                } else {
                    println!("from {} import {}", imp.module, imp.names.join(", "));
                }
            }
            println!();
        }

        let skeleton_text = skeleton_result.format(true);
        if !skeleton_text.is_empty() {
            println!("## Skeleton");
            println!("{}", skeleton_text);
        }
    }

    0
}
