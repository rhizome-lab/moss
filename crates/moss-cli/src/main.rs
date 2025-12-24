use clap::{Parser, Subcommand};
use std::path::PathBuf;

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
mod sessions;
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

        /// Disable smart display (no collapsing single-child dirs)
        #[arg(long)]
        raw: bool,

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

    /// Analyze Claude Code and other agent session logs
    Sessions {
        /// Session ID or path (optional - lists sessions if omitted)
        session: Option<String>,

        /// Project path to find sessions for (defaults to current directory)
        #[arg(short, long)]
        project: Option<PathBuf>,

        /// Apply jq filter to each JSONL line
        #[arg(long)]
        jq: Option<String>,

        /// Force specific format: claude, gemini, moss
        #[arg(long)]
        format: Option<String>,

        /// Run full analysis instead of dumping raw log
        #[arg(short, long)]
        analyze: bool,

        /// Limit number of sessions to list
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },
}

fn main() {
    let cli = Cli::parse();

    let exit_code = match cli.command {
        Commands::Path { query, root } => {
            commands::path_cmd::cmd_path(&query, root.as_deref(), cli.json)
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
            raw,
            focus,
            resolve_imports,
            all,
            full,
        } => commands::view_cmd::cmd_view(
            target.as_deref(),
            root.as_deref(),
            depth,
            line_numbers,
            deps,
            kind.as_deref(),
            calls,
            called_by,
            types_only,
            raw,
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
            commands::expand::cmd_expand(&symbol, file.as_deref(), root.as_deref(), cli.json)
        }
        Commands::Symbols { file, root } => {
            commands::symbols_cmd::cmd_symbols(&file, root.as_deref(), cli.json)
        }
        Commands::Callees { args, root } => {
            let (symbol, file) = normalize_symbol_args(&args);
            commands::callees::cmd_callees(&symbol, file.as_deref(), root.as_deref(), cli.json)
        }
        Commands::Callers { args, root } => {
            let (symbol, _file) = normalize_symbol_args(&args);
            commands::callers::cmd_callers(&symbol, root.as_deref(), cli.json)
        }
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
        } => commands::analyze_cmd::cmd_analyze(
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
        Commands::Summarize { file, root } => commands::summarize_cmd::cmd_summarize(&file, root.as_deref(), cli.json),
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
        } => commands::find_symbols::cmd_find_symbols(
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
        Commands::Sessions {
            session,
            project,
            jq,
            format,
            analyze,
            limit,
        } => {
            if let Some(session_id) = session {
                commands::sessions::cmd_sessions_show(
                    &session_id,
                    project.as_deref(),
                    jq.as_deref(),
                    format.as_deref(),
                    analyze,
                    cli.json,
                )
            } else {
                commands::sessions::cmd_sessions_list(project.as_deref(), limit, cli.json)
            }
        }
    };

    std::process::exit(exit_code);
}
