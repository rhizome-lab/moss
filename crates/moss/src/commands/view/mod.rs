//! View command - unified view of files, directories, and symbols.

pub mod file;
pub mod history;
pub mod lines;
pub mod search;
pub mod symbol;
pub mod tree;

use crate::commands::aliases::detect_project_languages;
use crate::config::MossConfig;
use crate::filter::Filter;
use crate::{daemon, path_resolve};
use clap::Args;
use rhizome_moss_derive::Merge;
use serde::Deserialize;
use std::path::{Path, PathBuf};

pub use search::search_symbols;

/// View command configuration.
#[derive(Debug, Clone, Deserialize, serde::Serialize, Default, Merge, schemars::JsonSchema)]
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
        self.line_numbers.unwrap_or(true)
    }

    pub fn show_docs(&self) -> bool {
        self.show_docs.unwrap_or(false)
    }
}

/// View command arguments.
///
/// # Target Syntax
///
/// | Syntax | Description |
/// |--------|-------------|
/// | `.` | Current directory tree |
/// | `path/to/file` | File skeleton (symbols) |
/// | `path/to/dir` | Directory tree |
/// | `file/Symbol` | Symbol in file |
/// | `file/Parent/method` | Nested symbol |
/// | `Parent/method` | Symbol search (when Parent isn't a path) |
/// | `file:123` | Symbol containing line 123 |
/// | `file:10-20` | Lines 10-20 (raw) |
/// | `SymbolName` | Symbol search across codebase |
#[derive(Args, Debug)]
pub struct ViewArgs {
    /// Target: path, path/Symbol, Parent/method, file:line, or SymbolName
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

    /// Filter by symbol kind: class, function, method
    #[arg(short = 'k', long = "kind")]
    pub kind: Option<String>,

    /// Show only type definitions (class, struct, enum, interface, type alias)
    #[arg(long = "types-only")]
    pub types_only: bool,

    /// Include test functions and test modules (hidden by default)
    #[arg(long)]
    pub tests: bool,

    /// Disable smart display (no collapsing single-child dirs)
    #[arg(long)]
    pub raw: bool,

    /// Focus view: show target at high detail, imports at signature level
    #[arg(long, value_name = "MODULE", num_args = 0..=1, default_missing_value = "*", require_equals = true)]
    pub focus: Option<String>,

    /// Resolve imports: inline signatures of specific imported symbols
    #[arg(long)]
    pub resolve_imports: bool,

    /// Show full source code
    #[arg(long)]
    pub full: bool,

    /// Show full docstrings
    #[arg(long)]
    pub docs: bool,

    /// Hide parent/ancestor context (shown by default for nested symbols)
    #[arg(long)]
    pub no_parent: bool,

    /// Context view: skeleton + imports combined
    #[arg(long)]
    pub context: bool,

    /// Prepend directory context (.context.md files from hierarchy)
    #[arg(long)]
    pub dir_context: bool,

    /// Exclude paths matching pattern or @alias
    #[arg(long, value_name = "PATTERN", value_delimiter = ',')]
    pub exclude: Vec<String>,

    /// Include only paths matching pattern or @alias
    #[arg(long, value_name = "PATTERN", value_delimiter = ',')]
    pub only: Vec<String>,

    /// Case-insensitive symbol matching
    #[arg(short = 'i', long)]
    pub case_insensitive: bool,

    /// Show git history for symbol (last N changes)
    #[arg(long, value_name = "N", default_missing_value = "5", num_args = 0..=1)]
    pub history: Option<usize>,
}

/// Run view command with args.
pub fn run(args: ViewArgs, format: crate::output::OutputFormat) -> i32 {
    let effective_root = args
        .root
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let config = MossConfig::load(&effective_root);

    // Handle --history mode
    if let Some(limit) = args.history {
        return history::cmd_history(
            args.target.as_deref(),
            &effective_root,
            limit,
            args.case_insensitive,
            format.is_json(),
        );
    }

    // Handle --dir-context: prepend directory context before main output
    if args.dir_context && !format.is_json() {
        let target_path = args
            .target
            .as_ref()
            .map(|t| effective_root.join(t))
            .unwrap_or_else(|| effective_root.clone());
        if let Some(ctx) =
            crate::commands::context::get_merged_context(&effective_root, &target_path)
        {
            println!("{}", ctx);
            println!("\n---\n");
        }
    }

    cmd_view(
        args.target.as_deref(),
        args.root.as_deref(),
        args.depth.unwrap_or_else(|| config.view.depth()),
        args.line_numbers || config.view.line_numbers(),
        args.deps,
        args.kind.as_deref(),
        args.types_only,
        args.tests,
        args.raw,
        args.focus.as_deref(),
        args.resolve_imports,
        args.full,
        args.docs || config.view.show_docs(),
        args.context,
        !args.no_parent,
        format.is_json(),
        format.is_pretty(),
        format.use_colors(),
        &args.exclude,
        &args.only,
        args.case_insensitive,
    )
}

