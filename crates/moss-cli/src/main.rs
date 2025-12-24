use clap::{Parser, Subcommand};
use moss_core::get_moss_dir;
use moss_languages::external_packages;
use moss_languages::Language;
use std::path::{Path, PathBuf};
use std::time::Instant;

mod analyze;
mod anchors;
mod cfg;
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

/// Simple profiler for timing breakdown
struct Profiler {
    start: Instant,
    events: Vec<(String, std::time::Duration)>,
    enabled: bool,
}

impl Profiler {
    fn new(enabled: bool) -> Self {
        Self {
            start: Instant::now(),
            events: Vec::new(),
            enabled,
        }
    }

    fn mark(&mut self, name: &str) {
        if self.enabled {
            self.events.push((name.to_string(), self.start.elapsed()));
        }
    }

    fn print(&self) {
        if !self.enabled || self.events.is_empty() {
            return;
        }
        eprintln!("\n--- Timing ---");
        let mut prev = std::time::Duration::ZERO;
        for (name, elapsed) in &self.events {
            let delta = *elapsed - prev;
            eprintln!(
                "  {:20} {:>8.2}ms (+{:.2}ms)",
                name,
                elapsed.as_secs_f64() * 1000.0,
                delta.as_secs_f64() * 1000.0
            );
            prev = *elapsed;
        }
        eprintln!(
            "  {:20} {:>8.2}ms",
            "total",
            self.start.elapsed().as_secs_f64() * 1000.0
        );
    }
}

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

    /// Show timing breakdown
    #[arg(long, global = true)]
    profile: bool,
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
        action: DaemonAction,

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

        /// Run test coverage analysis (detect missing tests)
        #[arg(long)]
        test_coverage: bool,

        /// Run scopes analysis (public/private symbol statistics)
        #[arg(long)]
        scopes: bool,

        /// Run test health analysis (pytest markers, skip reasons)
        #[arg(long)]
        test_health: bool,

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

#[derive(Subcommand)]
enum DaemonAction {
    /// Show daemon status
    Status,

    /// Shutdown the daemon
    Shutdown,

    /// Start the daemon (background)
    Start,

    /// Run the daemon in foreground (for debugging)
    Run,
}

fn main() {
    let cli = Cli::parse();
    let mut profiler = Profiler::new(cli.profile);
    profiler.mark("parsed_args");

    let exit_code = match cli.command {
        Commands::Path { query, root } => {
            cmd_path(&query, root.as_deref(), cli.json, &mut profiler)
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
            cmd_search_tree(&query, root.as_deref(), limit, cli.json)
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
        } => cmd_edit(
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
        Commands::Reindex { root, call_graph } => cmd_reindex(root.as_deref(), call_graph),
        Commands::Expand { args, root } => {
            let (symbol, file) = normalize_symbol_args(&args);
            cmd_expand(&symbol, file.as_deref(), root.as_deref(), cli.json)
        }
        Commands::Symbols { file, root } => {
            cmd_symbols(&file, root.as_deref(), cli.json, &mut profiler)
        }
        Commands::Callees { args, root } => {
            let (symbol, file) = normalize_symbol_args(&args);
            cmd_callees(&symbol, file.as_deref(), root.as_deref(), cli.json)
        }
        Commands::Callers { args, root } => {
            let (symbol, _file) = normalize_symbol_args(&args);
            cmd_callers(&symbol, root.as_deref(), cli.json, &mut profiler)
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
        } => cmd_skeleton(&file, root.as_deref(), docstrings, cli.json, &mut profiler),
        Commands::Context { file, root } => {
            cmd_context(&file, root.as_deref(), cli.json, &mut profiler)
        }
        Commands::Anchors { file, root, query } => {
            cmd_anchors(&file, root.as_deref(), query.as_deref(), cli.json)
        }
        Commands::Scopes {
            file,
            root,
            line,
            find,
        } => cmd_scopes(&file, root.as_deref(), line, find.as_deref(), cli.json),
        Commands::Deps {
            file,
            root,
            imports_only,
            exports_only,
        } => cmd_deps(&file, root.as_deref(), imports_only, exports_only, cli.json),
        Commands::Imports {
            query,
            root,
            resolve,
            graph,
            who_imports,
        } => cmd_imports(&query, root.as_deref(), resolve, graph, who_imports, cli.json),
        Commands::Complexity {
            file,
            root,
            threshold,
        } => cmd_complexity(&file, root.as_deref(), threshold, cli.json),
        Commands::Cfg {
            file,
            root,
            function,
        } => cmd_cfg(&file, root.as_deref(), function.as_deref(), cli.json),
        Commands::Daemon { action, root } => cmd_daemon(action, root.as_deref(), cli.json),
        Commands::Update { check } => cmd_update(check, cli.json),
        Commands::Health { root } => cmd_health(root.as_deref(), cli.json, &mut profiler),
        Commands::Analyze {
            target,
            root,
            health,
            complexity,
            security,
            test_coverage,
            scopes,
            test_health,
            threshold,
            kind,
        } => cmd_analyze(
            target.as_deref(),
            root.as_deref(),
            health,
            complexity,
            security,
            test_coverage,
            scopes,
            test_health,
            threshold,
            kind.as_deref(),
            cli.json,
        ),
        Commands::Overview { root, compact } => {
            cmd_overview(root.as_deref(), compact, cli.json, &mut profiler)
        }
        Commands::Summarize { file, root } => cmd_summarize(&file, root.as_deref(), cli.json),
        Commands::Grep {
            pattern,
            root,
            glob,
            limit,
            ignore_case,
        } => cmd_grep(
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
            &mut profiler,
        ),
        Commands::IndexStats { root } => cmd_index_stats(root.as_deref(), cli.json),
        Commands::ListFiles { prefix, root, limit } => {
            cmd_list_files(prefix.as_deref(), root.as_deref(), limit, cli.json)
        }
        Commands::IndexPackages { only, clear, root } => {
            cmd_index_packages(&only, clear, root.as_deref(), cli.json)
        }
    };

    profiler.mark("done");
    profiler.print();
    std::process::exit(exit_code);
}

fn cmd_path(query: &str, root: Option<&Path>, json: bool, profiler: &mut Profiler) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    profiler.mark("resolved_root");

    let client = daemon::DaemonClient::new(&root);

    // Try daemon first if available
    if client.is_available() {
        profiler.mark("daemon_check");
        if let Ok(matches) = client.path_query(query) {
            profiler.mark("daemon_query");
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
            profiler.mark("output");
            return 0;
        }
        // Fall through to direct if daemon query failed
    } else {
        profiler.mark("no_daemon");
        // Auto-start daemon in background for future queries
        let client_clone = daemon::DaemonClient::new(&root);
        std::thread::spawn(move || {
            let _ = client_clone.ensure_running();
        });
    }

    // Direct path resolution
    let matches = path_resolve::resolve(query, &root);
    profiler.mark("path_resolve");

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

/// Edit a node in the codebase tree
#[allow(clippy::too_many_arguments)]
fn cmd_edit(
    target: &str,
    root: Option<&Path>,
    delete: bool,
    replace: Option<&str>,
    before: Option<&str>,
    after: Option<&str>,
    prepend: Option<&str>,
    append: Option<&str>,
    move_before: Option<&str>,
    move_after: Option<&str>,
    copy_before: Option<&str>,
    copy_after: Option<&str>,
    move_prepend: Option<&str>,
    move_append: Option<&str>,
    copy_prepend: Option<&str>,
    copy_append: Option<&str>,
    swap: Option<&str>,
    dry_run: bool,
    json: bool,
) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // Count operations to ensure exactly one is specified
    let ops = [
        delete,
        replace.is_some(),
        before.is_some(),
        after.is_some(),
        prepend.is_some(),
        append.is_some(),
        move_before.is_some(),
        move_after.is_some(),
        copy_before.is_some(),
        copy_after.is_some(),
        move_prepend.is_some(),
        move_append.is_some(),
        copy_prepend.is_some(),
        copy_append.is_some(),
        swap.is_some(),
    ];
    let op_count = ops.iter().filter(|&&x| x).count();

    if op_count == 0 {
        eprintln!("Error: No operation specified. Use --delete, --replace, --before, --after, --prepend, --append, --move-*, --copy-*, or --swap");
        return 1;
    }
    if op_count > 1 {
        eprintln!("Error: Only one operation can be specified at a time");
        return 1;
    }

    // Resolve the target path
    let unified = match path_resolve::resolve_unified(target, &root) {
        Some(u) => u,
        None => {
            eprintln!("No matches for: {}", target);
            return 1;
        }
    };

    // We need a file path (cannot edit directories)
    if unified.is_directory {
        eprintln!("Cannot edit a directory: {}", target);
        return 1;
    }

    let file_path = root.join(&unified.file_path);
    let content = match std::fs::read_to_string(&file_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file: {}", e);
            return 1;
        }
    };

    let mut editor = edit::Editor::new();

    // Handle file-level operations (prepend/append without a symbol)
    if unified.symbol_path.is_empty() {
        // File-level operations
        let new_content = if let Some(content_to_prepend) = prepend {
            editor.prepend_to_file(&content, content_to_prepend)
        } else if let Some(content_to_append) = append {
            editor.append_to_file(&content, content_to_append)
        } else {
            eprintln!("Error: --delete, --replace, --before, --after require a symbol target");
            eprintln!("Hint: Use a path like 'src/foo.py/MyClass' to target a symbol");
            return 1;
        };

        if dry_run {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "dry_run": true,
                        "file": unified.file_path,
                        "operation": if prepend.is_some() { "prepend" } else { "append" },
                        "new_content": new_content
                    })
                );
            } else {
                println!("--- Dry run: {} ---", unified.file_path);
                println!("{}", new_content);
            }
            return 0;
        }

        if let Err(e) = std::fs::write(&file_path, &new_content) {
            eprintln!("Error writing file: {}", e);
            return 1;
        }

        if json {
            println!(
                "{}",
                serde_json::json!({
                    "success": true,
                    "file": unified.file_path,
                    "operation": if prepend.is_some() { "prepend" } else { "append" }
                })
            );
        } else {
            println!(
                "{}: {}",
                if prepend.is_some() { "Prepended to" } else { "Appended to" },
                unified.file_path
            );
        }
        return 0;
    }

    // Symbol-level operations
    let symbol_name = unified.symbol_path.last().unwrap();
    let loc = match editor.find_symbol(&file_path, &content, symbol_name) {
        Some(l) => l,
        None => {
            eprintln!("Symbol not found: {}", symbol_name);
            return 1;
        }
    };

    let (operation, new_content) = if delete {
        ("delete", editor.delete_symbol(&content, &loc))
    } else if let Some(new_code) = replace {
        ("replace", editor.replace_symbol(&content, &loc, new_code))
    } else if let Some(code) = before {
        ("insert_before", editor.insert_before(&content, &loc, code))
    } else if let Some(code) = after {
        ("insert_after", editor.insert_after(&content, &loc, code))
    } else if let Some(code) = prepend {
        // Prepend inside a container (class/impl)
        let body = match editor.find_container_body(&file_path, &content, symbol_name) {
            Some(b) => b,
            None => {
                eprintln!("Error: '{}' is not a container (class/impl)", symbol_name);
                eprintln!("Hint: --prepend works on classes and impl blocks");
                return 1;
            }
        };
        ("prepend", editor.prepend_to_container(&content, &body, code))
    } else if let Some(code) = append {
        // Append inside a container (class/impl)
        let body = match editor.find_container_body(&file_path, &content, symbol_name) {
            Some(b) => b,
            None => {
                eprintln!("Error: '{}' is not a container (class/impl)", symbol_name);
                eprintln!("Hint: --append works on classes and impl blocks");
                return 1;
            }
        };
        ("append", editor.append_to_container(&content, &body, code))
    } else if let Some(dest) = move_before {
        // Move operation: delete from source, insert before destination
        // First verify destination exists
        let _dest_loc = match editor.find_symbol(&file_path, &content, dest) {
            Some(l) => l,
            None => {
                eprintln!("Destination symbol not found: {}", dest);
                return 1;
            }
        };
        let source_content = &content[loc.start_byte..loc.end_byte];
        let without_source = editor.delete_symbol(&content, &loc);
        // Re-find destination after deletion (location may have shifted)
        let dest_loc_adjusted = match editor.find_symbol(&file_path, &without_source, dest) {
            Some(l) => l,
            None => {
                eprintln!("Destination symbol not found after deletion: {}", dest);
                return 1;
            }
        };
        ("move_before", editor.insert_before(&without_source, &dest_loc_adjusted, source_content))
    } else if let Some(dest) = move_after {
        // First verify destination exists
        let _dest_loc = match editor.find_symbol(&file_path, &content, dest) {
            Some(l) => l,
            None => {
                eprintln!("Destination symbol not found: {}", dest);
                return 1;
            }
        };
        let source_content = &content[loc.start_byte..loc.end_byte];
        let without_source = editor.delete_symbol(&content, &loc);
        // Re-find destination after deletion (location may have shifted)
        let dest_loc_adjusted = match editor.find_symbol(&file_path, &without_source, dest) {
            Some(l) => l,
            None => {
                eprintln!("Destination symbol not found after deletion: {}", dest);
                return 1;
            }
        };
        ("move_after", editor.insert_after(&without_source, &dest_loc_adjusted, source_content))
    } else if let Some(dest) = copy_before {
        // Copy operation: insert copy before destination (keep original)
        let dest_loc = match editor.find_symbol(&file_path, &content, dest) {
            Some(l) => l,
            None => {
                eprintln!("Destination symbol not found: {}", dest);
                return 1;
            }
        };
        let source_content = &content[loc.start_byte..loc.end_byte];
        ("copy_before", editor.insert_before(&content, &dest_loc, source_content))
    } else if let Some(dest) = copy_after {
        // Copy operation: insert copy after destination (keep original)
        let dest_loc = match editor.find_symbol(&file_path, &content, dest) {
            Some(l) => l,
            None => {
                eprintln!("Destination symbol not found: {}", dest);
                return 1;
            }
        };
        let source_content = &content[loc.start_byte..loc.end_byte];
        ("copy_after", editor.insert_after(&content, &dest_loc, source_content))
    } else if let Some(container) = move_prepend {
        // Move to beginning of container
        // First verify container exists
        let _body = match editor.find_container_body(&file_path, &content, container) {
            Some(b) => b,
            None => {
                eprintln!("Container not found: {}", container);
                return 1;
            }
        };
        let source_content = content[loc.start_byte..loc.end_byte].to_string();
        let without_source = editor.delete_symbol(&content, &loc);
        // Re-find container body after deletion (location may have shifted)
        let body = match editor.find_container_body(&file_path, &without_source, container) {
            Some(b) => b,
            None => {
                eprintln!("Container not found after deletion: {}", container);
                return 1;
            }
        };
        ("move_prepend", editor.prepend_to_container(&without_source, &body, &source_content))
    } else if let Some(container) = move_append {
        // Move to end of container
        // First verify container exists
        let _body = match editor.find_container_body(&file_path, &content, container) {
            Some(b) => b,
            None => {
                eprintln!("Container not found: {}", container);
                return 1;
            }
        };
        let source_content = content[loc.start_byte..loc.end_byte].to_string();
        let without_source = editor.delete_symbol(&content, &loc);
        // Re-find container body after deletion (location may have shifted)
        let body = match editor.find_container_body(&file_path, &without_source, container) {
            Some(b) => b,
            None => {
                eprintln!("Container not found after deletion: {}", container);
                return 1;
            }
        };
        ("move_append", editor.append_to_container(&without_source, &body, &source_content))
    } else if let Some(container) = copy_prepend {
        // Copy to beginning of container
        let body = match editor.find_container_body(&file_path, &content, container) {
            Some(b) => b,
            None => {
                eprintln!("Container not found: {}", container);
                return 1;
            }
        };
        let source_content = &content[loc.start_byte..loc.end_byte];
        ("copy_prepend", editor.prepend_to_container(&content, &body, source_content))
    } else if let Some(container) = copy_append {
        // Copy to end of container
        let body = match editor.find_container_body(&file_path, &content, container) {
            Some(b) => b,
            None => {
                eprintln!("Container not found: {}", container);
                return 1;
            }
        };
        let source_content = &content[loc.start_byte..loc.end_byte];
        ("copy_append", editor.append_to_container(&content, &body, source_content))
    } else if let Some(other) = swap {
        let other_loc = match editor.find_symbol(&file_path, &content, other) {
            Some(l) => l,
            None => {
                eprintln!("Other symbol not found: {}", other);
                return 1;
            }
        };
        // Swap: get both contents, then replace in order (handle offsets)
        let (first_loc, second_loc) = if loc.start_byte < other_loc.start_byte {
            (&loc, &other_loc)
        } else {
            (&other_loc, &loc)
        };
        let first_content = content[first_loc.start_byte..first_loc.end_byte].to_string();
        let second_content = content[second_loc.start_byte..second_loc.end_byte].to_string();

        // Build new content by replacing second first (to preserve offsets), then first
        let mut new = content.clone();
        new.replace_range(second_loc.start_byte..second_loc.end_byte, &first_content);
        new.replace_range(first_loc.start_byte..first_loc.end_byte, &second_content);
        ("swap", new)
    } else {
        eprintln!("Error: No valid operation");
        return 1;
    };

    if dry_run {
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "dry_run": true,
                    "file": unified.file_path,
                    "symbol": symbol_name,
                    "operation": operation,
                    "new_content": new_content
                })
            );
        } else {
            println!("--- Dry run: {} on {} ---", operation, symbol_name);
            println!("{}", new_content);
        }
        return 0;
    }

    if let Err(e) = std::fs::write(&file_path, &new_content) {
        eprintln!("Error writing file: {}", e);
        return 1;
    }

    if json {
        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "file": unified.file_path,
                "symbol": symbol_name,
                "operation": operation
            })
        );
    } else {
        println!("{}: {} in {}", operation, symbol_name, unified.file_path);
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
            .filter(|m| m.kind == "file" && (m.path.ends_with(".py") || m.path.ends_with(".rs")))
            .map(|m| root.join(&m.path))
            .collect()
    } else {
        // Resolve scope
        let matches = path_resolve::resolve(scope, root);
        matches
            .into_iter()
            .filter(|m| m.kind == "file" && (m.path.ends_with(".py") || m.path.ends_with(".rs")))
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

fn cmd_search_tree(query: &str, root: Option<&Path>, limit: usize, json: bool) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // Use fuzzy matching to find all matches
    let matches = path_resolve::resolve(query, &root);
    let total = matches.len();

    // For extension patterns, use higher limit unless explicitly set
    let effective_limit = if query.starts_with('.') && limit == 20 {
        500 // Default higher limit for extension searches
    } else {
        limit
    };

    let limited: Vec<_> = matches.into_iter().take(effective_limit).collect();

    if json {
        let output: Vec<_> = limited
            .iter()
            .map(|m| serde_json::json!({"path": m.path, "kind": m.kind, "score": m.score}))
            .collect();
        println!("{}", serde_json::to_string(&output).unwrap());
    } else {
        for m in &limited {
            println!("{} ({})", m.path, m.kind);
        }
        if total > effective_limit {
            println!("... +{} more (use -l to show more)", total - effective_limit);
        }
    }

    0
}

