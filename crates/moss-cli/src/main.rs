use clap::{Parser, Subcommand};
use moss_languages::support_for_path;
use std::path::{Path, PathBuf};

mod analyze;
mod anchors;
mod cfg;
mod commands;
mod complexity;
mod daemon;
mod deps;
mod edit;
mod grep;
mod health;
mod index;
mod overview;
mod path_resolve;
mod scopes;
mod skeleton;
mod summarize;
mod symbols;
mod tree;


/// Detect if a string looks like a file path
fn looks_like_file(s: &str) -> bool {
    // Contains path separator
    if s.contains('/') {
        return true;
    }
    // Has file extension (dot followed by 1-10 alphanumeric chars at end)
    if let Some(idx) = s.rfind('.') {
        let ext = &s[idx + 1..];
        if !ext.is_empty() && ext.len() <= 10 && ext.chars().all(|c| c.is_alphanumeric()) {
            return true;
        }
    }
    false
}

/// Try to parse file and symbol from a single string
/// Supports separators: :, ::, #
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

/// Normalize flexible symbol arguments to (symbol, optional_file)
/// Supports:
/// - ["symbol"] -> ("symbol", None)
/// - ["file:symbol"], ["file::symbol"], ["file#symbol"] -> ("symbol", Some("file"))
/// - ["file", "symbol"] -> ("symbol", Some("file"))
/// - ["symbol", "file"] -> ("symbol", Some("file"))
fn normalize_symbol_args(args: &[String]) -> (String, Option<String>) {
    match args.len() {
        0 => (String::new(), None),
        1 => {
            let arg = &args[0];
            // Try to parse file:symbol, file::symbol, or file#symbol
            if let Some((symbol, file)) = parse_file_symbol_string(arg) {
                return (symbol, Some(file));
            }
            (arg.clone(), None)
        }
        _ => {
            let (a, b) = (&args[0], &args[1]);
            let a_is_file = looks_like_file(a);
            let b_is_file = looks_like_file(b);

            if a_is_file && !b_is_file {
                (b.clone(), Some(a.clone()))
            } else if b_is_file && !a_is_file {
                (a.clone(), Some(b.clone()))
            } else if a_is_file && b_is_file {
                // Both look like files, use first as file, second as symbol
                (b.clone(), Some(a.clone()))
            } else {
                // Neither looks like file, first is symbol, second is scope hint
                (a.clone(), Some(b.clone()))
            }
        }
    }
}

#[derive(Parser)]
#[command(name = "moss")]
#[command(about = "Fast code intelligence CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output as JSON
    #[arg(long, global = true)]
    json: bool,

}

#[derive(Subcommand)]
enum Commands {
    /// Resolve a fuzzy path to exact location(s)
    Path {
        /// Query to resolve (file path, partial name, or symbol)
        query: String,

        /// Root directory to search (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,
    },

    /// View a node in the codebase tree (directory, file, or symbol)
    View {
        /// Target to view (path like src/main.py/Foo/bar). Optional when using filters.
        target: Option<String>,

        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,

        /// Depth of expansion (0=names only, 1=signatures, 2=with children, -1=all)
        #[arg(short, long, default_value = "1")]
        depth: i32,

        /// Show line numbers
        #[arg(short = 'n', long)]
        line_numbers: bool,

        /// Show dependencies (imports/exports)
        #[arg(long)]
        deps: bool,

        /// Filter by symbol type: class, function, method
        #[arg(short = 't', long = "type")]
        kind: Option<String>,

        /// Show symbols that call the target (callers)
        #[arg(long)]
        calls: bool,

        /// Show symbols that the target calls (callees)
        #[arg(long)]
        called_by: bool,

        /// Show only type definitions (class, struct, enum, interface, type alias)
        /// Filters out functions/methods for architectural overview
        #[arg(long)]
        types_only: bool,

        /// Focus view: show target at high detail, imports at signature level
        /// Resolves local imports and shows their skeletons inline
        /// Optionally filter to a specific module: --focus=models
        #[arg(long, value_name = "MODULE", num_args = 0..=1, default_missing_value = "*", require_equals = true)]
        focus: Option<String>,

        /// Resolve imports: inline signatures of specific imported symbols
        /// More targeted than --focus (shows only what's actually imported)
        #[arg(long)]
        resolve_imports: bool,

        /// Show all symbols including private ones (normally filtered by convention)
        #[arg(long)]
        all: bool,

        /// Show full source code (for symbols: complete implementation, for files: raw content)
        #[arg(long)]
        full: bool,
    },

