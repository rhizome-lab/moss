use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod analyze;
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
mod sessions;
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

        /// Show what functions the target calls (callees)
        #[arg(long)]
        calls: bool,

        /// Show what functions call the target (callers)
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

    /// Manage file index
    Index {
        #[command(subcommand)]
        action: commands::index::IndexAction,

        /// Root directory (defaults to current directory)
        #[arg(short, long, global = true)]
        root: Option<PathBuf>,
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

    /// Analyze codebase (unified health, complexity, security, overview)
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

        /// Show comprehensive project overview
        #[arg(long)]
        overview: bool,

        /// Show storage usage (index database, caches)
        #[arg(long)]
        storage: bool,

        /// Compact one-line output (for --overview)
        #[arg(short, long)]
        compact: bool,

        /// Complexity threshold - only show functions above this
        #[arg(short, long)]
        threshold: Option<usize>,

        /// Filter by symbol kind: function, method
        #[arg(long)]
        kind: Option<String>,
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

    /// Query package registry for package information
    Package {
        /// Package name to query
        package: String,

        /// Force specific ecosystem (cargo, npm, python)
        #[arg(short, long)]
        ecosystem: Option<String>,

        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,
    },
}

fn main() {
    let cli = Cli::parse();

    let exit_code = match cli.command {
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
        } => commands::view::cmd_view(
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
        Commands::Index { action, root } => commands::index::cmd_index(action, root.as_deref(), cli.json),
        Commands::Daemon { action, root } => commands::daemon::cmd_daemon(action, root.as_deref(), cli.json),
        Commands::Update { check } => commands::update::cmd_update(check, cli.json),
        Commands::Analyze {
            target,
            root,
            health,
            complexity,
            security,
            overview,
            storage,
            compact,
            threshold,
            kind,
        } => commands::analyze::cmd_analyze(
            target.as_deref(),
            root.as_deref(),
            health,
            complexity,
            security,
            overview,
            storage,
            compact,
            threshold,
            kind.as_deref(),
            cli.json,
        ),
        Commands::Grep {
            pattern,
            root,
            glob,
            limit,
            ignore_case,
        } => commands::grep::cmd_grep(
            &pattern,
            root.as_deref(),
            glob.as_deref(),
            limit,
            ignore_case,
            cli.json,
        ),
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
        Commands::Package {
            package,
            ecosystem,
            root,
        } => commands::package::cmd_package(
            &package,
            ecosystem.as_deref(),
            root.as_deref(),
            cli.json,
        ),
    };

    std::process::exit(exit_code);
}
