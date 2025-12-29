//! View command - unified view of files, directories, and symbols.

use crate::commands::filter::detect_project_languages;
use crate::config::MossConfig;
use crate::filter::Filter;
use crate::merge::Merge;
use crate::tree::{DocstringDisplay, FormatOptions, ViewNode, ViewNodeKind};
use crate::{daemon, deps, index, path_resolve, skeleton, symbols, tree};
use clap::Args;
use moss_languages::support_for_path;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// View command configuration.
#[derive(Debug, Clone, Deserialize, Default, Merge)]
#[serde(default)]
pub struct ViewConfig {
    /// Default depth for tree expansion (0=names, 1=signatures, 2=children, -1=all)
    pub depth: Option<i32>,
    /// Show line numbers by default
    pub line_numbers: Option<bool>,
    /// Show full docstrings by default (vs summary)
    pub show_docs: Option<bool>,
}

impl ViewConfig {
    pub fn depth(&self) -> i32 {
        self.depth.unwrap_or(1)
    }

    pub fn line_numbers(&self) -> bool {
        self.line_numbers.unwrap_or(false)
    }

    pub fn show_docs(&self) -> bool {
        self.show_docs.unwrap_or(false)
    }
}

/// View command arguments.
#[derive(Args, Debug)]
pub struct ViewArgs {
    /// Target to view (path like src/main.py/Foo/bar). Optional when using filters.
    pub target: Option<String>,

    /// Root directory (defaults to current directory)
    #[arg(short, long)]
    pub root: Option<PathBuf>,

    /// Depth of expansion (0=names only, 1=signatures, 2=with children, -1=all)
    #[arg(short, long)]
    pub depth: Option<i32>,

    /// Show line numbers
    #[arg(short = 'n', long)]
    pub line_numbers: bool,

    /// Show dependencies (imports/exports)
    #[arg(long)]
    pub deps: bool,

    /// Filter by symbol type: class, function, method
    #[arg(short = 't', long = "type")]
    pub kind: Option<String>,

    /// Show only type definitions (class, struct, enum, interface, type alias)
    #[arg(long = "types-only")]
    pub types_only: bool,

    /// Disable smart display (no collapsing single-child dirs)
    #[arg(long)]
    pub raw: bool,

    /// Focus view: show target at high detail, imports at signature level
    #[arg(long, value_name = "MODULE", num_args = 0..=1, default_missing_value = "*", require_equals = true)]
    pub focus: Option<String>,

    /// Resolve imports: inline signatures of specific imported symbols
    #[arg(long)]
    pub resolve_imports: bool,

    /// Show all symbols including private ones
    #[arg(long = "include-private")]
    pub include_private: bool,

    /// Show full source code
    #[arg(long)]
    pub full: bool,

    /// Show full docstrings
    #[arg(long)]
    pub docs: bool,

    /// Context view: skeleton + imports combined
    #[arg(long)]
    pub context: bool,

    /// Exclude paths matching pattern or @alias
    #[arg(long, value_name = "PATTERN")]
    pub exclude: Vec<String>,

    /// Include only paths matching pattern or @alias
    #[arg(long, value_name = "PATTERN")]
    pub only: Vec<String>,
}

/// Run view command with args.
pub fn run(args: ViewArgs, format: crate::output::OutputFormat) -> i32 {
    let effective_root = args
        .root
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let config = MossConfig::load(&effective_root);

    cmd_view(
        args.target.as_deref(),
        args.root.as_deref(),
        args.depth.unwrap_or_else(|| config.view.depth()),
        args.line_numbers || config.view.line_numbers(),
        args.deps,
        args.kind.as_deref(),
        args.types_only,
        args.raw,
        args.focus.as_deref(),
        args.resolve_imports,
        args.include_private,
        args.full,
        args.docs || config.view.show_docs(),
        args.context,
        format.is_json(),
        format.is_pretty(),
        format.use_colors(),
        &args.exclude,
        &args.only,
    )
}