fn cmd_reindex(root: Option<&Path>, call_graph: bool) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    match index::FileIndex::open(&root) {
        Ok(mut idx) => {
            match idx.refresh() {
                Ok(count) => {
                    println!("Indexed {} files", count);

                    // Optionally rebuild call graph
                    if call_graph {
                        match idx.refresh_call_graph() {
                            Ok((symbols, calls, imports)) => {
                                println!(
                                    "Indexed {} symbols, {} calls, {} imports",
                                    symbols, calls, imports
                                );
                            }
                            Err(e) => {
                                eprintln!("Error indexing call graph: {}", e);
                                return 1;
                            }
                        }
                    }
                    0
                }
                Err(e) => {
                    eprintln!("Error refreshing index: {}", e);
                    1
                }
            }
        }
        Err(e) => {
            eprintln!("Error opening index: {}", e);
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
        // Search all Python/Rust files
        path_resolve::all_files(&root)
            .into_iter()
            .filter(|m| m.kind == "file" && (m.path.ends_with(".py") || m.path.ends_with(".rs")))
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

fn cmd_symbols(file: &str, root: Option<&Path>, json: bool, profiler: &mut Profiler) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // Resolve the file
    let matches = path_resolve::resolve(file, &root);
    profiler.mark("path_resolve");
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
    profiler.mark("read_file");

    let parser = symbols::SymbolParser::new();
    let symbols = parser.parse_file(&file_path, &content);
    profiler.mark("parse_symbols");

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

fn cmd_callees(symbol: &str, file: Option<&str>, root: Option<&Path>, json: bool) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // Try index first (fast path)
    if let Ok(idx) = index::FileIndex::open(&root) {
        let (_, calls, _) = idx.call_graph_stats().unwrap_or((0, 0, 0));
        if calls > 0 {
            // Determine file path
            let file_path = if let Some(file) = file {
                // Resolve provided file
                let matches = path_resolve::resolve(file, &root);
                matches
                    .iter()
                    .find(|m| m.kind == "file")
                    .map(|m| m.path.clone())
            } else {
                // Find file from symbol
                idx.find_symbol(symbol)
                    .ok()
                    .and_then(|syms| syms.first().map(|(f, _, _, _)| f.clone()))
            };

            if let Some(file_path) = file_path {
                if let Ok(callees) = idx.find_callees(&file_path, symbol) {
                    if !callees.is_empty() {
                        if json {
                            let output: Vec<_> = callees
                                .iter()
                                .map(|(name, line)| serde_json::json!({"name": name, "file": file_path, "line": line}))
                                .collect();
                            println!("{}", serde_json::to_string(&output).unwrap());
                        } else {
                            println!("Callees of {}:", symbol);
                            for (name, line) in &callees {
                                println!("  {}:{}:{}", file_path, line, name);
                            }
                        }
                        return 0;
                    }
                }
            }
            eprintln!(
                "No callees found for: {} (index has {} calls)",
                symbol, calls
            );
            return 1;
        }
    }

    // Fallback to parsing (slower) - also auto-indexes like callers
    eprintln!("Call graph not indexed. Building now (one-time)...");

    if let Ok(mut idx) = index::FileIndex::open(&root) {
        if idx.needs_refresh() {
            if let Err(e) = idx.incremental_refresh() {
                eprintln!("Failed to refresh file index: {}", e);
                return 1;
            }
        }
        match idx.incremental_call_graph_refresh() {
            Ok((symbols, calls, imports)) => {
                if symbols > 0 || calls > 0 || imports > 0 {
                    eprintln!(
                        "Indexed {} symbols, {} calls, {} imports",
                        symbols, calls, imports
                    );
                }

                // Retry with index
                let file_path = if let Some(file) = file {
                    let matches = path_resolve::resolve(file, &root);
                    matches
                        .iter()
                        .find(|m| m.kind == "file")
                        .map(|m| m.path.clone())
                } else {
                    idx.find_symbol(symbol)
                        .ok()
                        .and_then(|syms| syms.first().map(|(f, _, _, _)| f.clone()))
                };

                if let Some(file_path) = file_path {
                    if let Ok(callees) = idx.find_callees(&file_path, symbol) {
                        if !callees.is_empty() {
                            if json {
                                let output: Vec<_> = callees
                                    .iter()
                                    .map(|(name, line)| serde_json::json!({"name": name, "file": file_path, "line": line}))
                                    .collect();
                                println!("{}", serde_json::to_string(&output).unwrap());
                            } else {
                                println!("Callees of {}:", symbol);
                                for (name, line) in &callees {
                                    println!("  {}:{}:{}", file_path, line, name);
                                }
                            }
                            return 0;
                        }
                    }
                }
                eprintln!("No callees found for: {}", symbol);
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

fn cmd_callers(symbol: &str, root: Option<&Path>, json: bool, profiler: &mut Profiler) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // Try index first (fast path)
    if let Ok(idx) = index::FileIndex::open(&root) {
        profiler.mark("open_index");

        // Check if call graph is populated
        let (_, calls, _) = idx.call_graph_stats().unwrap_or((0, 0, 0));
        if calls > 0 {
            // Index is populated - use it exclusively (don't fall back to slow scan)
            if let Ok(callers) = idx.find_callers(symbol) {
                profiler.mark("index_query");
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
    profiler.mark("index_miss");

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
        profiler.mark("file_index");

        // Now build call graph (incremental uses mtime to skip unchanged files)
        match idx.incremental_call_graph_refresh() {
            Ok((symbols, calls, imports)) => {
                eprintln!(
                    "Indexed {} symbols, {} calls, {} imports",
                    symbols, calls, imports
                );
                profiler.mark("call_graph");

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

fn cmd_skeleton(
    file: &str,
    root: Option<&Path>,
    include_docstrings: bool,
    json: bool,
    profiler: &mut Profiler,
) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // Resolve the file
    let matches = path_resolve::resolve(file, &root);
    profiler.mark("path_resolve");
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
    profiler.mark("read_file");

    let mut extractor = skeleton::SkeletonExtractor::new();
    let result = extractor.extract(&file_path, &content);
    profiler.mark("extract_skeleton");

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

        let symbols: Vec<_> = result.symbols.iter().map(symbol_to_json).collect();
        println!(
            "{}",
            serde_json::json!({
                "file": file_match.path,
                "symbols": symbols
            })
        );
    } else {
        let formatted = result.format(include_docstrings);
        if formatted.is_empty() {
            println!("# {} (no symbols)", file_match.path);
        } else {
            println!("# {}", file_match.path);
            println!("{}", formatted);
        }
    }

    0
}

fn cmd_context(file: &str, root: Option<&Path>, json: bool, profiler: &mut Profiler) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // Resolve the file
    let matches = path_resolve::resolve(file, &root);
    profiler.mark("path_resolve");
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
    profiler.mark("read_file");

    let line_count = content.lines().count();

    // Extract skeleton
    let mut skeleton_extractor = skeleton::SkeletonExtractor::new();
    let skeleton_result = skeleton_extractor.extract(&file_path, &content);
    profiler.mark("extract_skeleton");

    // Extract deps
    let deps_extractor = deps::DepsExtractor::new();
    let deps_result = deps_extractor.extract(&file_path, &content);
    profiler.mark("extract_deps");

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

fn cmd_anchors(file: &str, root: Option<&Path>, query: Option<&str>, json: bool) -> i32 {
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

fn cmd_scopes(
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

fn cmd_deps(
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

fn cmd_imports(
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

/// Convert a file path to a Python module name
/// e.g., "src/moss/gen/serialize.py" -> "moss.gen.serialize"
fn file_path_to_module(file_path: &str) -> Option<String> {
    let path = std::path::Path::new(file_path);
    let ext = path.extension()?.to_str()?;

    // Only Python files for now
    if ext != "py" {
        return None;
    }

    // Remove extension and common prefixes
    let stem = path.with_extension("");
    let stem_str = stem.to_str()?;

    // Strip common source directory prefixes
    let module_path = stem_str
        .strip_prefix("src/")
        .or_else(|| stem_str.strip_prefix("lib/"))
        .unwrap_or(stem_str);

    // Handle __init__.py - use parent directory as module
    let module_path = if module_path.ends_with("/__init__") {
        module_path.strip_suffix("/__init__")?
    } else {
        module_path
    };

    // Convert path separators to dots
    Some(module_path.replace('/', "."))
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

fn cmd_complexity(file: &str, root: Option<&Path>, threshold: Option<usize>, json: bool) -> i32 {
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

fn cmd_cfg(file: &str, root: Option<&Path>, function: Option<&str>, json: bool) -> i32 {
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

fn cmd_daemon(action: DaemonAction, root: Option<&Path>, json: bool) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    let client = daemon::DaemonClient::new(&root);

    let moss_dir = get_moss_dir(&root);
    match action {
        DaemonAction::Status => {
            if !client.is_available() {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "running": false,
                            "socket": moss_dir.join("daemon.sock").to_string_lossy()
                        })
                    );
                } else {
                    eprintln!("Daemon is not running");
                    eprintln!("Socket: {}", moss_dir.join("daemon.sock").display());
                }
                return 1;
            }

            match client.status() {
                Ok(status) => {
                    if json {
                        println!(
                            "{}",
                            serde_json::json!({
                                "running": true,
                                "uptime_secs": status.uptime_secs,
                                "files_indexed": status.files_indexed,
                                "symbols_indexed": status.symbols_indexed,
                                "queries_served": status.queries_served,
                                "pid": status.pid
                            })
                        );
                    } else {
                        println!("Daemon Status");
                        println!("  Running: yes");
                        if let Some(pid) = status.pid {
                            println!("  PID: {}", pid);
                        }
                        println!("  Uptime: {} seconds", status.uptime_secs);
                        println!("  Files indexed: {}", status.files_indexed);
                        println!("  Symbols indexed: {}", status.symbols_indexed);
                        println!("  Queries served: {}", status.queries_served);
                    }
                    0
                }
                Err(e) => {
                    eprintln!("Failed to get daemon status: {}", e);
                    1
                }
            }
        }

        DaemonAction::Shutdown => {
            if !client.is_available() {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"success": false, "error": "daemon not running"})
                    );
                } else {
                    eprintln!("Daemon is not running");
                }
                return 1;
            }

            match client.shutdown() {
                Ok(()) => {
                    if json {
                        println!("{}", serde_json::json!({"success": true}));
                    } else {
                        println!("Daemon shutdown requested");
                    }
                    0
                }
                Err(e) => {
                    // Connection reset is expected when daemon shuts down
                    if e.contains("Connection reset") || e.contains("Broken pipe") {
                        if json {
                            println!("{}", serde_json::json!({"success": true}));
                        } else {
                            println!("Daemon shutdown requested");
                        }
                        0
                    } else {
                        eprintln!("Failed to shutdown daemon: {}", e);
                        1
                    }
                }
            }
        }

        DaemonAction::Start => {
            if client.is_available() {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"success": false, "error": "daemon already running"})
                    );
                } else {
                    eprintln!("Daemon is already running");
                }
                return 1;
            }

            // Start the daemon process
            if client.ensure_running() {
                if json {
                    println!("{}", serde_json::json!({"success": true}));
                } else {
                    println!("Daemon started");
                }
                0
            } else {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"success": false, "error": "failed to start daemon"})
                    );
                } else {
                    eprintln!("Failed to start daemon");
                }
                1
            }
        }

        DaemonAction::Run => {
            // Run daemon in foreground (blocking)
            match daemon::run_daemon(&root) {
                Ok(code) => code,
                Err(e) => {
                    eprintln!("Daemon error: {}", e);
                    1
                }
            }
        }
    }
}