    /// Edit a node in the codebase tree (structural code modification)
    Edit {
        /// Target to edit (path like src/main.py/Foo/bar)
        target: String,

        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,

        /// Delete the target node
        #[arg(long)]
        delete: bool,

        /// Replace the target node with new content
        #[arg(long)]
        replace: Option<String>,

        /// Insert content before the target node (sibling)
        #[arg(long)]
        before: Option<String>,

        /// Insert content after the target node (sibling)
        #[arg(long)]
        after: Option<String>,

        /// Insert content at the beginning of the target container
        #[arg(long)]
        prepend: Option<String>,

        /// Insert content at the end of the target container
        #[arg(long)]
        append: Option<String>,

        /// Move the target node before another node
        #[arg(long)]
        move_before: Option<String>,

        /// Move the target node after another node
        #[arg(long)]
        move_after: Option<String>,

        /// Copy the target node before another node
        #[arg(long)]
        copy_before: Option<String>,

        /// Copy the target node after another node
        #[arg(long)]
        copy_after: Option<String>,

        /// Move the target node to the beginning of a container
        #[arg(long)]
        move_prepend: Option<String>,

        /// Move the target node to the end of a container
        #[arg(long)]
        move_append: Option<String>,

        /// Copy the target node to the beginning of a container
        #[arg(long)]
        copy_prepend: Option<String>,

        /// Copy the target node to the end of a container
        #[arg(long)]
        copy_append: Option<String>,

        /// Swap the target node with another node
        #[arg(long)]
        swap: Option<String>,

        /// Dry run - show what would be changed without applying
        #[arg(long)]
        dry_run: bool,
    },

    /// Search for files/symbols matching a pattern
    SearchTree {
        /// Search query
        query: String,

        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,

        /// Limit results
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },

    /// Rebuild the file index
    Reindex {
        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,

        /// Also rebuild the call graph (slower, parses all files)
        #[arg(short, long)]
        call_graph: bool,
    },

    /// Show full source of a symbol
    Expand {
        /// Symbol and optional file (supports: "symbol", "file:symbol", "file symbol", "symbol file")
        #[arg(required = true)]
        args: Vec<String>,

        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,
    },

    /// List symbols in a file
    Symbols {
        /// File to analyze
        file: String,

        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,
    },

    /// Find what a symbol calls
    Callees {
        /// Symbol and optional file (supports: "symbol", "file:symbol", "file symbol", "symbol file")
        #[arg(required = true)]
        args: Vec<String>,

        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,
    },

    /// Find symbols that call a given symbol
    Callers {
        /// Symbol and optional file (supports: "symbol", "file:symbol", "file symbol", "symbol file")
        #[arg(required = true)]
        args: Vec<String>,

        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,
    },

    /// Show directory tree structure
    Tree {
        /// Directory to show (defaults to root)
        #[arg(default_value = ".")]
        path: String,

        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,

        /// Maximum depth to display
        #[arg(short, long)]
        depth: Option<usize>,

        /// Show only directories
        #[arg(short = 'D', long)]
        dirs_only: bool,
    },

    /// Show code skeleton (function/class signatures)
    Skeleton {
        /// File to extract skeleton from
        file: String,

        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,

        /// Include docstrings
        #[arg(short = 'd', long, default_value = "true")]
        docstrings: bool,
    },

    /// Generate compiled context (skeleton + deps + summary)
    Context {
        /// File to analyze
        file: String,

        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,
    },

    /// List code anchors (named code locations)
    Anchors {
        /// File to extract anchors from
        file: String,

        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,

        /// Filter anchors by name (fuzzy match)
        #[arg(short, long)]
        query: Option<String>,
    },

