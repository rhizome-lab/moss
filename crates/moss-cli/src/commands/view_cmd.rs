//! View command - unified view of files, directories, and symbols.

use crate::{deps, index, path_resolve, skeleton, symbols, tree};
use moss_languages::support_for_path;
use std::path::{Path, PathBuf};

/// Check if a file has language support (symbols can be extracted)
fn has_language_support(path: &str) -> bool {
    support_for_path(Path::new(path))
        .map(|lang| lang.has_symbols())
        .unwrap_or(false)
}

/// Search for symbols in the index by name
fn search_symbols(query: &str, root: &Path) -> Vec<index::SymbolMatch> {
    let idx = match index::FileIndex::open(root) {
        Ok(i) => i,
        Err(_) => return vec![],
    };

    // Check if call graph is populated
    let stats = idx.call_graph_stats().unwrap_or_default();
    if stats.symbols == 0 {
        return vec![];
    }

    // Query symbols with fuzzy matching, limit to 10
    match idx.find_symbols(query, None, true, 10) {
        Ok(symbols) => symbols,
        Err(_) => vec![],
    }
}

/// View a symbol directly by file, name, and line
fn cmd_view_symbol_direct(
    file_path: &str,
    symbol_name: &str,
    _line: usize,
    root: &Path,
    depth: i32,
    full: bool,
    json: bool,
) -> i32 {
    cmd_view_symbol(file_path, &[symbol_name.to_string()], root, depth, false, full, json)
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

/// Unified view command
pub fn cmd_view(
    target: Option<&str>,
    root: Option<&Path>,
    depth: i32,
    line_numbers: bool,
    show_deps: bool,
    kind_filter: Option<&str>,
    show_calls: bool,
    show_called_by: bool,
    types_only: bool,
    raw: bool,
    focus: Option<&str>,
    resolve_imports: bool,
    show_all: bool,
    full: bool,
    json: bool,
) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // If kind filter is specified without target (or with "."), list matching symbols
    if kind_filter.is_some() {
        let scope = target.unwrap_or(".");
        return cmd_view_filtered(&root, scope, kind_filter.unwrap(), json);
    }

    // Handle --calls or --called-by with a target symbol
    if show_calls || show_called_by {
        let target = match target {
            Some(t) => t,
            None => {
                eprintln!("--calls and --called-by require a target symbol");
                return 1;
            }
        };
        // show_called_by → find callers (what calls this)
        // show_calls → find callees (what this calls)
        return cmd_view_calls(&root, target, show_called_by, show_calls, json);
    }

    // --focus requires a file target
    if focus.is_some() && target.is_none() {
        eprintln!("--focus requires a file target");
        return 1;
    }

    let target = target.unwrap_or(".");

    // Handle "." as current directory
    if target == "." {
        return cmd_view_directory(&root, &root, depth, raw, json);
    }

    // Use unified path resolution - get ALL matches
    let matches = path_resolve::resolve_unified_all(target, &root);

    // Also search for symbols in the index
    let symbol_matches = search_symbols(target, &root);

    let unified = match (matches.len(), symbol_matches.len()) {
        (0, 0) => {
            eprintln!("No matches for: {}", target);
            return 1;
        }
        (1, 0) => matches.into_iter().next().unwrap(),
        (0, 1) => {
            // Single symbol match - construct path to it
            let sym = &symbol_matches[0];
            return cmd_view_symbol_direct(&sym.file, &sym.name, sym.start_line, &root, depth, full, json);
        }
        _ => {
            // Multiple matches - list files and symbols
            if json {
                let file_items: Vec<_> = matches
                    .iter()
                    .map(|m| {
                        serde_json::json!({
                            "path": m.file_path,
                            "type": if m.is_directory { "directory" } else { "file" }
                        })
                    })
                    .collect();
                let symbol_items: Vec<_> = symbol_matches
                    .iter()
                    .map(|sym| {
                        serde_json::json!({
                            "path": format!("{}:{}", sym.file, sym.start_line),
                            "type": "symbol",
                            "name": sym.name,
                            "kind": sym.kind,
                            "parent": sym.parent
                        })
                    })
                    .collect();
                println!("{}", serde_json::json!({
                    "file_matches": file_items,
                    "symbol_matches": symbol_items
                }));
            } else {
                eprintln!("Multiple matches for '{}' - be more specific:", target);
                for m in &matches {
                    let kind = if m.is_directory { "directory" } else { "file" };
                    println!("  {} ({})", m.file_path, kind);
                }
                for sym in &symbol_matches {
                    let symbol_path = match &sym.parent {
                        Some(p) => format!("{}/{}", p, sym.name),
                        None => sym.name.clone(),
                    };
                    println!("  {}/{} ({}, line {})", sym.file, symbol_path, sym.kind, sym.start_line);
                }
            }
            return 1;
        }
    };

    if unified.is_directory {
        // View directory
        cmd_view_directory(&root.join(&unified.file_path), &root, depth, raw, json)
    } else if unified.symbol_path.is_empty() {
        // View file (--full overrides depth to show raw content)
        let effective_depth = if full { -1 } else { depth };
        cmd_view_file(
            &unified.file_path,
            &root,
            effective_depth,
            line_numbers,
            show_deps,
            types_only,
            focus,
            resolve_imports,
            show_all,
            json,
        )
    } else {
        // View symbol within file
        cmd_view_symbol(
            &unified.file_path,
            &unified.symbol_path,
            &root,
            depth,
            line_numbers,
            full,
            json,
        )
    }
}