fn cmd_update(check_only: bool, json: bool) -> i32 {
    const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
    const GITHUB_REPO: &str = "pterror/moss";

    let client = ureq::agent();

    // Fetch latest release from GitHub API
    let url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        GITHUB_REPO
    );

    let response = match client
        .get(&url)
        .set("User-Agent", "moss-cli")
        .set("Accept", "application/vnd.github+json")
        .call()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to check for updates: {}", e);
            return 1;
        }
    };

    let body: serde_json::Value = match response.into_json() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Failed to parse response: {}", e);
            return 1;
        }
    };

    let latest_version = body["tag_name"]
        .as_str()
        .unwrap_or("unknown")
        .trim_start_matches('v');

    let is_update_available = latest_version != CURRENT_VERSION
        && version_gt(latest_version, CURRENT_VERSION);

    if json && check_only {
        println!(
            "{}",
            serde_json::json!({
                "current_version": CURRENT_VERSION,
                "latest_version": latest_version,
                "update_available": is_update_available
            })
        );
        return 0;
    }

    if !json {
        println!("Current version: {}", CURRENT_VERSION);
        println!("Latest version:  {}", latest_version);
    }

    if !is_update_available {
        if !json {
            println!("You are running the latest version.");
        }
        return 0;
    }

    if check_only {
        if !json {
            println!();
            println!("Update available! Run 'moss update' to install.");
        }
        return 0;
    }

    // Perform the update
    println!();
    println!("Downloading update...");

    let target = get_target_triple();
    let asset_name = get_asset_name(&target);

    // Find the asset URL
    let assets = body["assets"].as_array();
    let asset_url = assets.and_then(|arr| {
        arr.iter()
            .find(|a| a["name"].as_str() == Some(&asset_name))
            .and_then(|a| a["browser_download_url"].as_str())
    });

    let asset_url = match asset_url {
        Some(url) => url,
        None => {
            eprintln!("No binary available for your platform: {}", target);
            eprintln!("Available assets:");
            if let Some(arr) = assets {
                for a in arr {
                    if let Some(name) = a["name"].as_str() {
                        eprintln!("  - {}", name);
                    }
                }
            }
            return 1;
        }
    };

    // Download the archive
    println!("  Downloading {}...", asset_name);
    let archive_response = match client.get(asset_url).call() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to download update: {}", e);
            return 1;
        }
    };

    let mut archive_data = Vec::new();
    if let Err(e) = archive_response.into_reader().read_to_end(&mut archive_data) {
        eprintln!("Failed to read download: {}", e);
        return 1;
    }

    // Download checksums
    let checksum_url = assets.and_then(|arr| {
        arr.iter()
            .find(|a| a["name"].as_str() == Some("SHA256SUMS.txt"))
            .and_then(|a| a["browser_download_url"].as_str())
    });

    if let Some(checksum_url) = checksum_url {
        println!("  Verifying checksum...");
        if let Ok(resp) = client.get(checksum_url).call() {
            if let Ok(checksums) = resp.into_string() {
                let expected = checksums
                    .lines()
                    .find(|line| line.contains(&asset_name))
                    .and_then(|line| line.split_whitespace().next());

                if let Some(expected) = expected {
                    let mut hasher = Sha256::new();
                    hasher.update(&archive_data);
                    let hash = hasher.finalize();
                    let actual: String = hash.iter().map(|b| format!("{:02x}", b)).collect();

                    if actual != expected {
                        eprintln!("Checksum mismatch!");
                        eprintln!("  Expected: {}", expected);
                        eprintln!("  Got:      {}", actual);
                        return 1;
                    }
                }
            }
        }
    }

    // Extract binary from archive
    println!("  Extracting...");
    let binary_data = if asset_name.ends_with(".tar.gz") {
        extract_tar_gz(&archive_data)
    } else if asset_name.ends_with(".zip") {
        extract_zip(&archive_data)
    } else {
        eprintln!("Unknown archive format: {}", asset_name);
        return 1;
    };

    let binary_data = match binary_data {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Failed to extract archive: {}", e);
            return 1;
        }
    };

    // Replace current binary
    println!("  Installing...");
    if let Err(e) = self_replace(&binary_data) {
        eprintln!("Failed to replace binary: {}", e);
        eprintln!("You may need to run with elevated permissions.");
        return 1;
    }

    println!();
    println!("Updated successfully to v{}!", latest_version);
    println!("Restart moss to use the new version.");

    0
}