/// Check if a file has language support (symbols can be extracted)
fn has_language_support(path: &str) -> bool {
    support_for_path(Path::new(path))
        .map(|lang| lang.has_symbols())
        .unwrap_or(false)
}

/// Search for symbols in the index by name
fn search_symbols(query: &str, root: &Path) -> Vec<index::SymbolMatch> {
    // Try index first - if enabled, use it (or build it if empty)
    if let Some(mut idx) = index::FileIndex::open_if_enabled(root) {
        let stats = idx.call_graph_stats().unwrap_or_default();
        if stats.symbols == 0 {
            // Index exists but has no symbols - build call graph now
            // This is a one-time cost that makes future lookups fast
            eprintln!("Building symbol index...");
            if let Err(e) = idx.refresh_call_graph() {
                eprintln!("Warning: failed to build index: {}", e);
                return search_symbols_unindexed(query, root);
            }
        }
        if let Ok(symbols) = idx.find_symbols(query, None, true, 10) {
            return symbols;
        }
    }

    // Fallback: walk filesystem and parse files (only if index disabled)
    search_symbols_unindexed(query, root)
}

/// Search for symbols by walking filesystem and parsing files
fn search_symbols_unindexed(query: &str, root: &Path) -> Vec<index::SymbolMatch> {
    use ignore::WalkBuilder;
    use nucleo_matcher::{Config, Matcher};

    let query_lower = query.to_lowercase();
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut matches = Vec::new();

    let walker = WalkBuilder::new(root).hidden(true).git_ignore(true).build();

    let extractor = skeleton::SkeletonExtractor::new();

    for entry in walker.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        // Only parse files with language support
        let Some(lang) = support_for_path(path) else {
            continue;
        };
        if !lang.has_symbols() {
            continue;
        }

        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };

        let result = extractor.extract(path, &content);
        let rel_path = path.strip_prefix(root).unwrap_or(path);

        // Collect matching symbols
        collect_matching_symbols(
            &result.symbols,
            &query_lower,
            &mut matcher,
            &rel_path.to_string_lossy(),
            None,
            &mut matches,
        );

        // Limit results
        if matches.len() >= 20 {
            break;
        }
    }

    // Sort by score descending
    matches.sort_by(|a, b| b.1.cmp(&a.1));
    matches.into_iter().take(10).map(|(m, _)| m).collect()
}

fn collect_matching_symbols(
    symbols: &[skeleton::SkeletonSymbol],
    query: &str,
    matcher: &mut nucleo_matcher::Matcher,
    file: &str,
    parent: Option<&str>,
    matches: &mut Vec<(index::SymbolMatch, u32)>,
) {
    use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
    use nucleo_matcher::Utf32Str;

    let pattern = Pattern::new(
        query,
        CaseMatching::Ignore,
        Normalization::Smart,
        AtomKind::Fuzzy,
    );

    for sym in symbols {
        let name_lower = sym.name.to_lowercase();
        let mut buf = Vec::new();
        let haystack = Utf32Str::new(&name_lower, &mut buf);

        if let Some(score) = pattern.score(haystack, matcher) {
            matches.push((
                index::SymbolMatch {
                    name: sym.name.clone(),
                    kind: sym.kind.to_string(),
                    file: file.to_string(),
                    start_line: sym.start_line,
                    end_line: sym.end_line,
                    parent: parent.map(|s| s.to_string()),
                },
                score,
            ));
        }

        // Recurse into children
        collect_matching_symbols(
            &sym.children,
            query,
            matcher,
            file,
            Some(&sym.name),
            matches,
        );
    }
}

/// View a symbol directly by file and name
fn cmd_view_symbol_direct(
    file_path: &str,
    symbol_name: &str,
    root: &Path,
    depth: i32,
    full: bool,
    show_docs: bool,
    json: bool,
    pretty: bool,
    use_colors: bool,
) -> i32 {
    cmd_view_symbol(
        file_path,
        &[symbol_name.to_string()],
        root,
        depth,
        full,
        show_docs,
        json,
        pretty,
        use_colors,
    )
}