/// Show callers/callees of a symbol
fn cmd_view_calls(
    root: &Path,
    target: &str,
    show_callers: bool,
    show_callees: bool,
    json: bool,
) -> i32 {
    // Try to parse target as file:symbol or just symbol
    let (symbol, file_hint) = if let Some((sym, file)) = parse_file_symbol_string(target) {
        (sym, Some(file))
    } else {
        (target.to_string(), None)
    };

    // Try index first
    let idx = match index::FileIndex::open(root) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("Failed to open index: {}. Run: moss reindex --call-graph", e);
            return 1;
        }
    };

    let stats = idx.call_graph_stats().unwrap_or_default();
    if stats.calls == 0 {
        eprintln!("Call graph not indexed. Run: moss reindex --call-graph");
        return 1;
    }

    let mut results: Vec<(String, String, usize, &str)> = Vec::new(); // (file, symbol, line, direction)

    // Get callers if requested
    if show_callers {
        match idx.find_callers(&symbol) {
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
                .ok()
                .and_then(|syms| syms.first().map(|(f, _, _, _)| f.clone()))
        };

        if let Some(file_path) = file_path {
            match idx.find_callees(&file_path, &symbol) {
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

/// List symbols matching a kind filter within a scope
fn cmd_view_filtered(root: &Path, scope: &str, kind: &str, json: bool) -> i32 {
    // Normalize kind
    let kind_lower = kind.to_lowercase();
    let kind_filter = match kind_lower.as_str() {
        "class" | "classes" => Some("class"),
        "function" | "functions" | "func" | "fn" => Some("function"),
        "method" | "methods" => Some("method"),
        "all" | "*" => None, // No filter
        _ => {
            eprintln!(
                "Unknown type: {}. Valid types: class, function, method",
                kind
            );
            return 1;
        }
    };

    // Resolve scope to files
    let files_to_search: Vec<PathBuf> = if scope == "." {
        // Search all files in root
        path_resolve::all_files(root)
            .into_iter()
            .filter(|m| m.kind == "file" && has_language_support(&m.path))
            .map(|m| root.join(&m.path))
            .collect()
    } else {
        // Resolve scope
        let matches = path_resolve::resolve(scope, root);
        matches
            .into_iter()
            .filter(|m| m.kind == "file" && has_language_support(&m.path))
            .map(|m| root.join(&m.path))
            .collect()
    };

    let mut all_symbols: Vec<(String, String, String, usize, Option<String>)> = Vec::new();
    let parser = symbols::SymbolParser::new();

    for file_path in files_to_search {
        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let rel_path = file_path
            .strip_prefix(root)
            .unwrap_or(&file_path)
            .to_string_lossy()
            .to_string();

        let syms = parser.parse_file(&file_path, &content);
        for sym in syms {
            let sym_kind = sym.kind.as_str();
            // Apply filter
            if let Some(filter) = kind_filter {
                if sym_kind != filter {
                    continue;
                }
            }
            all_symbols.push((
                rel_path.clone(),
                sym.name,
                sym_kind.to_string(),
                sym.start_line,
                sym.parent,
            ));
        }
    }

    if all_symbols.is_empty() {
        if json {
            println!("[]");
        } else {
            eprintln!("No symbols found matching type: {}", kind);
        }
        return 1;
    }

    // Sort by file, then line
    all_symbols.sort_by(|a, b| (&a.0, a.3).cmp(&(&b.0, b.3)));

    if json {
        let output: Vec<_> = all_symbols
            .iter()
            .map(|(file, name, kind, line, parent)| {
                serde_json::json!({
                    "file": file,
                    "name": name,
                    "kind": kind,
                    "line": line,
                    "parent": parent
                })
            })
            .collect();
        println!("{}", serde_json::to_string(&output).unwrap());
    } else {
        for (file, name, kind, line, parent) in &all_symbols {
            let parent_str = parent
                .as_ref()
                .map(|p| format!(" (in {})", p))
                .unwrap_or_default();
            println!("{}:{} {} {}{}", file, line, kind, name, parent_str);
        }
        eprintln!("\n{} symbols found", all_symbols.len());
    }

    0
}

fn cmd_view_directory(dir: &Path, root: &Path, depth: i32, raw: bool, json: bool) -> i32 {
    let effective_depth = if depth < 0 { None } else { Some(depth as usize) };
    let result = tree::generate_tree(
        dir,
        &tree::TreeOptions {
            max_depth: effective_depth,
            collapse_single: !raw,
            ..Default::default()
        },
    );

    let rel_path = dir
        .strip_prefix(root)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| dir.to_string_lossy().to_string());

    if json {
        println!(
            "{}",
            serde_json::json!({
                "type": "directory",
                "path": rel_path,
                "file_count": result.file_count,
                "dir_count": result.dir_count,
                "tree": result.lines
            })
        );
    } else {
        for line in &result.lines {
            println!("{}", line);
        }
        println!();
        println!(
            "{} directories, {} files",
            result.dir_count, result.file_count
        );
    }
    0
}

/// Resolve an import to a local file path based on the source file's language.
/// Falls back to external package resolution (stdlib, site-packages, mod cache) if local fails.
fn resolve_import(module: &str, current_file: &Path, root: &Path) -> Option<PathBuf> {
    let lang = moss_languages::support_for_path(current_file)?;

    // Try local resolution first
    if let Some(path) = lang.resolve_local_import(module, current_file, root) {
        return Some(path);
    }

    // Fall back to external resolution
    lang.resolve_external_import(module, root).map(|pkg| pkg.path)
}

fn cmd_view_file(
    file_path: &str,
    root: &Path,
    depth: i32,
    line_numbers: bool,
    show_deps: bool,
    types_only: bool,
    focus: Option<&str>,
    resolve_imports: bool,
    show_all: bool,
    json: bool,
) -> i32 {
    let full_path = root.join(file_path);
    let content = match std::fs::read_to_string(&full_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading {}: {}", file_path, e);
            return 1;
        }
    };

    // depth -1 or very high: show full content
    // depth 0: just file info
    // depth 1: skeleton (signatures)
    // depth 2+: skeleton with more detail

    if depth < 0 || depth > 2 {
        // Full content view
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "type": "file",
                    "path": file_path,
                    "content": content
                })
            );
        } else if line_numbers {
            for (i, line) in content.lines().enumerate() {
                println!("{:4} {}", i + 1, line);
            }
        } else {
            print!("{}", content);
        }
        return 0;
    }

    // Skeleton view
    let mut extractor = if show_all {
        skeleton::SkeletonExtractor::with_all()
    } else {
        skeleton::SkeletonExtractor::new()
    };
    let skeleton_result = extractor.extract(&full_path, &content);

    // Filter to types only if requested
    let skeleton_result = if types_only {
        skeleton_result.filter_types()
    } else {
        skeleton_result
    };

    // Get deps if showing deps, focus, or resolve_imports mode
    let deps_result = if show_deps || focus.is_some() || resolve_imports {
        let deps_extractor = deps::DepsExtractor::new();
        Some(deps_extractor.extract(&full_path, &content))
    } else {
        None
    };

    if json {
        fn symbol_to_json(sym: &skeleton::SkeletonSymbol, include_children: bool) -> serde_json::Value {
            let children = if include_children {
                sym.children
                    .iter()
                    .map(|c| symbol_to_json(c, include_children))
                    .collect::<Vec<_>>()
            } else {
                vec![]
            };
            serde_json::json!({
                "name": sym.name,
                "kind": sym.kind,
                "signature": sym.signature,
                "docstring": sym.docstring,
                "start_line": sym.start_line,
                "end_line": sym.end_line,
                "children": children
            })
        }

        let include_children = depth >= 2;
        let symbols: Vec<_> = skeleton_result
            .symbols
            .iter()
            .map(|s| symbol_to_json(s, include_children))
            .collect();

        let mut output = serde_json::json!({
            "type": "file",
            "path": file_path,
            "line_count": content.lines().count(),
            "symbols": symbols
        });

        if let Some(deps) = deps_result {
            output["imports"] = serde_json::json!(deps.imports.iter().map(|i| {
                serde_json::json!({
                    "module": i.module,
                    "names": i.names,
                    "line": i.line
                })
            }).collect::<Vec<_>>());

            if !deps.reexports.is_empty() {
                output["reexports"] = serde_json::json!(deps.reexports.iter().map(|r| {
                    serde_json::json!({
                        "module": r.module,
                        "names": r.names,
                        "is_star": r.is_star,
                        "line": r.line
                    })
                }).collect::<Vec<_>>());
            }
        }

        println!("{}", output);
    } else {
        println!("# {}", file_path);
        println!("Lines: {}", content.lines().count());

        if let Some(ref deps) = deps_result {
            if show_deps && !deps.imports.is_empty() {
                println!("\n## Imports");
                for imp in &deps.imports {
                    if imp.names.is_empty() {
                        println!("  import {}", imp.module);
                    } else {
                        println!("  from {} import {}", imp.module, imp.names.join(", "));
                    }
                }
            }

            if show_deps && !deps.exports.is_empty() {
                println!("\n## Exports");
                for exp in &deps.exports {
                    println!("  {}", exp.name);
                }
            }

            if show_deps && !deps.reexports.is_empty() {
                println!("\n## Re-exports");
                for reexp in &deps.reexports {
                    if reexp.is_star {
                        println!("  export * from '{}'", reexp.module);
                    } else {
                        println!(
                            "  export {{ {} }} from '{}'",
                            reexp.names.join(", "),
                            reexp.module
                        );
                    }
                }
            }
        }

        // Only show symbols if not in deps-only mode
        if depth >= 1 && !show_deps {
            // Always include docstrings (was: depth >= 2)
            let formatted = skeleton_result.format(true);
            if !formatted.is_empty() {
                println!("\n## Symbols");
                println!("{}", formatted);
            }
        }

        // Fisheye mode: show skeletons of imported files (local and external)
        if let Some(focus_filter) = focus {
            let deps = deps_result.as_ref().unwrap();
            let filter_all = focus_filter == "*";

            let mut resolved: Vec<(String, PathBuf, String)> = Vec::new();
            for imp in &deps.imports {
                let matches_filter = filter_all
                    || imp.module.contains(focus_filter)
                    || imp.module == focus_filter;

                if matches_filter {
                    if let Some(resolved_path) = resolve_import(&imp.module, &full_path, root) {
                        let display = if let Ok(rel_path) = resolved_path.strip_prefix(root) {
                            rel_path.display().to_string()
                        } else {
                            format!("[{}]", imp.module)
                        };
                        resolved.push((imp.module.clone(), resolved_path, display));
                    }
                }
            }

            if !resolved.is_empty() {
                println!("\n## Imported Modules (Skeletons)");
                let deps_extractor = deps::DepsExtractor::new();

                for (module_name, resolved_path, display) in resolved {
                    if let Ok(import_content) = std::fs::read_to_string(&resolved_path) {
                        let mut import_extractor = skeleton::SkeletonExtractor::new();
                        let import_skeleton = import_extractor.extract(&resolved_path, &import_content);
                        let import_skeleton = if types_only {
                            import_skeleton.filter_types()
                        } else {
                            import_skeleton
                        };

                        let formatted = import_skeleton.format(false);
                        if !formatted.is_empty() {
                            println!("\n### {} ({})", module_name, display);
                            println!("{}", formatted);
                        }

                        // Check for barrel file re-exports and follow them
                        let import_deps = deps_extractor.extract(&resolved_path, &import_content);
                        for reexp in &import_deps.reexports {
                            if let Some(reexp_path) = resolve_import(&reexp.module, &resolved_path, root) {
                                if let Ok(reexp_content) = std::fs::read_to_string(&reexp_path) {
                                    let mut reexp_extractor = skeleton::SkeletonExtractor::new();
                                    let reexp_skeleton = reexp_extractor.extract(&reexp_path, &reexp_content);
                                    let reexp_skeleton = if types_only {
                                        reexp_skeleton.filter_types()
                                    } else {
                                        reexp_skeleton
                                    };

                                    let formatted = reexp_skeleton.format(false);
                                    if !formatted.is_empty() {
                                        let reexp_display = reexp_path
                                            .strip_prefix(root)
                                            .map(|p| p.display().to_string())
                                            .unwrap_or_else(|_| format!("[{}]", reexp.module));
                                        let export_desc = if reexp.is_star {
                                            format!("export * from '{}'", reexp.module)
                                        } else {
                                            format!(
                                                "export {{ {} }} from '{}'",
                                                reexp.names.join(", "),
                                                reexp.module
                                            )
                                        };
                                        println!("\n### {} → {} ({})", module_name, export_desc, reexp_display);
                                        println!("{}", formatted);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Resolve imports mode: show only the specific imported symbols
        if resolve_imports {
            let deps = deps_result.as_ref().unwrap();

            let mut resolved_symbols: Vec<(String, String, String)> = Vec::new();

            for imp in &deps.imports {
                if imp.names.is_empty() {
                    continue;
                }

                if let Some(resolved_path) = resolve_import(&imp.module, &full_path, root) {
                    if let Ok(import_content) = std::fs::read_to_string(&resolved_path) {
                        let mut import_extractor = skeleton::SkeletonExtractor::new();
                        let import_skeleton = import_extractor.extract(&resolved_path, &import_content);

                        for name in &imp.names {
                            if let Some(sig) = find_symbol_signature(&import_skeleton.symbols, name) {
                                resolved_symbols.push((imp.module.clone(), name.clone(), sig));
                            }
                        }
                    }
                }
            }

            if !resolved_symbols.is_empty() {
                println!("\n## Resolved Imports");
                let mut current_module = String::new();
                for (module, _name, sig) in resolved_symbols {
                    if module != current_module {
                        println!("\n# from {}", module);
                        current_module = module;
                    }
                    println!("{}", sig);
                }
            }
        }
    }
    0
}

/// Find a symbol's signature in a skeleton
fn find_symbol_signature(symbols: &[skeleton::SkeletonSymbol], name: &str) -> Option<String> {
    for sym in symbols {
        if sym.name == name {
            return Some(sym.signature.clone());
        }
        if let Some(sig) = find_symbol_signature(&sym.children, name) {
            return Some(sig);
        }
    }
    None
}

fn cmd_view_symbol(
    file_path: &str,
    symbol_path: &[String],
    root: &Path,
    depth: i32,
    _line_numbers: bool,
    full: bool,
    json: bool,
) -> i32 {
    let full_path = root.join(file_path);
    let content = match std::fs::read_to_string(&full_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading {}: {}", file_path, e);
            return 1;
        }
    };

    let mut parser = symbols::SymbolParser::new();
    let symbol_name = symbol_path.last().unwrap();

    // Try to find and extract the symbol
    if let Some(source) = parser.extract_symbol_source(&full_path, &content, symbol_name) {
        let full_symbol_path = format!("{}/{}", file_path, symbol_path.join("/"));

        if json {
            println!(
                "{}",
                serde_json::json!({
                    "type": "symbol",
                    "path": full_symbol_path,
                    "file": file_path,
                    "symbol": symbol_name,
                    "source": source
                })
            );
        } else {
            if depth >= 0 {
                println!("# {}", full_symbol_path);
            }
            println!("{}", source);
        }
        0
    } else {
        // Try skeleton extraction for more context
        let mut extractor = skeleton::SkeletonExtractor::new();
        let skeleton_result = extractor.extract(&full_path, &content);

        // Search for symbol in skeleton
        fn find_symbol<'a>(
            symbols: &'a [skeleton::SkeletonSymbol],
            name: &str,
        ) -> Option<&'a skeleton::SkeletonSymbol> {
            for sym in symbols {
                if sym.name == name {
                    return Some(sym);
                }
                if let Some(found) = find_symbol(&sym.children, name) {
                    return Some(found);
                }
            }
            None
        }

        if let Some(sym) = find_symbol(&skeleton_result.symbols, symbol_name) {
            let full_symbol_path = format!("{}/{}", file_path, symbol_path.join("/"));

            // When --full is requested, extract source using line numbers
            if full && sym.start_line > 0 && sym.end_line > 0 {
                let lines: Vec<&str> = content.lines().collect();
                let start = (sym.start_line - 1) as usize;
                let end = std::cmp::min(sym.end_line as usize, lines.len());
                let source: String = lines[start..end].join("\n");

                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "type": "symbol",
                            "path": full_symbol_path,
                            "file": file_path,
                            "symbol": symbol_name,
                            "source": source,
                            "start_line": sym.start_line,
                            "end_line": sym.end_line
                        })
                    );
                } else {
                    if depth >= 0 {
                        println!("# {}", full_symbol_path);
                    }
                    println!("{}", source);
                }
                return 0;
            }

            // Default: show skeleton (signature + docstring)
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "type": "symbol",
                        "path": full_symbol_path,
                        "name": sym.name,
                        "kind": sym.kind,
                        "signature": sym.signature,
                        "docstring": sym.docstring,
                        "start_line": sym.start_line,
                        "end_line": sym.end_line
                    })
                );
            } else {
                println!("# {} ({})", full_symbol_path, sym.kind);
                if !sym.signature.is_empty() {
                    println!("{}", sym.signature);
                }
                if let Some(doc) = &sym.docstring {
                    println!("\n{}", doc);
                }
            }
            0
        } else {
            eprintln!("Symbol not found: {}", symbol_name);
            1
        }
    }
}