/// Get the target triple for the current platform
fn get_target_triple() -> String {
    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "unknown"
    };

    let os = if cfg!(target_os = "linux") {
        "unknown-linux-gnu"
    } else if cfg!(target_os = "macos") {
        "apple-darwin"
    } else if cfg!(target_os = "windows") {
        "pc-windows-msvc"
    } else {
        "unknown"
    };

    format!("{}-{}", arch, os)
}

/// Get the expected asset name for a target
fn get_asset_name(target: &str) -> String {
    if target.contains("windows") {
        format!("moss-{}.zip", target)
    } else {
        format!("moss-{}.tar.gz", target)
    }
}

/// Simple SHA256 hasher
struct Sha256 {
    state: [u32; 8],
    buffer: Vec<u8>,
    total_len: u64,
}

impl Sha256 {
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
                0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
            ],
            buffer: Vec::new(),
            total_len: 0,
        }
    }

    fn update(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
        self.total_len += data.len() as u64;

        while self.buffer.len() >= 64 {
            let block: [u8; 64] = self.buffer[..64].try_into().unwrap();
            self.process_block(&block);
            self.buffer.drain(..64);
        }
    }

    fn finalize(mut self) -> [u8; 32] {
        // Padding
        self.buffer.push(0x80);
        while (self.buffer.len() % 64) != 56 {
            self.buffer.push(0);
        }

        // Length in bits
        let bit_len = self.total_len * 8;
        self.buffer.extend_from_slice(&bit_len.to_be_bytes());

        // Process remaining blocks - clone buffer to avoid borrow conflict
        let buffer = std::mem::take(&mut self.buffer);
        for chunk in buffer.chunks(64) {
            let block: [u8; 64] = chunk.try_into().unwrap();
            self.process_block(&block);
        }

        // Output
        let mut result = [0u8; 32];
        for (i, &val) in self.state.iter().enumerate() {
            result[i * 4..(i + 1) * 4].copy_from_slice(&val.to_be_bytes());
        }
        result
    }

    fn process_block(&mut self, block: &[u8; 64]) {
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
            0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
            0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
            0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
            0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
            0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
            0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
        ];

        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(block[i * 4..(i + 1) * 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }
}

/// Extract the moss binary from a tar.gz archive
fn extract_tar_gz(data: &[u8]) -> Result<Vec<u8>, String> {
    use std::io::Read;

    let decoder = flate2::read::GzDecoder::new(data);
    let mut archive = tar::Archive::new(decoder);

    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?;

        if path.file_name().map(|n| n == "moss").unwrap_or(false) {
            let mut contents = Vec::new();
            entry.read_to_end(&mut contents).map_err(|e| e.to_string())?;
            return Ok(contents);
        }
    }

    Err("moss binary not found in archive".to_string())
}

/// Extract the moss binary from a zip archive
fn extract_zip(data: &[u8]) -> Result<Vec<u8>, String> {
    use std::io::{Cursor, Read};

    let reader = Cursor::new(data);
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| e.to_string())?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = file.name().to_string();

        if name == "moss.exe" || name == "moss" {
            let mut contents = Vec::new();
            file.read_to_end(&mut contents).map_err(|e| e.to_string())?;
            return Ok(contents);
        }
    }

    Err("moss binary not found in archive".to_string())
}

/// Replace the current binary with new data
fn self_replace(new_binary: &[u8]) -> Result<(), String> {
    use std::fs;
    use std::io::Write;

    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;

    // Create temp file in same directory (for atomic rename on same filesystem)
    let temp_path = current_exe.with_extension("new");
    let backup_path = current_exe.with_extension("old");

    // Write new binary to temp file
    let mut temp_file = fs::File::create(&temp_path).map_err(|e| e.to_string())?;
    temp_file.write_all(new_binary).map_err(|e| e.to_string())?;
    temp_file.sync_all().map_err(|e| e.to_string())?;
    drop(temp_file);

    // Set executable permission on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o755);
        fs::set_permissions(&temp_path, perms).map_err(|e| e.to_string())?;
    }

    // Rename current to backup
    if backup_path.exists() {
        fs::remove_file(&backup_path).ok();
    }
    fs::rename(&current_exe, &backup_path).map_err(|e| format!("backup failed: {}", e))?;

    // Rename new to current
    if let Err(e) = fs::rename(&temp_path, &current_exe) {
        // Try to restore backup
        let _ = fs::rename(&backup_path, &current_exe);
        return Err(format!("install failed: {}", e));
    }

    // Remove backup
    fs::remove_file(&backup_path).ok();

    Ok(())
}

/// Simple version comparison (semver-like)
fn version_gt(a: &str, b: &str) -> bool {
    let parse = |v: &str| -> Vec<u32> {
        v.split('.')
            .filter_map(|s| s.split('-').next()?.parse().ok())
            .collect()
    };

    let va = parse(a);
    let vb = parse(b);

    for (a, b) in va.iter().zip(vb.iter()) {
        match a.cmp(b) {
            std::cmp::Ordering::Greater => return true,
            std::cmp::Ordering::Less => return false,
            std::cmp::Ordering::Equal => continue,
        }
    }
    va.len() > vb.len()
}

fn cmd_health(root: Option<&Path>, json: bool, profiler: &mut Profiler) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    profiler.mark("resolved_root");

    let report = health::analyze_health(&root);
    profiler.mark("analyzed");

    if json {
        let large_files: Vec<_> = report
            .large_files
            .iter()
            .map(|lf| {
                serde_json::json!({
                    "path": lf.path,
                    "lines": lf.lines,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({
                "total_files": report.total_files,
                "python_files": report.python_files,
                "rust_files": report.rust_files,
                "other_files": report.other_files,
                "total_lines": report.total_lines,
                "total_functions": report.total_functions,
                "avg_complexity": (report.avg_complexity * 10.0).round() / 10.0,
                "max_complexity": report.max_complexity,
                "high_risk_functions": report.high_risk_functions,
                "large_files": large_files,
            })
        );
    } else {
        println!("{}", report.format());
    }

    0
}

fn cmd_analyze(
    target: Option<&str>,
    root: Option<&Path>,
    health: bool,
    complexity: bool,
    security: bool,
    test_coverage: bool,
    scopes: bool,
    test_health: bool,
    threshold: Option<usize>,
    kind_filter: Option<&str>,
    json: bool,
) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // If no specific flags, run core analyses (health, complexity, security)
    let any_flag = health || complexity || security || test_coverage || scopes || test_health;
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
        let mut output = report.to_json();

        // Add test coverage analysis if requested
        if test_coverage {
            let analysis_path = target.map(|t| root.join(t)).unwrap_or_else(|| root.clone());
            if let Some(test_coverage_result) = run_python_test_coverage(&analysis_path, json) {
                if let serde_json::Value::Object(ref mut map) = output {
                    map.insert("test_coverage".to_string(), test_coverage_result);
                }
            }
        }

        // Add scopes analysis if requested
        if scopes {
            let analysis_path = target.map(|t| root.join(t)).unwrap_or_else(|| root.clone());
            if let Some(scopes_result) = run_python_scopes(&analysis_path, json) {
                if let serde_json::Value::Object(ref mut map) = output {
                    map.insert("scopes".to_string(), scopes_result);
                }
            }
        }

        // Add test health analysis if requested
        if test_health {
            let analysis_path = target.map(|t| root.join(t)).unwrap_or_else(|| root.clone());
            if let Some(test_health_result) = run_python_test_health(&analysis_path, json) {
                if let serde_json::Value::Object(ref mut map) = output {
                    map.insert("test_health".to_string(), test_health_result);
                }
            }
        }

        println!("{}", output);
    } else {
        println!("{}", report.format());

        // Add test coverage analysis if requested
        if test_coverage {
            let analysis_path = target.map(|t| root.join(t)).unwrap_or_else(|| root.clone());
            if let Some(test_coverage_result) = run_python_test_coverage(&analysis_path, false) {
                if let serde_json::Value::String(s) = test_coverage_result {
                    println!("\n{}", s);
                }
            }
        }

        // Add scopes analysis if requested
        if scopes {
            let analysis_path = target.map(|t| root.join(t)).unwrap_or_else(|| root.clone());
            if let Some(scopes_result) = run_python_scopes(&analysis_path, false) {
                if let serde_json::Value::String(s) = scopes_result {
                    println!("\n{}", s);
                }
            }
        }

        // Add test health analysis if requested
        if test_health {
            let analysis_path = target.map(|t| root.join(t)).unwrap_or_else(|| root.clone());
            if let Some(test_health_result) = run_python_test_health(&analysis_path, false) {
                if let serde_json::Value::String(s) = test_health_result {
                    println!("\n{}", s);
                }
            }
        }
    }

    0
}

