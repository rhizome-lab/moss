//! Imports command - query imports across the codebase.

use crate::{deps, index, path_resolve, symbols};
use moss_languages::support_for_path;
use std::path::Path;

/// Query imports across the codebase
pub fn cmd_imports(
    query: &str,
    root: Option<&Path>,
    resolve: bool,
    graph: bool,
    who_imports: bool,
    json: bool,
) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // Try index first, but fall back to direct parsing if not available
    let idx = index::FileIndex::open(&root).ok();
    let import_count = idx
        .as_ref()
        .and_then(|i| i.call_graph_stats().ok())
        .map(|(_, _, imports)| imports)
        .unwrap_or(0);

    // --who_imports: find files that import a given module
    if who_imports {
        if import_count == 0 {
            eprintln!("Import tracking requires indexed call graph. Run: moss reindex --call-graph");
            return 1;
        }
        let idx = idx.unwrap();
        match idx.find_importers(query) {
            Ok(importers) => {
                if json {
                    let output: Vec<_> = importers
                        .iter()
                        .map(|(file, name, line)| {
                            serde_json::json!({
                                "file": file,
                                "name": name,
                                "line": line
                            })
                        })
                        .collect();
                    println!("{}", serde_json::to_string_pretty(&output).unwrap());
                } else if importers.is_empty() {
                    println!("No files import '{}'", query);
                } else {
                    println!("# Files importing '{}'", query);
                    for (file, name, line) in &importers {
                        if name == "*" {
                            println!("  {} (line {}, wildcard)", file, line);
                        } else {
                            println!("  {} (line {}, imports {})", file, line, name);
                        }
                    }
                }
                return 0;
            }
            Err(e) => {
                eprintln!("Error finding importers: {}", e);
                return 1;
            }
        }
    }

    // --graph: show what file imports and what imports it
    if graph {
        if import_count == 0 {
            eprintln!("Import graph requires indexed call graph. Run: moss reindex --call-graph");
            return 1;
        }
        let idx = idx.unwrap();

        // Resolve file path
        let matches = path_resolve::resolve(query, &root);
        let file_path = match matches.iter().find(|m| m.kind == "file") {
            Some(m) => &m.path,
            None => {
                eprintln!("File not found: {}", query);
                return 1;
            }
        };

        // Get what this file imports
        let imports = idx.get_imports(file_path).unwrap_or_default();

        // Get what imports this file (convert file path to module name)
        let module_name = file_path_to_module(file_path);
        let importers = if let Some(ref module) = module_name {
            idx.find_importers(module).unwrap_or_default()
        } else {
            Vec::new()
        };

        if json {
            let import_output: Vec<_> = imports
                .iter()
                .map(|i| {
                    serde_json::json!({
                        "module": i.module,
                        "name": i.name,
                        "alias": i.alias,
                        "line": i.line
                    })
                })
                .collect();
            let importer_output: Vec<_> = importers
                .iter()
                .map(|(file, name, line)| {
                    serde_json::json!({
                        "file": file,
                        "name": name,
                        "line": line
                    })
                })
                .collect();
            println!(
                "{}",
                serde_json::json!({
                    "file": file_path,
                    "module": module_name,
                    "imports": import_output,
                    "imported_by": importer_output
                })
            );
        } else {
            println!("# Import graph for {}", file_path);
            if let Some(ref m) = module_name {
                println!("# Module: {}", m);
            }
            println!();

            println!("## Imports ({}):", imports.len());
            if imports.is_empty() {
                println!("  (none)");
            } else {
                for imp in &imports {
                    let alias = imp
                        .alias
                        .as_ref()
                        .map(|a| format!(" as {}", a))
                        .unwrap_or_default();
                    if let Some(module) = &imp.module {
                        println!("  from {} import {}{}", module, imp.name, alias);
                    } else {
                        println!("  import {}{}", imp.name, alias);
                    }
                }
            }
            println!();

            println!("## Imported by ({}):", importers.len());
            if importers.is_empty() {
                println!("  (none)");
            } else {
                for (file, name, line) in &importers {
                    if name == "*" {
                        println!("  {} (line {}, wildcard)", file, line);
                    } else {
                        println!("  {} (line {}, imports {})", file, line, name);
                    }
                }
            }
        }
        return 0;
    }

    // For resolve mode, we need the index - no direct fallback possible
    if resolve {
        if import_count == 0 {
            eprintln!(
                "Import resolution requires indexed call graph. Run: moss reindex --call-graph"
            );
            return 1;
        }
        let idx = idx.unwrap();
        // Query format: "file:name" - resolve what module a name comes from
        let (file, name) = if let Some(idx) = query.find(':') {
            (&query[..idx], &query[idx + 1..])
        } else {
            eprintln!("Resolve format: file:name (e.g., cli.py:serialize)");
            return 1;
        };

        // Resolve the file first
        let matches = path_resolve::resolve(file, &root);
        let file_path = match matches.iter().find(|m| m.kind == "file") {
            Some(m) => &m.path,
            None => {
                eprintln!("File not found: {}", file);
                return 1;
            }
        };

        match idx.resolve_import(file_path, name) {
            Ok(Some((module, orig_name))) => {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "name": name,
                            "module": module,
                            "original_name": orig_name
                        })
                    );
                } else {
                    if name == orig_name {
                        println!("{} <- {}", name, module);
                    } else {
                        println!("{} <- {}.{}", name, module, orig_name);
                    }
                }
                0
            }
            Ok(None) => {
                if json {
                    println!("{}", serde_json::json!({"name": name, "module": null}));
                } else {
                    eprintln!("Name '{}' not found in imports of {}", name, file_path);
                }
                1
            }
            Err(e) => {
                eprintln!("Error resolving import: {}", e);
                1
            }
        }
    } else {
        // Show all imports for a file
        let matches = path_resolve::resolve(query, &root);
        let file_match = match matches.iter().find(|m| m.kind == "file") {
            Some(m) => m,
            None => {
                eprintln!("File not found: {}", query);
                return 1;
            }
        };
        let file_path = &file_match.path;

        // Try index first, fall back to direct parsing
        if import_count > 0 {
            if let Some(ref idx) = idx {
                match idx.get_imports(file_path) {
                    Ok(imports) => {
                        return output_imports(&imports, file_path, json);
                    }
                    Err(_) => {
                        // Fall through to direct parsing
                    }
                }
            }
        }

        // Direct parsing fallback
        let full_path = root.join(file_path);
        let content = match std::fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Error reading file: {}", e);
                return 1;
            }
        };

        let extractor = deps::DepsExtractor::new();
        let result = extractor.extract(&full_path, &content);

        // Convert deps::Import to symbols::Import format for output
        let imports: Vec<symbols::Import> = result
            .imports
            .iter()
            .flat_map(|imp| {
                if imp.names.is_empty() {
                    // "import x" or "import x as y"
                    vec![symbols::Import {
                        module: None,
                        name: imp.module.clone(),
                        alias: imp.alias.clone(),
                        line: imp.line,
                    }]
                } else {
                    // "from x import a, b, c"
                    imp.names
                        .iter()
                        .map(|name| symbols::Import {
                            module: Some(imp.module.clone()),
                            name: name.clone(),
                            alias: None,
                            line: imp.line,
                        })
                        .collect()
                }
            })
            .collect();

        output_imports(&imports, file_path, json)
    }
}

/// Convert a file path to a module name using language-specific rules
fn file_path_to_module(file_path: &str) -> Option<String> {
    let path = Path::new(file_path);
    let lang = support_for_path(path)?;
    lang.file_path_to_module_name(path)
}

fn output_imports(imports: &[symbols::Import], file_path: &str, json: bool) -> i32 {
    if imports.is_empty() {
        if json {
            println!("[]");
        } else {
            println!("No imports found in {}", file_path);
        }
        return 0;
    }

    if json {
        let output: Vec<_> = imports
            .iter()
            .map(|i| {
                serde_json::json!({
                    "module": i.module,
                    "name": i.name,
                    "alias": i.alias,
                    "line": i.line
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!("# Imports in {}", file_path);
        for imp in imports {
            let alias = imp
                .alias
                .as_ref()
                .map(|a| format!(" as {}", a))
                .unwrap_or_default();
            if let Some(module) = &imp.module {
                println!("  from {} import {}{}", module, imp.name, alias);
            } else {
                println!("  import {}{}", imp.name, alias);
            }
        }
    }
    0
}