/// Unified view command
pub fn cmd_view(
    target: Option<&str>,
    root: Option<&Path>,
    depth: i32,
    line_numbers: bool,
    show_deps: bool,
    kind_filter: Option<&str>,
    types_only: bool,
    raw: bool,
    focus: Option<&str>,
    resolve_imports: bool,
    include_private: bool,
    full: bool,
    show_docs: bool,
    context: bool,
    json: bool,
    pretty: bool,
    use_colors: bool,
    exclude: &[String],
    only: &[String],
) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // Ensure daemon is running if configured
    daemon::maybe_start_daemon(&root);

    // Build filter if exclude/only patterns are specified
    let filter = if !exclude.is_empty() || !only.is_empty() {
        let config = MossConfig::load(&root);
        let languages = detect_project_languages(&root);
        let lang_refs: Vec<&str> = languages.iter().map(|s| s.as_str()).collect();

        match Filter::new(exclude, only, &config.filter, &lang_refs) {
            Ok(f) => {
                // Print warnings for disabled aliases
                for warning in f.warnings() {
                    eprintln!("warning: {}", warning);
                }
                Some(f)
            }
            Err(e) => {
                eprintln!("error: {}", e);
                return 1;
            }
        }
    } else {
        None
    };

    // If kind filter is specified without target (or with "."), list matching symbols
    if let Some(kind) = kind_filter {
        let scope = target.unwrap_or(".");
        return cmd_view_filtered(&root, scope, kind, json);
    }

    // --focus requires a file target
    if focus.is_some() && target.is_none() {
        eprintln!("--focus requires a file target");
        return 1;
    }

    let target = target.unwrap_or(".");

    // Handle "." as current directory
    if target == "." {
        return cmd_view_directory(
            &root,
            &root,
            depth,
            raw,
            json,
            pretty,
            use_colors,
            filter.as_ref(),
        );
    }

    // Use unified path resolution - get ALL matches
    let matches = path_resolve::resolve_unified_all(target, &root);

    // Only search for symbols if no file/directory matches found
    // This avoids expensive filesystem walks when we already have a match
    let symbol_matches = if matches.is_empty() {
        search_symbols(target, &root)
    } else {
        Vec::new()
    };

    let unified = match (matches.len(), symbol_matches.len()) {
        (0, 0) => {
            eprintln!("No matches for: {}", target);
            return 1;
        }
        (1, 0) => matches.into_iter().next().unwrap(),
        (0, 1) => {
            // Single symbol match - construct path to it
            let sym = &symbol_matches[0];
            return cmd_view_symbol_direct(
                &sym.file, &sym.name, &root, depth, full, show_docs, json, pretty, use_colors,
            );
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
                println!(
                    "{}",
                    serde_json::json!({
                        "file_matches": file_items,
                        "symbol_matches": symbol_items
                    })
                );
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
                    println!(
                        "  {}/{} ({}, line {})",
                        sym.file, symbol_path, sym.kind, sym.start_line
                    );
                }
            }
            return 1;
        }
    };

    if unified.is_directory {
        // View directory
        cmd_view_directory(
            &root.join(&unified.file_path),
            &root,
            depth,
            raw,
            json,
            pretty,
            use_colors,
            filter.as_ref(),
        )
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
            include_private,
            show_docs,
            context,
            json,
            pretty,
            use_colors,
        )
    } else {
        // View symbol within file
        cmd_view_symbol(
            &unified.file_path,
            &unified.symbol_path,
            &root,
            depth,
            full,
            show_docs,
            json,
            pretty,
            use_colors,
        )
    }
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