/// Run Python test_gaps module for test coverage analysis
fn run_python_test_coverage(path: &Path, json: bool) -> Option<serde_json::Value> {
    use std::process::Command;

    let script = if json {
        format!(
            r#"
import json
from pathlib import Path
from moss_intelligence.test_gaps import analyze_test_coverage
report = analyze_test_coverage(Path('{}'))
print(json.dumps({{
    'coverage_percent': report.coverage_percent,
    'tested_count': report.tested_count,
    'untested_count': report.untested_count,
    'total_source_files': report.total_source_files,
    'patterns': [{{
        'pattern': p.pattern,
        'language': p.language,
        'count': p.count
    }} for p in report.patterns],
    'gaps': [{{
        'source_file': str(gap.source_file),
        'expected_test': gap.expected_test,
        'language': gap.language
    }} for gap in report.gaps[:20]]
}}))
"#,
            path.display()
        )
    } else {
        format!(
            r#"
from pathlib import Path
from moss_intelligence.test_gaps import analyze_test_coverage
report = analyze_test_coverage(Path('{}'))
print(report.to_compact())
"#,
            path.display()
        )
    };

    let output = Command::new("uv")
        .args(["run", "python", "-c", &script])
        .output()
        .ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if json {
            serde_json::from_str(stdout.trim()).ok()
        } else {
            Some(serde_json::Value::String(stdout.trim().to_string()))
        }
    } else {
        None
    }
}

/// Run Python scopes module for public/private symbol analysis
fn run_python_scopes(path: &Path, json: bool) -> Option<serde_json::Value> {
    use std::process::Command;

    let script = if json {
        format!(
            r#"
import json
from pathlib import Path
from moss_intelligence.scopes import analyze_project_scopes
report = analyze_project_scopes(Path('{}'))
print(json.dumps(report.to_dict()))
"#,
            path.display()
        )
    } else {
        format!(
            r#"
from pathlib import Path
from moss_intelligence.scopes import analyze_project_scopes
report = analyze_project_scopes(Path('{}'))
print(report.to_compact())
"#,
            path.display()
        )
    };

    let output = Command::new("uv")
        .args(["run", "python", "-c", &script])
        .output()
        .ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if json {
            serde_json::from_str(stdout.trim()).ok()
        } else {
            Some(serde_json::Value::String(stdout.trim().to_string()))
        }
    } else {
        None
    }
}

/// Run Python test_health module for pytest marker analysis
fn run_python_test_health(path: &Path, json: bool) -> Option<serde_json::Value> {
    use std::process::Command;

    let script = if json {
        format!(
            r#"
import json
from pathlib import Path
from moss_intelligence.test_health import analyze_test_health
report = analyze_test_health(Path('{}'))
print(json.dumps(report.to_dict()))
"#,
            path.display()
        )
    } else {
        format!(
            r#"
from pathlib import Path
from moss_intelligence.test_health import analyze_test_health
report = analyze_test_health(Path('{}'))
print(report.to_compact())
"#,
            path.display()
        )
    };

    let output = Command::new("uv")
        .args(["run", "python", "-c", &script])
        .output()
        .ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if json {
            serde_json::from_str(stdout.trim()).ok()
        } else {
            Some(serde_json::Value::String(stdout.trim().to_string()))
        }
    } else {
        None
    }
}

fn cmd_overview(root: Option<&Path>, compact: bool, json: bool, profiler: &mut Profiler) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    profiler.mark("resolved_root");

    let report = overview::analyze_overview(&root);
    profiler.mark("analyzed");

    if json {
        println!(
            "{}",
            serde_json::json!({
                "total_files": report.total_files,
                "python_files": report.python_files,
                "rust_files": report.rust_files,
                "other_files": report.other_files,
                "total_lines": report.total_lines,
                "total_functions": report.total_functions,
                "total_classes": report.total_classes,
                "total_methods": report.total_methods,
                "avg_complexity": (report.avg_complexity * 10.0).round() / 10.0,
                "max_complexity": report.max_complexity,
                "high_risk_functions": report.high_risk_functions,
                "functions_with_docs": report.functions_with_docs,
                "doc_coverage": (report.doc_coverage * 100.0).round() / 100.0,
                "total_imports": report.total_imports,
                "unique_modules": report.unique_modules,
                "todo_count": report.todo_count,
                "fixme_count": report.fixme_count,
                "health_score": (report.health_score * 100.0).round() / 100.0,
                "grade": report.grade
            })
        );
    } else if compact {
        println!("{}", report.format_compact());
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

fn cmd_grep(
    pattern: &str,
    root: Option<&Path>,
    glob_pattern: Option<&str>,
    limit: usize,
    ignore_case: bool,
    json: bool,
) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    match grep::grep(pattern, &root, glob_pattern, limit, ignore_case) {
        Ok(result) => {
            if json {
                println!("{}", serde_json::to_string(&result).unwrap());
            } else {
                if result.matches.is_empty() {
                    eprintln!("No matches found for: {}", pattern);
                    return 1;
                }
                for m in &result.matches {
                    println!("{}:{}:{}", m.file, m.line, m.content);
                }
                eprintln!(
                    "\n{} matches in {} files",
                    result.total_matches, result.files_searched
                );
            }
            0
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            1
        }
    }
}

fn cmd_find_symbols(
    name: &str,
    root: Option<&Path>,
    kind: Option<&str>,
    fuzzy: bool,
    limit: usize,
    json: bool,
    profiler: &mut Profiler,
) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    profiler.mark("resolved_root");

    // Open or create index
    let idx = match index::FileIndex::open(&root) {
        Ok(idx) => idx,
        Err(e) => {
            eprintln!("Failed to open index: {}", e);
            return 1;
        }
    };
    profiler.mark("open_index");

    // Check if call graph is populated (symbols are indexed there)
    let (symbol_count, _, _) = idx.call_graph_stats().unwrap_or((0, 0, 0));
    if symbol_count == 0 {
        eprintln!("Symbol index empty. Run: moss reindex --call-graph");
        return 1;
    }
    profiler.mark("check_stats");

    // Query symbols
    match idx.find_symbols(name, kind, fuzzy, limit) {
        Ok(symbols) => {
            profiler.mark("query");

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

/// Check if a file appears to be binary by looking for null bytes in the first 8KB
fn is_binary_file(path: &Path) -> bool {
    use std::io::Read;

    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };

    let mut buffer = [0u8; 8192];
    let Ok(bytes_read) = file.read(&mut buffer) else {
        return false;
    };

    // Check for null bytes (common in binary files)
    buffer[..bytes_read].contains(&0)
}

fn cmd_index_stats(root: Option<&Path>, json: bool) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    let moss_dir = get_moss_dir(&root);
    let db_path = moss_dir.join("index.sqlite");

    // Get DB file size
    let db_size = std::fs::metadata(&db_path)
        .map(|m| m.len())
        .unwrap_or(0);

    // Open index and get stats
    let idx = match index::FileIndex::open(&root) {
        Ok(idx) => idx,
        Err(e) => {
            eprintln!("Failed to open index: {}", e);
            return 1;
        }
    };

    // Get file stats from index
    let files = match idx.all_files() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to read files: {}", e);
            return 1;
        }
    };

    let file_count = files.iter().filter(|f| !f.is_dir).count();
    let dir_count = files.iter().filter(|f| f.is_dir).count();

    // Count by extension (detect binary files)
    let mut ext_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for f in &files {
        if f.is_dir {
            continue;
        }
        let path = std::path::Path::new(&f.path);
        let ext = match path.extension().and_then(|e| e.to_str()) {
            Some(e) => e.to_string(),
            None => {
                // No extension - check if binary
                let full_path = root.join(&f.path);
                if is_binary_file(&full_path) {
                    "(binary)".to_string()
                } else {
                    "(no ext)".to_string()
                }
            }
        };
        *ext_counts.entry(ext).or_insert(0) += 1;
    }

    // Sort by count descending
    let mut ext_list: Vec<_> = ext_counts.into_iter().collect();
    ext_list.sort_by(|a, b| b.1.cmp(&a.1));

    // Get call graph stats
    let (symbol_count, call_count, import_count) = idx.call_graph_stats().unwrap_or((0, 0, 0));

    // Calculate codebase size (sum of file sizes)
    let mut codebase_size: u64 = 0;
    for f in &files {
        if !f.is_dir {
            let full_path = root.join(&f.path);
            if let Ok(meta) = std::fs::metadata(&full_path) {
                codebase_size += meta.len();
            }
        }
    }

    if json {
        let output = serde_json::json!({
            "db_size_bytes": db_size,
            "codebase_size_bytes": codebase_size,
            "ratio": if codebase_size > 0 { db_size as f64 / codebase_size as f64 } else { 0.0 },
            "file_count": file_count,
            "dir_count": dir_count,
            "symbol_count": symbol_count,
            "call_count": call_count,
            "import_count": import_count,
            "extensions": ext_list.iter().take(20).map(|(e, c)| serde_json::json!({"ext": e, "count": c})).collect::<Vec<_>>()
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!("Index Statistics");
        println!("================");
        println!();
        println!("Database:     {} ({:.1} KB)", db_path.display(), db_size as f64 / 1024.0);
        println!("Codebase:     {:.1} MB", codebase_size as f64 / 1024.0 / 1024.0);
        println!("Ratio:        {:.2}%", if codebase_size > 0 { db_size as f64 / codebase_size as f64 * 100.0 } else { 0.0 });
        println!();
        println!("Files:        {} ({} dirs)", file_count, dir_count);
        println!("Symbols:      {}", symbol_count);
        println!("Calls:        {}", call_count);
        println!("Imports:      {}", import_count);
        println!();
        println!("Top extensions:");
        for (ext, count) in ext_list.iter().take(15) {
            println!("  {:12} {:>6}", ext, count);
        }
    }

    0
}

fn cmd_list_files(prefix: Option<&str>, root: Option<&Path>, limit: usize, json: bool) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    let idx = match index::FileIndex::open(&root) {
        Ok(idx) => idx,
        Err(e) => {
            eprintln!("Failed to open index: {}", e);
            return 1;
        }
    };

    let files = match idx.all_files() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to read files: {}", e);
            return 1;
        }
    };

    // Filter by prefix and exclude directories
    let prefix_str = prefix.unwrap_or("");
    let filtered: Vec<&str> = files
        .iter()
        .filter(|f| !f.is_dir && f.path.starts_with(prefix_str))
        .take(limit)
        .map(|f| f.path.as_str())
        .collect();

    if json {
        println!("{}", serde_json::to_string(&filtered).unwrap());
    } else {
        for path in &filtered {
            println!("{}", path);
        }
    }

    0
}