/// Unified view command
#[allow(clippy::too_many_arguments)]
pub fn cmd_view(
    target: Option<&str>,
    root: Option<&Path>,
    depth: i32,
    line_numbers: bool,
    show_deps: bool,
    kind_filter: Option<&str>,
    types_only: bool,
    show_tests: bool,
    raw: bool,
    focus: Option<&str>,
    resolve_imports: bool,
    full: bool,
    show_docs: bool,
    context: bool,
    show_parent: bool,
    json: bool,
    pretty: bool,
    use_colors: bool,
    exclude: &[String],
    only: &[String],
    case_insensitive: bool,
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

        match Filter::new(exclude, only, &config.aliases, &lang_refs) {
            Ok(f) => {
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
        return tree::cmd_view_filtered(&root, scope, kind, json);
    }

    // --focus requires a file target
    if focus.is_some() && target.is_none() {
        eprintln!("--focus requires a file target");
        return 1;
    }

    let target = target.unwrap_or(".");

    // Handle "." as current directory
    if target == "." {
        return tree::cmd_view_directory(
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

    // Handle line targets: file.rs:30 (symbol at line) or file.rs:30-55 (range)
    if let Some((file_path, line, end_opt)) = lines::parse_line_target(target) {
        if let Some(end) = end_opt {
            return lines::cmd_view_line_range(
                &file_path, line, end, &root, show_docs, json, pretty, use_colors,
            );
        } else {
            return symbol::cmd_view_symbol_at_line(
                &file_path,
                line,
                &root,
                depth,
                show_docs,
                show_parent,
                context,
                json,
                pretty,
                use_colors,
            );
        }
    }

    // Check if query looks like a symbol path (contains / but first segment isn't a real path)
    let has_file_extension = target
        .rsplit('/')
        .next()
        .map(|last| last.contains('.'))
        .unwrap_or(false);
    // Trailing slash means "directory only" - skip symbol search
    let dir_only = target.ends_with('/');

    let is_symbol_query = !dir_only  // Trailing slash = directory, not symbol
        && !target.starts_with('@')
        && target.contains('/')
        && !target.starts_with('/')
        && !has_file_extension
        && {
            let first_seg = target.split('/').next().unwrap_or("");
            !root.join(first_seg).exists()
        };

    // For symbol queries, only search symbols (skip file resolution)
    let (matches, symbol_matches) = if is_symbol_query {
        (Vec::new(), search::search_symbols(target, &root))
    } else {
        let matches = path_resolve::resolve_unified_all(target, &root);
        // Don't fall back to symbol search if query ends with /
        let symbol_matches = if matches.is_empty() && !dir_only {
            search::search_symbols(target, &root)
        } else {
            Vec::new()
        };
        (matches, symbol_matches)
    };

    let unified = match (matches.len(), symbol_matches.len()) {
        (0, 0) => {
            eprintln!("No matches for: {}", target);
            return 1;
        }
        (1, 0) => matches.into_iter().next().unwrap(),
        (0, 1) => {
            let sym = &symbol_matches[0];
            return symbol::cmd_view_symbol_direct(
                &sym.file,
                &sym.name,
                sym.parent.as_deref(),
                &root,
                depth,
                full,
                show_docs,
                show_parent,
                context,
                json,
                pretty,
                use_colors,
                case_insensitive,
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
        tree::cmd_view_directory(
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
        let effective_depth = if full { -1 } else { depth };
        file::cmd_view_file(
            &unified.file_path,
            &root,
            effective_depth,
            line_numbers,
            show_deps,
            types_only,
            show_tests,
            focus,
            resolve_imports,
            show_docs,
            context,
            json,
            pretty,
            use_colors,
        )
    } else {
        // Check if symbol path contains glob patterns
        let symbol_pattern = unified.symbol_path.join("/");
        if path_resolve::is_glob_pattern(&symbol_pattern) {
            return symbol::cmd_view_symbol_glob(
                &unified.file_path,
                &symbol_pattern,
                &root,
                depth,
                full,
                show_docs,
                json,
                pretty,
                use_colors,
                case_insensitive,
            );
        }

        symbol::cmd_view_symbol(
            &unified.file_path,
            &unified.symbol_path,
            &root,
            depth,
            full,
            show_docs,
            show_parent,
            context,
            json,
            pretty,
            use_colors,
            case_insensitive,
        )
    }
}
