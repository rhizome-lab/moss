use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::time::Instant;

mod anchors;
mod cfg;
mod complexity;
mod daemon;
mod deps;
mod health;
mod index;
mod path_resolve;
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
            eprintln!("  {:20} {:>8.2}ms (+{:.2}ms)", name, elapsed.as_secs_f64() * 1000.0, delta.as_secs_f64() * 1000.0);
            prev = *elapsed;
        }
        eprintln!("  {:20} {:>8.2}ms", "total", self.start.elapsed().as_secs_f64() * 1000.0);
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

    /// View a file or symbol (shows content)
    View {
        /// Target to view (file path or symbol name)
        target: String,

        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,

        /// Show line numbers
        #[arg(short = 'n', long)]
        line_numbers: bool,
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
        /// Symbol to expand (function, class, or method name)
        symbol: String,

        /// File to search in (optional, will search all files if not provided)
        #[arg(short, long)]
        file: Option<String>,

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
        /// Symbol name to analyze
        symbol: String,

        /// File containing the symbol (optional - will search if not provided)
        #[arg(short, long)]
        file: Option<String>,

        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,
    },

    /// Find symbols that call a given symbol
    Callers {
        /// Symbol name to find callers for
        symbol: String,

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

    /// Show codebase health metrics
    Health {
        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,
    },

    /// Summarize what a module does
    Summarize {
        /// File to summarize
        file: String,

        /// Root directory (defaults to current directory)
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

    /// Start the daemon
    Start,
}

/// Parse file:symbol, file::symbol, or file#symbol syntax
/// Returns (file, symbol) where file is Some if separator found
fn parse_file_symbol(input: &str, explicit_file: Option<String>) -> (Option<String>, String) {
    // If explicit file provided, use it
    if explicit_file.is_some() {
        return (explicit_file, input.to_string());
    }

    // Try various separators: #, ::, :
    for sep in &['#', ':'] {
        if let Some(idx) = input.find(*sep) {
            let (file_part, sym_part) = input.split_at(idx);
            if !file_part.is_empty() {
                // Handle :: (skip first char)
                let sym = sym_part.trim_start_matches(*sep);
                if !sym.is_empty() {
                    return (Some(file_part.to_string()), sym.to_string());
                }
            }
        }
    }

    // No separator - just symbol
    (None, input.to_string())
}

fn main() {
    let cli = Cli::parse();
    let mut profiler = Profiler::new(cli.profile);
    profiler.mark("parsed_args");

    let exit_code = match cli.command {
        Commands::Path { query, root } => cmd_path(&query, root.as_deref(), cli.json, &mut profiler),
        Commands::View { target, root, line_numbers } => {
            cmd_view(&target, root.as_deref(), line_numbers, cli.json)
        }
        Commands::SearchTree { query, root, limit } => {
            cmd_search_tree(&query, root.as_deref(), limit, cli.json)
        }
        Commands::Reindex { root, call_graph } => cmd_reindex(root.as_deref(), call_graph),
        Commands::Expand { symbol, file, root } => {
            cmd_expand(&symbol, file.as_deref(), root.as_deref(), cli.json)
        }
        Commands::Symbols { file, root } => cmd_symbols(&file, root.as_deref(), cli.json, &mut profiler),
        Commands::Callees { symbol, file, root } => {
            // Support file:symbol, file::symbol, or file#symbol syntax
            let (actual_file, actual_symbol) = parse_file_symbol(&symbol, file);
            cmd_callees(&actual_symbol, actual_file.as_deref(), root.as_deref(), cli.json)
        }
        Commands::Callers { symbol, root } => cmd_callers(&symbol, root.as_deref(), cli.json, &mut profiler),
        Commands::Tree { path, root, depth, dirs_only } => {
            cmd_tree(&path, root.as_deref(), depth, dirs_only, cli.json)
        }
        Commands::Skeleton { file, root, docstrings } => {
            cmd_skeleton(&file, root.as_deref(), docstrings, cli.json, &mut profiler)
        }
        Commands::Anchors { file, root, query } => {
            cmd_anchors(&file, root.as_deref(), query.as_deref(), cli.json)
        }
        Commands::Deps { file, root, imports_only, exports_only } => {
            cmd_deps(&file, root.as_deref(), imports_only, exports_only, cli.json)
        }
        Commands::Imports { query, root, resolve } => {
            cmd_imports(&query, root.as_deref(), resolve, cli.json)
        }
        Commands::Complexity { file, root, threshold } => {
            cmd_complexity(&file, root.as_deref(), threshold, cli.json)
        }
        Commands::Cfg { file, root, function } => {
            cmd_cfg(&file, root.as_deref(), function.as_deref(), cli.json)
        }
        Commands::Daemon { action, root } => cmd_daemon(action, root.as_deref(), cli.json),
        Commands::Health { root } => cmd_health(root.as_deref(), cli.json, &mut profiler),
        Commands::Summarize { file, root } => cmd_summarize(&file, root.as_deref(), cli.json),
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

fn cmd_view(target: &str, root: Option<&Path>, line_numbers: bool, json: bool) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // Resolve the target to a file
    let matches = path_resolve::resolve(target, &root);

    if matches.is_empty() {
        eprintln!("No matches for: {}", target);
        return 1;
    }

    // Take the first file match
    let file_match = matches
        .iter()
        .find(|m| m.kind == "file")
        .unwrap_or(&matches[0]);

    let file_path = root.join(&file_match.path);

    match std::fs::read_to_string(&file_path) {
        Ok(content) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "path": file_match.path,
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
            0
        }
        Err(e) => {
            eprintln!("Error reading {}: {}", file_match.path, e);
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
    let limited: Vec<_> = matches.into_iter().take(limit).collect();

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
                                println!("Indexed {} symbols, {} calls, {} imports", symbols, calls, imports);
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
            .filter(|m| {
                m.kind == "file"
                    && (m.path.ends_with(".py") || m.path.ends_with(".rs"))
            })
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

    let mut parser = symbols::SymbolParser::new();
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
                matches.iter().find(|m| m.kind == "file").map(|m| m.path.clone())
            } else {
                // Find file from symbol
                idx.find_symbol(symbol).ok().and_then(|syms| syms.first().map(|(f, _, _, _)| f.clone()))
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
            eprintln!("No callees found for: {} (index has {} calls)", symbol, calls);
            return 1;
        }
    }

    // Fallback to parsing (slower) - also auto-indexes like callers
    eprintln!("Call graph not indexed. Building now (one-time)...");

    if let Ok(mut idx) = index::FileIndex::open(&root) {
        if idx.needs_refresh() {
            if let Err(e) = idx.refresh() {
                eprintln!("Failed to refresh file index: {}", e);
                return 1;
            }
        }
        match idx.refresh_call_graph() {
            Ok((symbols, calls, imports)) => {
                eprintln!("Indexed {} symbols, {} calls, {} imports", symbols, calls, imports);

                // Retry with index
                let file_path = if let Some(file) = file {
                    let matches = path_resolve::resolve(file, &root);
                    matches.iter().find(|m| m.kind == "file").map(|m| m.path.clone())
                } else {
                    idx.find_symbol(symbol).ok().and_then(|syms| syms.first().map(|(f, _, _, _)| f.clone()))
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
            eprintln!("No callers found for: {} (index has {} calls)", symbol, calls);
            return 1;
        }
    }
    profiler.mark("index_miss");

    // Index empty - auto-reindex (faster than file scan)
    eprintln!("Call graph not indexed. Building now (one-time)...");

    if let Ok(mut idx) = index::FileIndex::open(&root) {
        // Ensure file index is populated first
        if idx.needs_refresh() {
            if let Err(e) = idx.refresh() {
                eprintln!("Failed to refresh file index: {}", e);
                return 1;
            }
        }
        profiler.mark("file_index");

        // Now build call graph
        match idx.refresh_call_graph() {
            Ok((symbols, calls, imports)) => {
                eprintln!("Indexed {} symbols, {} calls, {} imports", symbols, calls, imports);
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

fn cmd_tree(path: &str, root: Option<&Path>, depth: Option<usize>, dirs_only: bool, json: bool) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // Resolve the path if it's not "."
    let target_dir = if path == "." {
        root.clone()
    } else {
        let matches = path_resolve::resolve(path, &root);
        match matches.iter().find(|m| m.kind == "directory") {
            Some(m) => root.join(&m.path),
            None => {
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
        println!("{} directories, {} files", result.dir_count, result.file_count);
    }

    0
}

fn cmd_skeleton(file: &str, root: Option<&Path>, include_docstrings: bool, json: bool, profiler: &mut Profiler) -> i32 {
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

    let mut extractor = anchors::AnchorExtractor::new();

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

fn cmd_deps(file: &str, root: Option<&Path>, imports_only: bool, exports_only: bool, json: bool) -> i32 {
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

    let mut extractor = deps::DepsExtractor::new();
    let result = extractor.extract(&file_path, &content);

    if json {
        let imports_json: Vec<_> = if !exports_only {
            result.imports.iter().map(|i| {
                serde_json::json!({
                    "module": i.module,
                    "names": i.names,
                    "alias": i.alias,
                    "line": i.line,
                    "is_relative": i.is_relative
                })
            }).collect()
        } else {
            Vec::new()
        };

        let exports_json: Vec<_> = if !imports_only {
            result.exports.iter().map(|e| {
                serde_json::json!({
                    "name": e.name,
                    "kind": e.kind,
                    "line": e.line
                })
            }).collect()
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
                    let alias = imp.alias.as_ref().map(|a| format!(" as {}", a)).unwrap_or_default();
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

fn cmd_imports(query: &str, root: Option<&Path>, resolve: bool, json: bool) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // Try index first, but fall back to direct parsing if not available
    let idx = index::FileIndex::open(&root).ok();
    let import_count = idx.as_ref()
        .and_then(|i| i.call_graph_stats().ok())
        .map(|(_, _, imports)| imports)
        .unwrap_or(0);

    // For resolve mode, we need the index - no direct fallback possible
    if resolve {
        if import_count == 0 {
            eprintln!("Import resolution requires indexed call graph. Run: moss reindex --call-graph");
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
                    println!("{}", serde_json::json!({
                        "name": name,
                        "module": module,
                        "original_name": orig_name
                    }));
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

        let mut extractor = deps::DepsExtractor::new();
        let result = extractor.extract(&full_path, &content);

        // Convert deps::Import to symbols::Import format for output
        let imports: Vec<symbols::Import> = result.imports.iter().flat_map(|imp| {
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
                imp.names.iter().map(|name| symbols::Import {
                    module: Some(imp.module.clone()),
                    name: name.clone(),
                    alias: None,
                    line: imp.line,
                }).collect()
            }
        }).collect();

        output_imports(&imports, file_path, json)
    }
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
            let alias = imp.alias.as_ref().map(|a| format!(" as {}", a)).unwrap_or_default();
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

    let mut analyzer = complexity::ComplexityAnalyzer::new();
    let report = analyzer.analyze(&file_path, &content);

    // Filter by threshold if specified
    let functions: Vec<_> = if let Some(t) = threshold {
        report.functions.into_iter().filter(|f| f.complexity >= t).collect()
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
            println!("\nNo functions found{}",
                threshold.map(|t| format!(" above threshold {}", t)).unwrap_or_default());
        } else {
            let avg = functions.iter().map(|f| f.complexity).sum::<usize>() as f64 / functions.len() as f64;
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
                let parent = f.parent.as_ref().map(|p| format!("{}.", p)).unwrap_or_default();
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

    match action {
        DaemonAction::Status => {
            if !client.is_available() {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "running": false,
                            "socket": root.join(".moss/daemon.sock").to_string_lossy()
                        })
                    );
                } else {
                    eprintln!("Daemon is not running");
                    eprintln!("Socket: {}", root.join(".moss/daemon.sock").display());
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
                    println!("{}", serde_json::json!({"success": false, "error": "daemon not running"}));
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
                    println!("{}", serde_json::json!({"success": false, "error": "daemon already running"}));
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
                    println!("{}", serde_json::json!({"success": false, "error": "failed to start daemon"}));
                } else {
                    eprintln!("Failed to start daemon");
                }
                1
            }
        }
    }
}

fn cmd_health(root: Option<&Path>, json: bool, profiler: &mut Profiler) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    profiler.mark("resolved_root");

    let report = health::analyze_health(&root);
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
                "avg_complexity": (report.avg_complexity * 10.0).round() / 10.0,
                "max_complexity": report.max_complexity,
                "high_risk_functions": report.high_risk_functions,
            })
        );
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