/// Index external packages into the global cache.
fn cmd_index_packages(only: &[String], clear: bool, root: Option<&Path>, json: bool) -> i32 {
    let root = root.map(|p| p.to_path_buf()).unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    });

    // Open or create the index
    let index = match external_packages::PackageIndex::open() {
        Ok(idx) => idx,
        Err(e) => {
            eprintln!("Failed to open package index: {}", e);
            return 1;
        }
    };

    if clear {
        if let Err(e) = index.clear() {
            eprintln!("Failed to clear index: {}", e);
            return 1;
        }
        if !json {
            println!("Cleared existing index");
        }
    }

    // Determine which ecosystems to index
    let all_ecosystems = ["python", "go", "js", "deno", "java", "cpp", "rust"];
    let do_all = only.is_empty();
    let ecosystems: Vec<&str> = if do_all {
        all_ecosystems.to_vec()
    } else {
        only.iter()
            .map(|s| s.as_str())
            .filter(|s| all_ecosystems.contains(s))
            .collect()
    };

    // Log error for unknown ecosystems (but continue with valid ones)
    for eco in only {
        if !all_ecosystems.contains(&eco.as_str()) {
            eprintln!("Error: unknown ecosystem '{}', valid options: {}", eco, all_ecosystems.join(", "));
        }
    }

    let mut stats = IndexPackagesStats::default();

    for eco in &ecosystems {
        match *eco {
            "python" => index_python_packages(&index, &root, &mut stats, json),
            "go" => index_go_packages(&index, &mut stats, json),
            "js" => index_js_packages(&index, &root, &mut stats, json),
            "deno" => index_deno_packages(&index, &mut stats, json),
            "java" => index_java_packages(&index, &mut stats, json),
            "cpp" => index_cpp_packages(&index, &mut stats, json),
            "rust" => index_rust_packages(&index, &mut stats, json),
            _ => {}
        }
    }

    if json {
        println!("{}", serde_json::json!({
            "python_packages": stats.python_packages,
            "python_symbols": stats.python_symbols,
            "go_packages": stats.go_packages,
            "go_symbols": stats.go_symbols,
            "js_packages": stats.js_packages,
            "js_symbols": stats.js_symbols,
            "deno_packages": stats.deno_packages,
            "deno_symbols": stats.deno_symbols,
            "java_packages": stats.java_packages,
            "java_symbols": stats.java_symbols,
            "cpp_packages": stats.cpp_packages,
            "cpp_symbols": stats.cpp_symbols,
            "rust_packages": stats.rust_packages,
            "rust_symbols": stats.rust_symbols,
        }));
    } else {
        println!("\nIndexing complete:");
        if ecosystems.contains(&"python") {
            println!("  Python: {} packages, {} symbols", stats.python_packages, stats.python_symbols);
        }
        if ecosystems.contains(&"go") {
            println!("  Go: {} packages, {} symbols", stats.go_packages, stats.go_symbols);
        }
        if ecosystems.contains(&"js") {
            println!("  JavaScript: {} packages, {} symbols", stats.js_packages, stats.js_symbols);
        }
        if ecosystems.contains(&"deno") {
            println!("  Deno: {} packages, {} symbols", stats.deno_packages, stats.deno_symbols);
        }
        if ecosystems.contains(&"java") {
            println!("  Java: {} packages, {} symbols", stats.java_packages, stats.java_symbols);
        }
        if ecosystems.contains(&"cpp") {
            println!("  C/C++: {} headers, {} symbols", stats.cpp_packages, stats.cpp_symbols);
        }
        if ecosystems.contains(&"rust") {
            println!("  Rust: {} packages, {} symbols", stats.rust_packages, stats.rust_symbols);
        }
    }

    0
}

#[derive(Default)]
struct IndexPackagesStats {
    python_packages: usize,
    python_symbols: usize,
    go_packages: usize,
    go_symbols: usize,
    js_packages: usize,
    js_symbols: usize,
    deno_packages: usize,
    deno_symbols: usize,
    java_packages: usize,
    java_symbols: usize,
    cpp_packages: usize,
    cpp_symbols: usize,
    rust_packages: usize,
    rust_symbols: usize,
}

/// Index Python stdlib and site-packages.
fn index_python_packages(
    index: &external_packages::PackageIndex,
    root: &Path,
    stats: &mut IndexPackagesStats,
    json: bool,
) {
    let version = moss_languages::python::get_python_version(root)
        .and_then(|v| external_packages::Version::parse(&v));

    let min_version = version.unwrap_or(external_packages::Version { major: 3, minor: 0 });

    if !json {
        println!("Indexing Python packages (version {:?})...", version);
    }

    let mut extractor = skeleton::SkeletonExtractor::new();

    let python = moss_languages::python::Python;

    // Index stdlib
    if let Some(stdlib) = python.find_stdlib(root) {
        if !json {
            println!("  Stdlib: {}", stdlib.display());
        }
        if let Ok(entries) = std::fs::read_dir(&stdlib) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();

                if python.should_skip_package_entry(&name, path.is_dir()) {
                    continue;
                }

                let module_name = python.package_module_name(&name);

                // Skip if already indexed
                if let Ok(true) = index.is_indexed("python", &module_name) {
                    continue;
                }

                let pkg_id = match index.insert_package(
                    "python",
                    &module_name,
                    &path.to_string_lossy(),
                    min_version,
                    version, // stdlib is version-specific
                ) {
                    Ok(id) => id,
                    Err(_) => continue,
                };

                stats.python_packages += 1;
                let symbols = index_python_path(&mut extractor, index, pkg_id, &path);
                stats.python_symbols += symbols;
            }
        }
    }

    // Index site-packages
    if let Some(site_packages) = moss_languages::python::find_python_site_packages(root) {
        if !json {
            println!("  Site-packages: {}", site_packages.display());
        }
        if let Ok(entries) = std::fs::read_dir(&site_packages) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();

                if python.should_skip_package_entry(&name, path.is_dir()) {
                    continue;
                }

                let module_name = python.package_module_name(&name);

                // Skip if already indexed
                if let Ok(true) = index.is_indexed("python", &module_name) {
                    continue;
                }

                let pkg_id = match index.insert_package(
                    "python",
                    &module_name,
                    &path.to_string_lossy(),
                    min_version,
                    None, // packages may work with newer versions
                ) {
                    Ok(id) => id,
                    Err(_) => continue,
                };

                stats.python_packages += 1;
                let symbols = index_python_path(&mut extractor, index, pkg_id, &path);
                stats.python_symbols += symbols;
            }
        }
    }
}

/// Index a Python path (file or directory) and return symbol count.
fn index_python_path(
    extractor: &mut skeleton::SkeletonExtractor,
    index: &external_packages::PackageIndex,
    pkg_id: i64,
    path: &Path,
) -> usize {
    let mut count = 0;

    if path.is_file() && path.extension().map_or(false, |e| e == "py") {
        if let Ok(content) = std::fs::read_to_string(path) {
            let result = extractor.extract(path, &content);
            count += count_and_insert_symbols(index, pkg_id, &result.symbols);
        }
    } else if path.is_dir() {
        // Index __init__.py if it exists
        let init_path = path.join("__init__.py");
        if init_path.is_file() {
            if let Ok(content) = std::fs::read_to_string(&init_path) {
                let result = extractor.extract(&init_path, &content);
                count += count_and_insert_symbols(index, pkg_id, &result.symbols);
            }
        }
    }

    count
}

/// Index Go stdlib and mod cache.
fn index_go_packages(
    index: &external_packages::PackageIndex,
    stats: &mut IndexPackagesStats,
    json: bool,
) {
    let go = moss_languages::go::Go;
    let version = moss_languages::go::get_go_version()
        .and_then(|v| external_packages::Version::parse(&v));

    let min_version = version.unwrap_or(external_packages::Version { major: 1, minor: 0 });

    if !json {
        println!("Indexing Go packages (version {:?})...", version);
    }

    let mut extractor = skeleton::SkeletonExtractor::new();

    // Index stdlib
    if let Some(stdlib) = go.find_stdlib(Path::new(".")) {
        if !json {
            println!("  Stdlib: {}", stdlib.display());
        }
        index_go_stdlib_recursive(&go, &mut extractor, index, &stdlib, "", min_version, version, stats);
    }

    // Index mod cache (just top-level for now - full recursive would be slow)
    if let Some(mod_cache) = moss_languages::go::find_go_mod_cache() {
        if !json {
            println!("  Mod cache: {}", mod_cache.display());
        }
        // Note: mod cache indexing is lazy (on-demand) for performance
        // We just report its availability here
    }
}

/// Recursively index Go stdlib packages.
fn index_go_stdlib_recursive(
    go: &moss_languages::go::Go,
    extractor: &mut skeleton::SkeletonExtractor,
    index: &external_packages::PackageIndex,
    base: &Path,
    prefix: &str,
    min_version: external_packages::Version,
    max_version: Option<external_packages::Version>,
    stats: &mut IndexPackagesStats,
) {
    let Ok(entries) = std::fs::read_dir(base) else { return };

    let mut has_go_files = false;

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let is_dir = path.is_dir();

        if go.should_skip_package_entry(&name, is_dir) {
            continue;
        }

        if is_dir {
            // Recurse into subdirectories
            let sub_prefix = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", prefix, name)
            };
            index_go_stdlib_recursive(go, extractor, index, &path, &sub_prefix, min_version, max_version, stats);
        } else {
            has_go_files = true;
        }
    }

    // If this directory has .go files, index it as a package
    if has_go_files && !prefix.is_empty() {
        // Skip if already indexed
        if let Ok(true) = index.is_indexed("go", prefix) {
            return;
        }

        let pkg_id = match index.insert_package(
            "go",
            prefix,
            &base.to_string_lossy(),
            min_version,
            max_version,
        ) {
            Ok(id) => id,
            Err(_) => return,
        };

        stats.go_packages += 1;

        // Index all .go files in this directory
        if let Ok(entries) = std::fs::read_dir(base) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if !go.should_skip_package_entry(&name, false) {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        let result = extractor.extract(&path, &content);
                        stats.go_symbols += count_and_insert_symbols(index, pkg_id, &result.symbols);
                    }
                }
            }
        }
    }
}

/// Index JavaScript packages from node_modules.
fn index_js_packages(
    index: &external_packages::PackageIndex,
    root: &Path,
    stats: &mut IndexPackagesStats,
    json: bool,
) {
    let js = moss_languages::javascript::JavaScript;
    let version = moss_languages::ecmascript::get_node_version()
        .and_then(|v| external_packages::Version::parse(&v));

    if !json {
        println!("Indexing JavaScript packages (version {:?})...", version);
    }

    let node_modules = match js.find_package_cache(root) {
        Some(nm) => nm,
        None => {
            if !json {
                println!("  No node_modules found");
            }
            return;
        }
    };

    if !json {
        println!("  node_modules: {}", node_modules.display());
    }

    let min_version = version.unwrap_or(external_packages::Version { major: 0, minor: 0 });
    let mut extractor = skeleton::SkeletonExtractor::new();

    if let Ok(entries) = std::fs::read_dir(&node_modules) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden dirs (but not scoped packages starting with @)
            if js.should_skip_package_entry(&name, path.is_dir()) && !name.starts_with('@') {
                continue;
            }

            // Handle scoped packages (@scope/name)
            if name.starts_with('@') && path.is_dir() {
                if let Ok(scoped) = std::fs::read_dir(&path) {
                    for scoped_entry in scoped.flatten() {
                        let scoped_path = scoped_entry.path();
                        let scoped_name = format!("{}/{}", name, scoped_entry.file_name().to_string_lossy());

                        if let Some(entry_point) = get_js_entry_point(&scoped_path) {
                            index_single_js_package(
                                index, &mut extractor, &scoped_name, &entry_point, min_version, stats
                            );
                        }
                    }
                }
                continue;
            }

            // Regular package
            if path.is_dir() {
                if let Some(entry_point) = get_js_entry_point(&path) {
                    index_single_js_package(
                        index, &mut extractor, &name, &entry_point, min_version, stats
                    );
                }
            }
        }
    }
}

