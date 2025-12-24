//! Deps command - show imports and exports for a file.

use crate::{deps, path_resolve};
use std::path::Path;

/// Show imports and exports for a file
pub fn cmd_deps(
    file: &str,
    root: Option<&Path>,
    imports_only: bool,
    exports_only: bool,
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

    let extractor = deps::DepsExtractor::new();
    let result = extractor.extract(&file_path, &content);

    if json {
        let imports_json: Vec<_> = if !exports_only {
            result
                .imports
                .iter()
                .map(|i| {
                    serde_json::json!({
                        "module": i.module,
                        "names": i.names,
                        "alias": i.alias,
                        "line": i.line,
                        "is_relative": i.is_relative
                    })
                })
                .collect()
        } else {
            Vec::new()
        };

        let exports_json: Vec<_> = if !imports_only {
            result
                .exports
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "name": e.name,
                        "kind": e.kind,
                        "line": e.line
                    })
                })
                .collect()
        } else {
            Vec::new()
        };

        println!(
            "{}",
            serde_json::json!({
                "file": file_match.path,
                "imports": imports_json,
                "exports": exports_json
            })
        );
    } else {
        println!("# {}", file_match.path);

        if !exports_only && !result.imports.is_empty() {
            println!("\n## Imports ({}):", result.imports.len());
            for imp in &result.imports {
                let prefix = if imp.is_relative {
                    format!(".{}", imp.module)
                } else {
                    imp.module.clone()
                };

                if imp.names.is_empty() {
                    let alias = imp
                        .alias
                        .as_ref()
                        .map(|a| format!(" as {}", a))
                        .unwrap_or_default();
                    println!("  import {}{}", prefix, alias);
                } else {
                    println!("  from {} import {}", prefix, imp.names.join(", "));
                }
            }
        }

        if !imports_only && !result.exports.is_empty() {
            println!("\n## Exports ({}):", result.exports.len());
            for exp in &result.exports {
                println!("  {}: {}", exp.kind, exp.name);
            }
        }
    }

    0
}