fn cmd_view_directory(
    dir: &Path,
    _root: &Path,
    depth: i32,
    raw: bool,
    json: bool,
    pretty: bool,
    use_colors: bool,
    filter: Option<&Filter>,
) -> i32 {
    let effective_depth = if depth < 0 {
        None
    } else {
        Some(depth as usize)
    };

    // Include symbols inside files when depth > 1
    let include_symbols = depth > 1 || depth < 0;

    // Generate ViewNode tree
    let view_node = tree::generate_view_tree(
        dir,
        &tree::TreeOptions {
            max_depth: effective_depth,
            collapse_single: !raw,
            include_symbols,
            ..Default::default()
        },
    );

    // Apply filter if present
    let view_node = if let Some(f) = filter {
        filter_view_node(view_node, f)
    } else {
        view_node
    };

    // Count files and directories
    fn count_nodes(node: &ViewNode) -> (usize, usize) {
        let mut files = 0;
        let mut dirs = 0;
        for child in &node.children {
            match child.kind {
                ViewNodeKind::Directory => {
                    dirs += 1;
                    let (sub_files, sub_dirs) = count_nodes(child);
                    files += sub_files;
                    dirs += sub_dirs;
                }
                ViewNodeKind::File => files += 1,
                ViewNodeKind::Symbol(_) => {}
            }
        }
        (files, dirs)
    }
    let (file_count, dir_count) = count_nodes(&view_node);

    if json {
        // Serialize the ViewNode directly for structured output
        println!("{}", serde_json::to_string(&view_node).unwrap());
    } else {
        // Format as text tree (minimal by default unless --pretty)
        let format_options = FormatOptions {
            minimal: !pretty,
            use_colors,
            ..Default::default()
        };
        let lines = tree::format_view_node(&view_node, &format_options);
        for line in &lines {
            println!("{}", line);
        }
        println!();
        println!("{} directories, {} files", dir_count, file_count);
    }
    0
}