/// Get the entry point for a JavaScript package.
fn get_js_entry_point(pkg_dir: &Path) -> Option<PathBuf> {
    let pkg_json = pkg_dir.join("package.json");
    if pkg_json.is_file() {
        let content = std::fs::read_to_string(&pkg_json).ok()?;
        let json: serde_json::Value = serde_json::from_str(&content).ok()?;

        // Try module, main, or index.js
        if let Some(module) = json.get("module").and_then(|v| v.as_str()) {
            let path = pkg_dir.join(module.trim_start_matches("./"));
            if path.is_file() {
                return Some(path);
            }
        }
        if let Some(main) = json.get("main").and_then(|v| v.as_str()) {
            let path = pkg_dir.join(main.trim_start_matches("./"));
            if path.is_file() {
                return Some(path);
            }
            // Try with .js extension
            let path = pkg_dir.join(main.trim_start_matches("./")).with_extension("js");
            if path.is_file() {
                return Some(path);
            }
        }
    }

    // Fallback to index.js
    for ext in &["js", "mjs", "cjs"] {
        let index = pkg_dir.join(format!("index.{}", ext));
        if index.is_file() {
            return Some(index);
        }
    }

    None
}

/// Index a single JavaScript package.
fn index_single_js_package(
    index: &external_packages::PackageIndex,
    extractor: &mut skeleton::SkeletonExtractor,
    name: &str,
    entry_point: &Path,
    min_version: external_packages::Version,
    stats: &mut IndexPackagesStats,
) {
    // Skip if already indexed
    if let Ok(true) = index.is_indexed("js", name) {
        return;
    }

    let pkg_id = match index.insert_package(
        "js",
        name,
        &entry_point.to_string_lossy(),
        min_version,
        None,
    ) {
        Ok(id) => id,
        Err(_) => return,
    };

    stats.js_packages += 1;

    if let Ok(content) = std::fs::read_to_string(entry_point) {
        let result = extractor.extract(entry_point, &content);
        stats.js_symbols += count_and_insert_symbols(index, pkg_id, &result.symbols);
    }
}

/// Index Deno packages from cache.
fn index_deno_packages(
    index: &external_packages::PackageIndex,
    stats: &mut IndexPackagesStats,
    json: bool,
) {
    let version = moss_languages::ecmascript::get_deno_version()
        .and_then(|v| external_packages::Version::parse(&v));

    if !json {
        println!("Indexing Deno packages (version {:?})...", version);
    }

    let cache = match moss_languages::ecmascript::find_deno_cache() {
        Some(c) => c,
        None => {
            if !json {
                println!("  No Deno cache found");
            }
            return;
        }
    };

    if !json {
        println!("  Deno cache: {}", cache.display());
    }

    let min_version = version.unwrap_or(external_packages::Version { major: 1, minor: 0 });
    let mut extractor = skeleton::SkeletonExtractor::new();

    // Index npm packages from Deno's npm cache
    let npm_cache = cache.join("npm").join("registry.npmjs.org");
    if npm_cache.is_dir() {
        if !json {
            println!("  Indexing npm packages...");
        }
        index_deno_npm_packages(index, &npm_cache, min_version, &mut extractor, stats);
    }

    // Index URL-based deps
    let deps_cache = cache.join("deps");
    if deps_cache.is_dir() {
        if !json {
            println!("  Indexing URL deps...");
        }
        index_deno_url_packages(index, &deps_cache, min_version, &mut extractor, stats);
    }
}

/// Index Deno npm packages.
fn index_deno_npm_packages(
    index: &external_packages::PackageIndex,
    npm_cache: &Path,
    min_version: external_packages::Version,
    extractor: &mut skeleton::SkeletonExtractor,
    stats: &mut IndexPackagesStats,
) {
    // Structure: npm/registry.npmjs.org/package/version/
    if let Ok(packages) = std::fs::read_dir(npm_cache) {
        for pkg_entry in packages.flatten() {
            let pkg_path = pkg_entry.path();
            let pkg_name = pkg_entry.file_name().to_string_lossy().to_string();

            if !pkg_path.is_dir() {
                continue;
            }

            // Handle scoped packages (@scope/name)
            if pkg_name.starts_with('@') {
                if let Ok(scoped) = std::fs::read_dir(&pkg_path) {
                    for scoped_entry in scoped.flatten() {
                        let scoped_path = scoped_entry.path();
                        let full_name = format!("{}/{}", pkg_name, scoped_entry.file_name().to_string_lossy());
                        index_deno_npm_package(index, extractor, &scoped_path, &full_name, min_version, stats);
                    }
                }
            } else {
                index_deno_npm_package(index, extractor, &pkg_path, &pkg_name, min_version, stats);
            }
        }
    }
}

/// Index a single Deno npm package (finds latest version).
fn index_deno_npm_package(
    index: &external_packages::PackageIndex,
    extractor: &mut skeleton::SkeletonExtractor,
    pkg_path: &Path,
    name: &str,
    min_version: external_packages::Version,
    stats: &mut IndexPackagesStats,
) {
    // Skip if already indexed
    if let Ok(true) = index.is_indexed("deno", name) {
        return;
    }

    // Find latest version directory
    let mut versions: Vec<_> = std::fs::read_dir(pkg_path)
        .ok()
        .map(|entries| entries.flatten().filter(|e| e.path().is_dir()).collect())
        .unwrap_or_default();

    if versions.is_empty() {
        return;
    }

    // Sort by version and take the latest
    versions.sort_by(|a, b| {
        let a_name = a.file_name().to_string_lossy().to_string();
        let b_name = b.file_name().to_string_lossy().to_string();
        external_packages::version_cmp(&a_name, &b_name)
    });

    let version_dir = versions.last().unwrap().path();

    // Find entry point
    let entry_point = match find_deno_package_entry(&version_dir) {
        Some(e) => e,
        None => return,
    };

    let pkg_id = match index.insert_package(
        "deno",
        name,
        &entry_point.to_string_lossy(),
        min_version,
        None,
    ) {
        Ok(id) => id,
        Err(_) => return,
    };

    stats.deno_packages += 1;

    if let Ok(content) = std::fs::read_to_string(&entry_point) {
        let result = extractor.extract(&entry_point, &content);
        stats.deno_symbols += count_and_insert_symbols(index, pkg_id, &result.symbols);
    }
}

/// Find entry point for a Deno package.
fn find_deno_package_entry(dir: &Path) -> Option<PathBuf> {
    // Try package.json
    let pkg_json = dir.join("package.json");
    if pkg_json.is_file() {
        if let Ok(content) = std::fs::read_to_string(&pkg_json) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                for field in &["module", "main"] {
                    if let Some(entry) = json.get(field).and_then(|v| v.as_str()) {
                        let path = dir.join(entry.trim_start_matches("./"));
                        if path.is_file() {
                            return Some(path);
                        }
                        let with_ext = path.with_extension("js");
                        if with_ext.is_file() {
                            return Some(with_ext);
                        }
                    }
                }
            }
        }
    }

    // Fallback to index files
    for ext in &["js", "mjs", "cjs", "ts"] {
        let index = dir.join(format!("index.{}", ext));
        if index.is_file() {
            return Some(index);
        }
    }

    None
}

/// Index Deno URL-based packages.
fn index_deno_url_packages(
    index: &external_packages::PackageIndex,
    deps_cache: &Path,
    min_version: external_packages::Version,
    extractor: &mut skeleton::SkeletonExtractor,
    stats: &mut IndexPackagesStats,
) {
    // Structure: deps/https/deno.land/<hash> with <hash>.metadata.json
    for scheme in &["https", "http"] {
        let scheme_dir = deps_cache.join(scheme);
        if !scheme_dir.is_dir() {
            continue;
        }

        if let Ok(hosts) = std::fs::read_dir(&scheme_dir) {
            for host_entry in hosts.flatten() {
                let host_path = host_entry.path();
                let host = host_entry.file_name().to_string_lossy().to_string();

                if !host_path.is_dir() {
                    continue;
                }

                index_deno_host_packages(index, extractor, &host_path, &host, scheme, min_version, stats);
            }
        }
    }
}

/// Index packages from a specific Deno host cache.
fn index_deno_host_packages(
    index: &external_packages::PackageIndex,
    extractor: &mut skeleton::SkeletonExtractor,
    host_path: &Path,
    host: &str,
    scheme: &str,
    min_version: external_packages::Version,
    stats: &mut IndexPackagesStats,
) {
    if let Ok(entries) = std::fs::read_dir(host_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            // Skip metadata files
            if name.ends_with(".metadata.json") {
                continue;
            }

            // Read metadata to get URL
            let meta_path = host_path.join(format!("{}.metadata.json", name));
            if !meta_path.is_file() {
                continue;
            }

            let url = match std::fs::read_to_string(&meta_path) {
                Ok(content) => {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                        json.get("url").and_then(|v| v.as_str()).map(|s| s.to_string())
                    } else {
                        None
                    }
                }
                Err(_) => None,
            };

            let pkg_name = match &url {
                Some(u) => u.strip_prefix(&format!("{}://", scheme)).unwrap_or(&name).to_string(),
                None => format!("{}/{}", host, name),
            };

            // Skip if already indexed
            if let Ok(true) = index.is_indexed("deno", &pkg_name) {
                continue;
            }

            if !path.is_file() {
                continue;
            }

            let pkg_id = match index.insert_package(
                "deno",
                &pkg_name,
                &path.to_string_lossy(),
                min_version,
                None,
            ) {
                Ok(id) => id,
                Err(_) => continue,
            };

            stats.deno_packages += 1;

            if let Ok(content) = std::fs::read_to_string(&path) {
                let result = extractor.extract(&path, &content);
                stats.deno_symbols += count_and_insert_symbols(index, pkg_id, &result.symbols);
            }
        }
    }
}

/// Index Java packages from Maven and Gradle caches.
fn index_java_packages(
    index: &external_packages::PackageIndex,
    stats: &mut IndexPackagesStats,
    json: bool,
) {
    let version = moss_languages::java::get_java_version()
        .and_then(|v| external_packages::Version::parse(&v));

    if !json {
        println!("Indexing Java packages (version {:?})...", version);
    }

    let min_version = version.unwrap_or(external_packages::Version { major: 11, minor: 0 });

    // Index Maven repository
    if let Some(maven_repo) = moss_languages::java::find_maven_repository() {
        if !json {
            println!("  Maven repository: {}", maven_repo.display());
        }
        index_maven_packages(index, &maven_repo, min_version, stats);
    } else if !json {
        println!("  No Maven repository found");
    }

    // Index Gradle cache
    if let Some(gradle_cache) = moss_languages::java::find_gradle_cache() {
        if !json {
            println!("  Gradle cache: {}", gradle_cache.display());
        }
        index_gradle_packages(index, &gradle_cache, min_version, stats);
    } else if !json {
        println!("  No Gradle cache found");
    }
}

