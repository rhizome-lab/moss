use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

mod anchors;
mod complexity;
mod daemon;
mod deps;
mod index;
mod path_resolve;
mod skeleton;
mod symbols;
mod tree;

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

        /// File containing the symbol
        #[arg(short, long)]
        file: String,

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
}

fn main() {
    let cli = Cli::parse();

    let exit_code = match cli.command {
        Commands::Path { query, root } => cmd_path(&query, root.as_deref(), cli.json),
        Commands::View { target, root, line_numbers } => {
            cmd_view(&target, root.as_deref(), line_numbers, cli.json)
        }
        Commands::SearchTree { query, root, limit } => {
            cmd_search_tree(&query, root.as_deref(), limit, cli.json)
        }
        Commands::Reindex { root } => cmd_reindex(root.as_deref()),
        Commands::Expand { symbol, file, root } => {
            cmd_expand(&symbol, file.as_deref(), root.as_deref(), cli.json)
        }
        Commands::Symbols { file, root } => cmd_symbols(&file, root.as_deref(), cli.json),
        Commands::Callees { symbol, file, root } => {
            cmd_callees(&symbol, &file, root.as_deref(), cli.json)
        }
        Commands::Callers { symbol, root } => cmd_callers(&symbol, root.as_deref(), cli.json),
        Commands::Tree { path, root, depth, dirs_only } => {
            cmd_tree(&path, root.as_deref(), depth, dirs_only, cli.json)
        }
        Commands::Skeleton { file, root, docstrings } => {
            cmd_skeleton(&file, root.as_deref(), docstrings, cli.json)
        }
        Commands::Anchors { file, root, query } => {
            cmd_anchors(&file, root.as_deref(), query.as_deref(), cli.json)
        }
        Commands::Deps { file, root, imports_only, exports_only } => {
            cmd_deps(&file, root.as_deref(), imports_only, exports_only, cli.json)
        }
        Commands::Complexity { file, root, threshold } => {
            cmd_complexity(&file, root.as_deref(), threshold, cli.json)
        }
    };

    std::process::exit(exit_code);
}

fn cmd_path(query: &str, root: Option<&Path>, json: bool) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // Try daemon first
    let client = daemon::DaemonClient::new(&root);
    if client.is_available() {
        if let Ok(matches) = client.path_query(query) {
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
            return 0;
        }
        // Fall through to direct if daemon query failed
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

fn cmd_reindex(root: Option<&Path>) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    match index::FileIndex::open(&root) {
        Ok(mut idx) => {
            match idx.refresh() {
                Ok(count) => {
                    println!("Indexed {} files", count);
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

fn cmd_symbols(file: &str, root: Option<&Path>, json: bool) -> i32 {
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

    let mut parser = symbols::SymbolParser::new();
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

fn cmd_callees(symbol: &str, file: &str, root: Option<&Path>, json: bool) -> i32 {
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

    let mut parser = symbols::SymbolParser::new();
    let callees = parser.find_callees(&file_path, &content, symbol);

    if callees.is_empty() {
        eprintln!("No callees found for: {}", symbol);
        return 1;
    }

    if json {
        println!("{}", serde_json::to_string(&callees).unwrap());
    } else {
        println!("Callees of {}:", symbol);
        for callee in &callees {
            println!("  {}", callee);
        }
    }

    0
}

fn cmd_callers(symbol: &str, root: Option<&Path>, json: bool) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // Get all files (not just fuzzy matches)
    let all_paths = path_resolve::all_files(&root);
    let files: Vec<_> = all_paths.into_iter().map(|m| (m.path, m.kind == "directory")).collect();

    let mut parser = symbols::SymbolParser::new();
    let callers = parser.find_callers(&root, &files, symbol);

    if callers.is_empty() {
        eprintln!("No callers found for: {}", symbol);
        return 1;
    }

    if json {
        let output: Vec<_> = callers
            .iter()
            .map(|(file, sym)| serde_json::json!({"file": file, "symbol": sym}))
            .collect();
        println!("{}", serde_json::to_string(&output).unwrap());
    } else {
        println!("Callers of {}:", symbol);
        for (file, sym) in &callers {
            println!("  {}:{}", file, sym);
        }
    }

    0
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

fn cmd_skeleton(file: &str, root: Option<&Path>, include_docstrings: bool, json: bool) -> i32 {
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

    let mut extractor = skeleton::SkeletonExtractor::new();
    let result = extractor.extract(&file_path, &content);

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