    /// Analyze variable scopes and bindings
    Scopes {
        /// File to analyze
        file: String,

        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,

        /// Line number to show bindings at
        #[arg(short, long)]
        line: Option<usize>,

        /// Find definition of a name at a line
        #[arg(short, long)]
        find: Option<String>,
    },

    /// Show module dependencies (imports and exports)
    Deps {
        /// File to analyze
        file: String,

        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,

        /// Show only imports
        #[arg(short, long)]
        imports_only: bool,

        /// Show only exports
        #[arg(short, long)]
        exports_only: bool,
    },

    /// Query imports from the index
    Imports {
        /// File to show imports for, or name to resolve
        query: String,

        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,

        /// Resolve a name in context of a file (what module does it come from?)
        #[arg(short = 'R', long)]
        resolve: bool,

        /// Show import graph (what this file imports and what imports it)
        #[arg(short, long)]
        graph: bool,

        /// Find files that import the given module
        #[arg(short, long)]
        who_imports: bool,
    },

    /// Calculate cyclomatic complexity
    Complexity {
        /// File to analyze
        file: String,

        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,

        /// Only show functions with complexity above threshold
        #[arg(short, long)]
        threshold: Option<usize>,
    },

    /// Show control flow graph
    Cfg {
        /// File to analyze
        file: String,

        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,

        /// Specific function to analyze
        #[arg(short, long)]
        function: Option<String>,
    },

    /// Manage the moss daemon
    Daemon {
        #[command(subcommand)]
        action: commands::daemon::DaemonAction,

        /// Root directory (defaults to current directory)
        #[arg(short, long, global = true)]
        root: Option<PathBuf>,
    },

    /// Check for and install updates
    Update {
        /// Check for updates without installing
        #[arg(short, long)]
        check: bool,
    },

    /// Show codebase health metrics
    Health {
        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,
    },

    /// Analyze codebase (unified health, complexity, security)
    Analyze {
        /// Target to analyze (path, file, or directory). Defaults to current directory.
        target: Option<String>,

        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,

        /// Run health analysis (codebase metrics)
        #[arg(long)]
        health: bool,

        /// Run complexity analysis (cyclomatic complexity)
        #[arg(long)]
        complexity: bool,

        /// Run security analysis (vulnerability scanning)
        #[arg(long)]
        security: bool,

        /// Complexity threshold - only show functions above this
        #[arg(short, long)]
        threshold: Option<usize>,

        /// Filter by symbol kind: function, method
        #[arg(long)]
        kind: Option<String>,
    },

    /// Show comprehensive codebase overview
    Overview {
        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,

        /// Compact one-line output
        #[arg(short, long)]
        compact: bool,
    },

    /// Summarize what a module does
    Summarize {
        /// File to summarize
        file: String,

        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,
    },

    /// Search for text patterns in files (fast ripgrep-based search)
    Grep {
        /// Regex pattern to search for
        pattern: String,

        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,

        /// Glob pattern to filter files (e.g., "*.py")
        #[arg(short, long)]
        glob: Option<String>,

        /// Maximum number of matches to return
        #[arg(short, long, default_value = "100")]
        limit: usize,

        /// Case-insensitive search
        #[arg(short = 'i', long)]
        ignore_case: bool,
    },

    /// Find symbols by name across the codebase
    FindSymbols {
        /// Symbol name to search for (supports partial matching with --fuzzy)
        name: String,

        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,

        /// Filter by kind: function, class, method
        #[arg(short, long)]
        kind: Option<String>,

        /// Enable fuzzy matching (default: true)
        #[arg(short, long, default_value = "true")]
        fuzzy: bool,

        /// Maximum number of results
        #[arg(short, long, default_value = "50")]
        limit: usize,
    },

    /// Show index statistics (DB size vs codebase size)
    IndexStats {
        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,
    },

    /// List indexed files (with optional prefix filter)
    ListFiles {
        /// Path prefix to filter (e.g., "src/moss" for files in that dir)
        prefix: Option<String>,

        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,

        /// Limit results
        #[arg(short, long, default_value = "1000")]
        limit: usize,
    },