/// Index Maven repository packages.
fn index_maven_packages(
    index: &external_packages::PackageIndex,
    maven_repo: &Path,
    min_version: external_packages::Version,
    stats: &mut IndexPackagesStats,
) {
    // Walk the Maven repo, looking for JAR files
    // Structure: group/artifact/version/artifact-version.jar
    index_maven_directory(index, maven_repo, maven_repo, min_version, stats);
}

/// Recursively index a Maven directory.
fn index_maven_directory(
    index: &external_packages::PackageIndex,
    maven_repo: &Path,
    current: &Path,
    min_version: external_packages::Version,
    stats: &mut IndexPackagesStats,
) {
    let entries = match std::fs::read_dir(current) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            // Check if this looks like a version directory (contains JARs)
            if has_jar_files(&path) {
                // This is a version dir, parent is artifact, grandparent is group path
                if let Some(artifact_dir) = current.parent() {
                    let artifact = current.file_name().unwrap_or_default().to_string_lossy();
                    if let Some(group_path) = artifact_dir.strip_prefix(maven_repo).ok() {
                        let group = group_path.to_string_lossy().replace('/', ".");
                        let pkg_name = format!("{}:{}", group, artifact);

                        // Skip if already indexed
                        if let Ok(true) = index.is_indexed("java", &pkg_name) {
                            continue;
                        }

                        // Find the JAR (prefer sources)
                        if let Some(jar_path) = find_maven_jar(&path, &artifact) {
                            let _ = index.insert_package(
                                "java",
                                &pkg_name,
                                &jar_path.to_string_lossy(),
                                min_version,
                                None,
                            );
                            stats.java_packages += 1;
                            // Note: We don't extract symbols from JARs (binary)
                        }
                    }
                }
            } else {
                // Recurse into subdirectory
                index_maven_directory(index, maven_repo, &path, min_version, stats);
            }
        }
    }
}

/// Check if a directory contains JAR files.
fn has_jar_files(dir: &Path) -> bool {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".jar") {
                return true;
            }
        }
    }
    false
}

/// Find the best JAR in a Maven version directory.
fn find_maven_jar(version_dir: &Path, artifact: &str) -> Option<PathBuf> {
    let entries: Vec<_> = std::fs::read_dir(version_dir).ok()?.flatten().collect();

    // Prefer sources JAR
    for entry in &entries {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(artifact) && name.ends_with("-sources.jar") {
            return Some(entry.path());
        }
    }

    // Fall back to regular JAR
    for entry in &entries {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(artifact) && name.ends_with(".jar") && !name.ends_with("-javadoc.jar") {
            return Some(entry.path());
        }
    }

    None
}

/// Index Gradle cache packages.
fn index_gradle_packages(
    index: &external_packages::PackageIndex,
    gradle_cache: &Path,
    min_version: external_packages::Version,
    stats: &mut IndexPackagesStats,
) {
    // Structure: group/artifact/version/hash/artifact-version.jar
    if let Ok(groups) = std::fs::read_dir(gradle_cache) {
        for group_entry in groups.flatten() {
            let group_path = group_entry.path();
            let group = group_entry.file_name().to_string_lossy().to_string();

            if !group_path.is_dir() {
                continue;
            }

            if let Ok(artifacts) = std::fs::read_dir(&group_path) {
                for artifact_entry in artifacts.flatten() {
                    let artifact_path = artifact_entry.path();
                    let artifact = artifact_entry.file_name().to_string_lossy().to_string();

                    if !artifact_path.is_dir() {
                        continue;
                    }

                    let pkg_name = format!("{}:{}", group, artifact);

                    // Skip if already indexed
                    if let Ok(true) = index.is_indexed("java", &pkg_name) {
                        continue;
                    }

                    // Find latest version
                    if let Some(jar_path) = find_gradle_jar(&artifact_path) {
                        let _ = index.insert_package(
                            "java",
                            &pkg_name,
                            &jar_path.to_string_lossy(),
                            min_version,
                            None,
                        );
                        stats.java_packages += 1;
                    }
                }
            }
        }
    }
}

/// Find the best JAR in a Gradle artifact directory.
fn find_gradle_jar(artifact_dir: &Path) -> Option<PathBuf> {
    // Find latest version
    let mut versions: Vec<_> = std::fs::read_dir(artifact_dir).ok()?
        .flatten()
        .filter(|e| e.path().is_dir())
        .collect();

    versions.sort_by(|a, b| {
        let a_name = a.file_name().to_string_lossy().to_string();
        let b_name = b.file_name().to_string_lossy().to_string();
        external_packages::version_cmp(&a_name, &b_name)
    });

    let version_dir = versions.last()?.path();

    // Look through hash directories
    if let Ok(hashes) = std::fs::read_dir(&version_dir) {
        for hash_entry in hashes.flatten() {
            let hash_path = hash_entry.path();
            if !hash_path.is_dir() {
                continue;
            }

            // Prefer sources JAR
            if let Ok(files) = std::fs::read_dir(&hash_path) {
                for file_entry in files.flatten() {
                    let name = file_entry.file_name().to_string_lossy().to_string();
                    if name.ends_with("-sources.jar") {
                        return Some(file_entry.path());
                    }
                }
            }

            // Fall back to regular JAR
            if let Ok(files) = std::fs::read_dir(&hash_path) {
                for file_entry in files.flatten() {
                    let name = file_entry.file_name().to_string_lossy().to_string();
                    if name.ends_with(".jar") && !name.ends_with("-sources.jar") && !name.ends_with("-javadoc.jar") {
                        return Some(file_entry.path());
                    }
                }
            }
        }
    }

    None
}

/// Index C/C++ system headers.
fn index_cpp_packages(
    index: &external_packages::PackageIndex,
    stats: &mut IndexPackagesStats,
    json: bool,
) {
    let cpp = moss_languages::cpp::Cpp;
    let version = moss_languages::c_cpp::get_gcc_version()
        .and_then(|v| external_packages::Version::parse(&v));

    if !json {
        println!("Indexing C/C++ headers (version {:?})...", version);
    }

    let include_paths = moss_languages::c_cpp::find_cpp_include_paths();

    if include_paths.is_empty() {
        if !json {
            println!("  No include paths found");
        }
        return;
    }

    if !json {
        for path in &include_paths {
            println!("  Include path: {}", path.display());
        }
    }

    let min_version = version.unwrap_or(external_packages::Version { major: 11, minor: 0 });
    let mut extractor = skeleton::SkeletonExtractor::new();

    // Index headers from each include path
    for include_path in &include_paths {
        index_cpp_directory(&cpp, index, include_path, include_path, min_version, &mut extractor, stats);
    }
}

/// Recursively index a C/C++ include directory.
fn index_cpp_directory(
    cpp: &moss_languages::cpp::Cpp,
    index: &external_packages::PackageIndex,
    base_path: &Path,
    current: &Path,
    min_version: external_packages::Version,
    extractor: &mut skeleton::SkeletonExtractor,
    stats: &mut IndexPackagesStats,
) {
    let entries = match std::fs::read_dir(current) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let is_dir = path.is_dir();

        if cpp.should_skip_package_entry(&name, is_dir) {
            continue;
        }

        if is_dir {
            index_cpp_directory(cpp, index, base_path, &path, min_version, extractor, stats);
        } else {
            // Get relative path from base
            let rel_path = path.strip_prefix(base_path)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| name.clone());

            // Skip if already indexed
            if let Ok(true) = index.is_indexed("cpp", &rel_path) {
                continue;
            }

            let pkg_id = match index.insert_package(
                "cpp",
                &rel_path,
                &path.to_string_lossy(),
                min_version,
                None,
            ) {
                Ok(id) => id,
                Err(_) => continue,
            };

            stats.cpp_packages += 1;

            // Extract symbols from header
            if let Ok(content) = std::fs::read_to_string(&path) {
                let result = extractor.extract(&path, &content);
                stats.cpp_symbols += count_and_insert_symbols(index, pkg_id, &result.symbols);
            }
        }
    }
}

/// Index Rust crates from cargo registry.
fn index_rust_packages(
    index: &external_packages::PackageIndex,
    stats: &mut IndexPackagesStats,
    json: bool,
) {
    let rust = moss_languages::rust::Rust;
    let version = moss_languages::rust::get_rust_version()
        .and_then(|v| external_packages::Version::parse(&v));

    if !json {
        println!("Indexing Rust crates (version {:?})...", version);
    }

    let registry = match rust.find_package_cache(Path::new(".")) {
        Some(r) => r,
        None => {
            if !json {
                println!("  No cargo registry found");
            }
            return;
        }
    };

    if !json {
        println!("  Cargo registry: {}", registry.display());
    }

    let min_version = version.unwrap_or(external_packages::Version { major: 1, minor: 0 });
    let mut extractor = skeleton::SkeletonExtractor::new();

    // Registry structure: ~/.cargo/registry/src/index.crates.io-*/crate-version/
    if let Ok(indices) = std::fs::read_dir(&registry) {
        for index_entry in indices.flatten() {
            let index_path = index_entry.path();
            if !index_path.is_dir() {
                continue;
            }

            if let Ok(crates) = std::fs::read_dir(&index_path) {
                for crate_entry in crates.flatten() {
                    let crate_path = crate_entry.path();
                    let crate_name = crate_entry.file_name().to_string_lossy().to_string();

                    // Skip if not a directory
                    if !crate_path.is_dir() {
                        continue;
                    }

                    // Extract crate name (remove version suffix)
                    // e.g., "serde-1.0.193" -> "serde"
                    let name = crate_name.rsplit_once('-')
                        .map(|(n, _)| n)
                        .unwrap_or(&crate_name);

                    // Skip if already indexed
                    if let Ok(true) = index.is_indexed("rust", name) {
                        continue;
                    }

                    // Find src/lib.rs
                    let lib_rs = crate_path.join("src").join("lib.rs");
                    if !lib_rs.is_file() {
                        continue;
                    }

                    let pkg_id = match index.insert_package(
                        "rust",
                        name,
                        &lib_rs.to_string_lossy(),
                        min_version,
                        None,
                    ) {
                        Ok(id) => id,
                        Err(_) => continue,
                    };

                    stats.rust_packages += 1;

                    if let Ok(content) = std::fs::read_to_string(&lib_rs) {
                        let result = extractor.extract(&lib_rs, &content);
                        stats.rust_symbols += count_and_insert_symbols(index, pkg_id, &result.symbols);
                    }
                }
            }
        }
    }
}

/// Count symbols and insert them into the index.
fn count_and_insert_symbols(
    index: &external_packages::PackageIndex,
    pkg_id: i64,
    symbols: &[skeleton::SkeletonSymbol],
) -> usize {
    let mut count = 0;
    for sym in symbols {
        let _ = index.insert_symbol(
            pkg_id,
            &sym.name,
            sym.kind,
            &sym.signature,
            sym.start_line as u32,
        );
        count += 1;
        count += count_and_insert_symbols(index, pkg_id, &sym.children);
    }
    count
}