/// Filter a ViewNode tree, removing nodes that don't pass the filter.
fn filter_view_node(mut node: ViewNode, filter: &Filter) -> ViewNode {
    node.children = node
        .children
        .into_iter()
        .filter_map(|child| {
            // Check if this node passes the filter
            let path = Path::new(&child.path);
            match child.kind {
                ViewNodeKind::Directory => {
                    // Recursively filter directory children
                    let filtered = filter_view_node(child, filter);
                    // Keep directory if it has any children left
                    if filtered.children.is_empty() {
                        None
                    } else {
                        Some(filtered)
                    }
                }
                ViewNodeKind::File => {
                    if filter.matches(path) {
                        Some(child)
                    } else {
                        None
                    }
                }
                ViewNodeKind::Symbol(_) => {
                    // Symbols are not filtered by path
                    Some(child)
                }
            }
        })
        .collect();
    node
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
    lang.resolve_external_import(module, root)
        .map(|pkg| pkg.path)
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
    include_private: bool,
    show_docs: bool,
    context: bool,
    json: bool,
    pretty: bool,
    use_colors: bool,
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

    if !(0..=2).contains(&depth) {
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

    // Get grammar for syntax highlighting
    let grammar =
        moss_languages::support_for_path(&full_path).map(|s| s.grammar_name().to_string());

    // Skeleton view
    let extractor = if include_private {
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

    // Get deps if showing deps, focus, resolve_imports, or context mode
    let deps_result = if show_deps || focus.is_some() || resolve_imports || context {
        let deps_extractor = deps::DepsExtractor::new();
        Some(deps_extractor.extract(&full_path, &content))
    } else {
        None
    };

    if json {
        // Use ViewNode for consistent structured output
        let view_node = skeleton_result.to_view_node(grammar.as_deref());
        println!("{}", serde_json::to_string(&view_node).unwrap());
    } else {
        println!("# {}", file_path);
        println!("Lines: {}", content.lines().count());

        if let Some(ref deps) = deps_result {
            let show = show_deps || context;
            if show && !deps.imports.is_empty() {
                println!("\n## Imports");
                for imp in &deps.imports {
                    if imp.names.is_empty() {
                        println!("  import {}", imp.module);
                    } else {
                        println!("  from {} import {}", imp.module, imp.names.join(", "));
                    }
                }
            }

            if show && !deps.exports.is_empty() {
                println!("\n## Exports");
                for exp in &deps.exports {
                    println!("  {}", exp.name);
                }
            }

            if show && !deps.reexports.is_empty() {
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

        // Show symbols if depth >= 1 and not in deps-only mode (or context mode which shows both)
        if depth >= 1 && (!show_deps || context) {
            // Use ViewNode for consistent formatting
            let view_node = skeleton_result.to_view_node(grammar.as_deref());
            let format_options = FormatOptions {
                docstrings: if context {
                    DocstringDisplay::None // Skip docstrings in context mode for brevity
                } else if show_docs {
                    DocstringDisplay::Full
                } else {
                    DocstringDisplay::Summary
                },
                line_numbers: true,
                skip_root: true, // Skip file header, we already printed it
                max_depth: None,
                minimal: !pretty,
                use_colors,
            };
            let lines = tree::format_view_node(&view_node, &format_options);
            if !lines.is_empty() {
                println!("\n## Symbols");
                for line in lines {
                    println!("{}", line);
                }
            }
        }

        // Fisheye mode: show skeletons of imported files (local and external)
        if let Some(focus_filter) = focus {
            let deps = deps_result.as_ref().unwrap();
            let filter_all = focus_filter == "*";

            let mut resolved: Vec<(String, PathBuf, String)> = Vec::new();
            for imp in &deps.imports {
                let matches_filter =
                    filter_all || imp.module.contains(focus_filter) || imp.module == focus_filter;

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
                        let import_extractor = skeleton::SkeletonExtractor::new();
                        let import_skeleton =
                            import_extractor.extract(&resolved_path, &import_content);
                        let import_skeleton = if types_only {
                            import_skeleton.filter_types()
                        } else {
                            import_skeleton
                        };

                        let import_grammar = moss_languages::support_for_path(&resolved_path)
                            .map(|s| s.grammar_name().to_string());
                        let view_node = import_skeleton.to_view_node(import_grammar.as_deref());
                        let format_options = FormatOptions {
                            docstrings: DocstringDisplay::None,
                            line_numbers: true,
                            skip_root: true,
                            max_depth: None,
                            minimal: !pretty,
                            use_colors,
                        };
                        let lines = tree::format_view_node(&view_node, &format_options);
                        if !lines.is_empty() {
                            println!("\n### {} ({})", module_name, display);
                            for line in lines {
                                println!("{}", line);
                            }
                        }

                        // Check for barrel file re-exports and follow them
                        let import_deps = deps_extractor.extract(&resolved_path, &import_content);
                        for reexp in &import_deps.reexports {
                            if let Some(reexp_path) =
                                resolve_import(&reexp.module, &resolved_path, root)
                            {
                                if let Ok(reexp_content) = std::fs::read_to_string(&reexp_path) {
                                    let reexp_extractor = skeleton::SkeletonExtractor::new();
                                    let reexp_skeleton =
                                        reexp_extractor.extract(&reexp_path, &reexp_content);
                                    let reexp_skeleton = if types_only {
                                        reexp_skeleton.filter_types()
                                    } else {
                                        reexp_skeleton
                                    };

                                    let reexp_grammar =
                                        moss_languages::support_for_path(&reexp_path)
                                            .map(|s| s.grammar_name().to_string());
                                    let view_node =
                                        reexp_skeleton.to_view_node(reexp_grammar.as_deref());
                                    let format_options = FormatOptions {
                                        docstrings: DocstringDisplay::None,
                                        line_numbers: true,
                                        skip_root: true,
                                        max_depth: None,
                                        minimal: !pretty,
                                        use_colors,
                                    };
                                    let lines = tree::format_view_node(&view_node, &format_options);
                                    if !lines.is_empty() {
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
                                        println!(
                                            "\n### {} → {} ({})",
                                            module_name, export_desc, reexp_display
                                        );
                                        for line in lines {
                                            println!("{}", line);
                                        }
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
                        let import_extractor = skeleton::SkeletonExtractor::new();
                        let import_skeleton =
                            import_extractor.extract(&resolved_path, &import_content);

                        for name in &imp.names {
                            if let Some(sig) = find_symbol_signature(&import_skeleton.symbols, name)
                            {
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

/// Find a symbol by name in a skeleton (recursive)
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

/// Find a symbol's signature in a skeleton
fn find_symbol_signature(symbols: &[skeleton::SkeletonSymbol], name: &str) -> Option<String> {
    find_symbol(symbols, name).map(|sym| sym.signature.clone())
}

fn cmd_view_symbol(
    file_path: &str,
    symbol_path: &[String],
    root: &Path,
    depth: i32,
    full: bool,
    show_docs: bool,
    json: bool,
    pretty: bool,
    use_colors: bool,
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

    // Get grammar for syntax highlighting
    let grammar =
        moss_languages::support_for_path(&full_path).map(|s| s.grammar_name().to_string());

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
            // Apply syntax highlighting in pretty mode
            let highlighted = if let Some(ref g) = grammar {
                tree::highlight_source(&source, g, use_colors)
            } else {
                source
            };
            println!("{}", highlighted);
        }
        0
    } else {
        // Try skeleton extraction for more context
        let extractor = skeleton::SkeletonExtractor::new();
        let skeleton_result = extractor.extract(&full_path, &content);

        if let Some(sym) = find_symbol(&skeleton_result.symbols, symbol_name) {
            let full_symbol_path = format!("{}/{}", file_path, symbol_path.join("/"));

            // When --full is requested, extract source using line numbers
            if full && sym.start_line > 0 && sym.end_line > 0 {
                let lines: Vec<&str> = content.lines().collect();
                let start = sym.start_line - 1;
                let end = std::cmp::min(sym.end_line, lines.len());
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
                    // Apply syntax highlighting in pretty mode
                    let highlighted = if let Some(ref g) = grammar {
                        tree::highlight_source(&source, g, use_colors)
                    } else {
                        source
                    };
                    println!("{}", highlighted);
                }
                return 0;
            }

            // Default: show skeleton (signature + docstring + children)
            let view_node = sym.to_view_node(&full_symbol_path, grammar.as_deref());
            if json {
                // Use ViewNode for consistent structured output
                println!("{}", serde_json::to_string(&view_node).unwrap());
            } else {
                // Use ViewNode for consistent text formatting
                println!("# {} ({})", full_symbol_path, sym.kind);
                let format_options = FormatOptions {
                    docstrings: if show_docs {
                        DocstringDisplay::Full
                    } else {
                        DocstringDisplay::Summary
                    },
                    line_numbers: true,
                    skip_root: false,
                    max_depth: None,
                    minimal: !pretty,
                    use_colors,
                };
                let lines = tree::format_view_node(&view_node, &format_options);
                for line in lines {
                    println!("{}", line);
                }
            }
            0
        } else {
            // "Did You Mean?" bridge: if symbol not found but text exists, suggest grep
            let text_matches: Vec<_> = content.match_indices(symbol_name).collect();
            if text_matches.is_empty() {
                eprintln!("Symbol not found: {}", symbol_name);
            } else {
                eprintln!(
                    "Symbol '{}' not found in AST. However, the string '{}' appears {} time{}.",
                    symbol_name,
                    symbol_name,
                    text_matches.len(),
                    if text_matches.len() == 1 { "" } else { "s" }
                );
                eprintln!("Did you mean: moss grep '{}' {}", symbol_name, file_path);
            }
            1
        }
    }
}