    /// Index external packages (stdlib, site-packages) into global cache
    IndexPackages {
        /// Ecosystems to index (python, go, js, deno, java, cpp, rust). Defaults to all available.
        #[arg(long, value_delimiter = ',')]
        only: Vec<String>,

        /// Clear existing index before re-indexing
        #[arg(long)]
        clear: bool,

        /// Root directory for finding venv/node_modules (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,
    },
}

fn main() {
    let cli = Cli::parse();

    let exit_code = match cli.command {
        Commands::Path { query, root } => {
            cmd_path(&query, root.as_deref(), cli.json)
        }
        Commands::View {
            target,
            root,
            depth,
            line_numbers,
            deps,
            kind,
            calls,
            called_by,
            types_only,
            focus,
            resolve_imports,
            all,
            full,
        } => cmd_view(
            target.as_deref(),
            root.as_deref(),
            depth,
            line_numbers,
            deps,
            kind.as_deref(),
            calls,
            called_by,
            types_only,
            focus.as_deref(),
            resolve_imports,
            all,
            full,
            cli.json,
        ),
        Commands::SearchTree { query, root, limit } => {
            commands::search_tree::cmd_search_tree(&query, root.as_deref(), limit, cli.json)
        }
        Commands::Edit {
            target,
            root,
            delete,
            replace,
            before,
            after,
            prepend,
            append,
            move_before,
            move_after,
            copy_before,
            copy_after,
            move_prepend,
            move_append,
            copy_prepend,
            copy_append,
            swap,
            dry_run,
        } => commands::edit::cmd_edit(
            &target,
            root.as_deref(),
            delete,
            replace.as_deref(),
            before.as_deref(),
            after.as_deref(),
            prepend.as_deref(),
            append.as_deref(),
            move_before.as_deref(),
            move_after.as_deref(),
            copy_before.as_deref(),
            copy_after.as_deref(),
            move_prepend.as_deref(),
            move_append.as_deref(),
            copy_prepend.as_deref(),
            copy_append.as_deref(),
            swap.as_deref(),
            dry_run,
            cli.json,
        ),
        Commands::Reindex { root, call_graph } => commands::reindex::cmd_reindex(root.as_deref(), call_graph),
        Commands::Expand { args, root } => {
            let (symbol, file) = normalize_symbol_args(&args);
            cmd_expand(&symbol, file.as_deref(), root.as_deref(), cli.json)
        }
        Commands::Symbols { file, root } => {
            cmd_symbols(&file, root.as_deref(), cli.json)
        }
        Commands::Callees { args, root } => {
            let (symbol, file) = normalize_symbol_args(&args);
            commands::callees::cmd_callees(&symbol, file.as_deref(), root.as_deref(), cli.json)
        }
        Commands::Callers { args, root } => {
            let (symbol, _file) = normalize_symbol_args(&args);
            cmd_callers(&symbol, root.as_deref(), cli.json)
        }
        Commands::Tree {
            path,
            root,
            depth,
            dirs_only,
        } => cmd_tree(&path, root.as_deref(), depth, dirs_only, cli.json),
        Commands::Skeleton {
            file,
            root,
            docstrings,
        } => commands::skeleton::cmd_skeleton(&file, root.as_deref(), docstrings, cli.json),
        Commands::Context { file, root } => {
            commands::context::cmd_context(&file, root.as_deref(), cli.json)
        }
        Commands::Anchors { file, root, query } => {
            commands::anchors::cmd_anchors(&file, root.as_deref(), query.as_deref(), cli.json)
        }
        Commands::Scopes {
            file,
            root,
            line,
            find,
        } => commands::scopes::cmd_scopes(&file, root.as_deref(), line, find.as_deref(), cli.json),
        Commands::Deps {
            file,
            root,
            imports_only,
            exports_only,
        } => commands::deps_cmd::cmd_deps(&file, root.as_deref(), imports_only, exports_only, cli.json),
        Commands::Imports {
            query,
            root,
            resolve,
            graph,
            who_imports,
        } => commands::imports::cmd_imports(&query, root.as_deref(), resolve, graph, who_imports, cli.json),
        Commands::Complexity {
            file,
            root,
            threshold,
        } => commands::complexity::cmd_complexity(&file, root.as_deref(), threshold, cli.json),
        Commands::Cfg {
            file,
            root,
            function,
        } => commands::cfg::cmd_cfg(&file, root.as_deref(), function.as_deref(), cli.json),
        Commands::Daemon { action, root } => commands::daemon::cmd_daemon(action, root.as_deref(), cli.json),
        Commands::Update { check } => commands::update::cmd_update(check, cli.json),
        Commands::Health { root } => commands::health::cmd_health(root.as_deref(), cli.json),
        Commands::Analyze {
            target,
            root,
            health,
            complexity,
            security,
            threshold,
            kind,
        } => cmd_analyze(
            target.as_deref(),
            root.as_deref(),
            health,
            complexity,
            security,
            threshold,
            kind.as_deref(),
            cli.json,
        ),
        Commands::Overview { root, compact } => {
            commands::overview::cmd_overview(root.as_deref(), compact, cli.json)
        }
        Commands::Summarize { file, root } => cmd_summarize(&file, root.as_deref(), cli.json),
        Commands::Grep {
            pattern,
            root,
            glob,
            limit,
            ignore_case,
        } => commands::grep_cmd::cmd_grep(
            &pattern,
            root.as_deref(),
            glob.as_deref(),
            limit,
            ignore_case,
            cli.json,
        ),
        Commands::FindSymbols {
            name,
            root,
            kind,
            fuzzy,
            limit,
        } => cmd_find_symbols(
            &name,
            root.as_deref(),
            kind.as_deref(),
            fuzzy,
            limit,
            cli.json,
        ),
        Commands::IndexStats { root } => commands::index_stats::cmd_index_stats(root.as_deref(), cli.json),
        Commands::ListFiles { prefix, root, limit } => {
            commands::list_files::cmd_list_files(prefix.as_deref(), root.as_deref(), limit, cli.json)
        }
        Commands::IndexPackages { only, clear, root } => {
            commands::index_packages::cmd_index_packages(&only, clear, root.as_deref(), cli.json)
        }
    };

    std::process::exit(exit_code);
}

fn cmd_path(query: &str, root: Option<&Path>, json: bool) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    let client = daemon::DaemonClient::new(&root);

    // Try daemon first if available
    if client.is_available() {        if let Ok(matches) = client.path_query(query) {            if matches.is_empty() {
                if json {
                    println!("[]");
                } else {
                    eprintln!("No matches for: {}", query);
                }
                return 1;
            }
            if json {
                let output: Vec<_> = matches
                    .iter()
                    .map(|m| serde_json::json!({"path": m.path, "kind": m.kind}))
                    .collect();
                println!("{}", serde_json::to_string(&output).unwrap());
            } else {
                for m in &matches {
                    println!("{} ({})", m.path, m.kind);
                }
            }            return 0;
        }
        // Fall through to direct if daemon query failed
    } else {        // Auto-start daemon in background for future queries
        let client_clone = daemon::DaemonClient::new(&root);
        std::thread::spawn(move || {
            let _ = client_clone.ensure_running();
        });
    }

    // Direct path resolution
    let matches = path_resolve::resolve(query, &root);
    if matches.is_empty() {
        if json {
            println!("[]");
        } else {
            eprintln!("No matches for: {}", query);
        }
        return 1;
    }

    if json {
        let output: Vec<_> = matches
            .iter()
            .map(|m| serde_json::json!({"path": m.path, "kind": m.kind}))
            .collect();
        println!("{}", serde_json::to_string(&output).unwrap());
    } else {
        for m in &matches {
            println!("{} ({})", m.path, m.kind);
        }
    }

    0
}

fn cmd_view(
    target: Option<&str>,
    root: Option<&Path>,
    depth: i32,
    line_numbers: bool,
    show_deps: bool,
    kind_filter: Option<&str>,
    show_calls: bool,
    show_called_by: bool,
    types_only: bool,
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
        return cmd_view_calls(&root, target, show_calls, show_called_by, json);
    }

    // --focus requires a file target
    if focus.is_some() && target.is_none() {
        eprintln!("--focus requires a file target");
        return 1;
    }

    let target = target.unwrap_or(".");

    // Handle "." as current directory
    if target == "." {
        return cmd_view_directory(&root, &root, depth, json);
    }

    // Use unified path resolution
    let unified = match path_resolve::resolve_unified(target, &root) {
        Some(u) => u,
        None => {
            eprintln!("No matches for: {}", target);
            return 1;
        }
    };

    if unified.is_directory {
        // View directory
        cmd_view_directory(&root.join(&unified.file_path), &root, depth, json)
    } else if unified.symbol_path.is_empty() {
        // View file (--full overrides depth to show raw content)
        let effective_depth = if full { -1 } else { depth };
        cmd_view_file(&unified.file_path, &root, effective_depth, line_numbers, show_deps, types_only, focus, resolve_imports, show_all, json)
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

    let (_, call_count, _) = idx.call_graph_stats().unwrap_or((0, 0, 0));
    if call_count == 0 {
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
        for (file, sym, line, direction) in &results {
            println!("  {}:{}:{} ({})", file, line, sym, direction);
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
    let files_to_search: Vec<std::path::PathBuf> = if scope == "." {
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

fn cmd_view_directory(dir: &Path, root: &Path, depth: i32, json: bool) -> i32 {
    let effective_depth = if depth < 0 { None } else { Some(depth as usize) };
    let result = tree::generate_tree(dir, effective_depth, false);

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

            if show_deps && !deps.reexports.is_empty() {
                println!("\n## Re-exports");
                for reexp in &deps.reexports {
                    if reexp.is_star {
                        println!("  export * from '{}'", reexp.module);
                    } else {
                        println!("  export {{ {} }} from '{}'", reexp.names.join(", "), reexp.module);
                    }
                }
            }
        }

        if depth >= 1 {
            let formatted = skeleton_result.format(depth >= 2);
            if !formatted.is_empty() {
                println!("\n## Symbols");
                println!("{}", formatted);
            }
        }

        // Fisheye mode: show skeletons of imported files (local and external)
        // With --focus alone: show all imports
        // With --focus=module: filter to matching imports
        if let Some(focus_filter) = focus {
            // deps_result is guaranteed to be Some when focus is true
            let deps = deps_result.as_ref().unwrap();
            let filter_all = focus_filter == "*";

            // Collect resolved imports (optionally filtered)
            // Tuple: (module_name, resolved_path, display_path)
            let mut resolved: Vec<(String, PathBuf, String)> = Vec::new();
            for imp in &deps.imports {
                // Check if this import matches the filter
                let matches_filter = filter_all
                    || imp.module.contains(focus_filter)
                    || imp.module == focus_filter;

                if matches_filter {
                    if let Some(resolved_path) = resolve_import(&imp.module, &full_path, root) {
                        // For display: use relative path if within root, else use module name
                        let display = if let Ok(rel_path) = resolved_path.strip_prefix(root) {
                            rel_path.display().to_string()
                        } else {
                            // External package - show abbreviated path
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

                        let formatted = import_skeleton.format(false); // depth 1 = signatures only
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
                                        let reexp_display = reexp_path.strip_prefix(root)
                                            .map(|p| p.display().to_string())
                                            .unwrap_or_else(|_| format!("[{}]", reexp.module));
                                        let export_desc = if reexp.is_star {
                                            format!("export * from '{}'", reexp.module)
                                        } else {
                                            format!("export {{ {} }} from '{}'", reexp.names.join(", "), reexp.module)
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

            // Collect imports with specific names
            let mut resolved_symbols: Vec<(String, String, String)> = Vec::new(); // (module, name, signature)

            for imp in &deps.imports {
                if imp.names.is_empty() {
                    continue; // Skip bare "import x" statements
                }

                if let Some(resolved_path) = resolve_import(&imp.module, &full_path, root) {
                    if let Ok(import_content) = std::fs::read_to_string(&resolved_path) {
                        let mut import_extractor = skeleton::SkeletonExtractor::new();
                        let import_skeleton = import_extractor.extract(&resolved_path, &import_content);

                        // Find each imported name in the module's skeleton
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
        // Check children (for nested classes, methods, etc.)
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

fn cmd_expand(symbol: &str, file: Option<&str>, root: Option<&Path>, json: bool) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    let mut parser = symbols::SymbolParser::new();

    // If file is provided, search only that file
    let files_to_search: Vec<PathBuf> = if let Some(file_query) = file {
        let matches = path_resolve::resolve(file_query, &root);
        matches
            .into_iter()
            .filter(|m| m.kind == "file")
            .map(|m| root.join(&m.path))
            .collect()
    } else {
        // Search all files with language support
        path_resolve::all_files(&root)
            .into_iter()
            .filter(|m| m.kind == "file" && has_language_support(&m.path))
            .map(|m| root.join(&m.path))
            .collect()
    };

    for file_path in files_to_search {
        if let Ok(content) = std::fs::read_to_string(&file_path) {
            if let Some(source) = parser.extract_symbol_source(&file_path, &content, symbol) {
                let rel_path = file_path
                    .strip_prefix(&root)
                    .unwrap_or(&file_path)
                    .to_string_lossy();

                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "symbol": symbol,
                            "file": rel_path,
                            "source": source
                        })
                    );
                } else {
                    println!("# {}:{}", rel_path, symbol);
                    println!("{}", source);
                }
                return 0;
            }
        }
    }

    eprintln!("Symbol not found: {}", symbol);
    1
}

fn cmd_symbols(file: &str, root: Option<&Path>, json: bool) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // Resolve the file
    let matches = path_resolve::resolve(file, &root);    let file_match = match matches.iter().find(|m| m.kind == "file") {
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

    let parser = symbols::SymbolParser::new();
    let symbols = parser.parse_file(&file_path, &content);

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
                file_match.path,
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

fn cmd_callers(symbol: &str, root: Option<&Path>, json: bool) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // Try index first (fast path)
    if let Ok(idx) = index::FileIndex::open(&root) {

        // Check if call graph is populated
        let (_, calls, _) = idx.call_graph_stats().unwrap_or((0, 0, 0));
        if calls > 0 {
            // Index is populated - use it exclusively (don't fall back to slow scan)
            if let Ok(callers) = idx.find_callers(symbol) {
                if !callers.is_empty() {
                    if json {
                        let output: Vec<_> = callers
                            .iter()
                            .map(|(file, sym, line)| serde_json::json!({"file": file, "symbol": sym, "line": line}))
                            .collect();
                        println!("{}", serde_json::to_string(&output).unwrap());
                    } else {
                        println!("Callers of {}:", symbol);
                        for (file, sym, line) in &callers {
                            println!("  {}:{}:{}", file, line, sym);
                        }
                    }
                    return 0;
                }
            }
            // Index populated but no results - symbol not called anywhere
            eprintln!(
                "No callers found for: {} (index has {} calls)",
                symbol, calls
            );
            return 1;
        }
    }

    // Index empty or stale - auto-reindex (incremental is faster)
    eprintln!("Call graph not indexed. Building now...");

    if let Ok(mut idx) = index::FileIndex::open(&root) {
        // Ensure file index is populated first
        if idx.needs_refresh() {
            if let Err(e) = idx.incremental_refresh() {
                eprintln!("Failed to refresh file index: {}", e);
                return 1;
            }
        }

        // Now build call graph (incremental uses mtime to skip unchanged files)
        match idx.incremental_call_graph_refresh() {
            Ok((symbols, calls, imports)) => {
                eprintln!(
                    "Indexed {} symbols, {} calls, {} imports",
                    symbols, calls, imports
                );

                // Retry the query
                if let Ok(callers) = idx.find_callers(symbol) {
                    if !callers.is_empty() {
                        if json {
                            let output: Vec<_> = callers
                                .iter()
                                .map(|(file, sym, line)| serde_json::json!({"file": file, "symbol": sym, "line": line}))
                                .collect();
                            println!("{}", serde_json::to_string(&output).unwrap());
                        } else {
                            println!("Callers of {}:", symbol);
                            for (file, sym, line) in &callers {
                                println!("  {}:{}:{}", file, line, sym);
                            }
                        }
                        return 0;
                    }
                }
                eprintln!("No callers found for: {}", symbol);
                return 1;
            }
            Err(e) => {
                eprintln!("Failed to build call graph: {}", e);
                return 1;
            }
        }
    }

    eprintln!("Failed to open index");
    1
}

fn cmd_tree(
    path: &str,
    root: Option<&Path>,
    depth: Option<usize>,
    dirs_only: bool,
    json: bool,
) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // Resolve the path using unified resolution (handles trailing slashes)
    let target_dir = if path == "." {
        root.clone()
    } else {
        match path_resolve::resolve_unified(path, &root) {
            Some(u) if u.is_directory => root.join(&u.file_path),
            _ => {
                // Maybe it's an exact path
                let exact = root.join(path);
                if exact.is_dir() {
                    exact
                } else {
                    eprintln!("Directory not found: {}", path);
                    return 1;
                }
            }
        }
    };

    let result = tree::generate_tree(&target_dir, depth, dirs_only);

    if json {
        println!(
            "{}",
            serde_json::json!({
                "root": result.root_name,
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

/// Check if a file has language support (symbols can be extracted)
fn has_language_support(path: &str) -> bool {
    support_for_path(Path::new(path))
        .map(|lang| lang.has_symbols())
        .unwrap_or(false)
}

fn cmd_analyze(
    target: Option<&str>,
    root: Option<&Path>,
    health: bool,
    complexity: bool,
    security: bool,
    threshold: Option<usize>,
    kind_filter: Option<&str>,
    json: bool,
) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // If no specific flags, run all analyses
    let any_flag = health || complexity || security;
    let (run_health, run_complexity, run_security) = if !any_flag {
        (true, true, true)
    } else {
        (health, complexity, security)
    };

    let report = analyze::analyze(
        target,
        &root,
        run_health,
        run_complexity,
        run_security,
        threshold,
        kind_filter,
    );

    if json {
        println!("{}", report.to_json());
    } else {
        println!("{}", report.format());
    }

    0
}

fn cmd_summarize(file: &str, root: Option<&Path>, json: bool) -> i32 {
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

    let summary = summarize::summarize_module(&file_path, &content);

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
                "file": file_match.path,
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

fn cmd_find_symbols(
    name: &str,
    root: Option<&Path>,
    kind: Option<&str>,
    fuzzy: bool,
    limit: usize,
    json: bool,
) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // Open or create index
    let idx = match index::FileIndex::open(&root) {
        Ok(idx) => idx,
        Err(e) => {
            eprintln!("Failed to open index: {}", e);
            return 1;
        }
    };

    // Check if call graph is populated (symbols are indexed there)
    let (symbol_count, _, _) = idx.call_graph_stats().unwrap_or((0, 0, 0));
    if symbol_count == 0 {
        eprintln!("Symbol index empty. Run: moss reindex --call-graph");
        return 1;
    }

    // Query symbols
    match idx.find_symbols(name, kind, fuzzy, limit) {
        Ok(symbols) => {

            if symbols.is_empty() {
                if json {
                    println!("[]");
                } else {
                    eprintln!("No symbols found matching: {}", name);
                }
                return 1;
            }

            if json {
                let output: Vec<_> = symbols
                    .iter()
                    .map(|(sym_name, sym_kind, file, start, end, parent)| {
                        serde_json::json!({
                            "name": sym_name,
                            "kind": sym_kind,
                            "file": file,
                            "line": start,
                            "end_line": end,
                            "parent": parent
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string(&output).unwrap());
            } else {
                for (sym_name, sym_kind, file, start, _end, parent) in &symbols {
                    let parent_str = parent
                        .as_ref()
                        .map(|p| format!(" (in {})", p))
                        .unwrap_or_default();
                    println!("{}:{} {} {}{}", file, start, sym_kind, sym_name, parent_str);
                }
            }
            0
        }
        Err(e) => {
            eprintln!("Query failed: {}", e);
            1
        }
    }
}
