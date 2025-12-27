use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

use moss::commands;
use moss::serve;

#[derive(Parser)]
#[command(name = "moss")]
#[command(about = "Fast code intelligence CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output as JSON
    #[arg(long, global = true)]
    json: bool,

    /// Filter JSON output with jq expression (implies --json)
    #[arg(long, global = true, value_name = "EXPR")]
    jq: Option<String>,
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

        /// Show only type definitions (class, struct, enum, interface, type alias)
        /// Filters out functions/methods for architectural overview
        #[arg(long = "types-only")]
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
        #[arg(long = "include-private")]
        include_private: bool,

        /// Show full source code (for symbols: complete implementation, for files: raw content)
        #[arg(long)]
        full: bool,

        /// Show full docstrings (by default only summary up to double blank line is shown)
        #[arg(long)]
        docs: bool,

        /// Context view: skeleton + imports combined (ideal for LLM context)
        #[arg(long)]
        context: bool,

        /// Exclude paths matching pattern or @alias (repeatable)
        /// Patterns: globs like "*.test.js", "**/tests/**"
        /// Aliases: @tests, @config, @build, @docs, @generated
        #[arg(long, value_name = "PATTERN")]
        exclude: Vec<String>,

        /// Include only paths matching pattern or @alias (repeatable)
        #[arg(long, value_name = "PATTERN")]
        only: Vec<String>,
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

        /// Exclude files matching patterns or aliases (e.g., @tests, *.test.js)
        #[arg(long, value_delimiter = ',')]
        exclude: Vec<String>,

        /// Only include files matching patterns or aliases
        #[arg(long, value_delimiter = ',')]
        only: Vec<String>,
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

    /// Manage tree-sitter grammars for parsing
    Grammars {
        #[command(subcommand)]
        action: commands::grammars::GrammarAction,
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

        /// Show what functions the target calls
        #[arg(long)]
        callees: bool,

        /// Show what functions call the target
        #[arg(long)]
        callers: bool,

        /// Run linters and include results in analysis
        #[arg(long)]
        lint: bool,

        /// Show git history hotspots (high churn + high complexity)
        #[arg(long)]
        hotspots: bool,

        /// Check documentation references (broken links in docs)
        #[arg(long)]
        check_refs: bool,

        /// Find docs with stale code references (covered code changed since doc was modified)
        #[arg(long)]
        stale_docs: bool,

        /// Check that all {{example: path#name}} references have matching markers
        #[arg(long)]
        check_examples: bool,

        /// Exclude paths matching pattern or @alias (repeatable)
        #[arg(long, value_name = "PATTERN")]
        exclude: Vec<String>,

        /// Include only paths matching pattern or @alias (repeatable)
        #[arg(long, value_name = "PATTERN")]
        only: Vec<String>,
    },

    /// Manage filter aliases
    Filter {
        #[command(subcommand)]
        action: commands::filter::FilterAction,

        /// Root directory (defaults to current directory)
        #[arg(short, long, global = true)]
        root: Option<PathBuf>,
    },

    /// Search for text patterns in files (fast ripgrep-based search)
    Grep {
        /// Regex pattern to search for
        pattern: String,

        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,

        /// Maximum number of matches to return
        #[arg(short, long, default_value = "100")]
        limit: usize,

        /// Case-insensitive search
        #[arg(short = 'i', long)]
        ignore_case: bool,

        /// Exclude files matching patterns or aliases (e.g., @tests, *.test.js)
        #[arg(long, value_delimiter = ',')]
        exclude: Vec<String>,

        /// Only include files matching patterns or aliases (e.g., @docs, *.py)
        #[arg(long, value_delimiter = ',')]
        only: Vec<String>,
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

    /// Package management: info, list, tree, outdated
    Package {
        #[command(subcommand)]
        action: commands::package::PackageAction,

        /// Force specific ecosystem (cargo, npm, python)
        #[arg(short, long, global = true)]
        ecosystem: Option<String>,

        /// Root directory (defaults to current directory)
        #[arg(short, long, global = true)]
        root: Option<PathBuf>,
    },

    /// List and view Claude Code plans from ~/.claude/plans/
    Plans {
        /// Plan name to view (omit to list all plans)
        name: Option<String>,

        /// Limit number of plans to list
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },

    /// Structured TODO.md editing (prevents content loss)
    Todo {
        /// Action: add, done, list (default: list)
        action: Option<String>,

        /// Item text (for add) or index (for done)
        item: Option<String>,

        /// Show full TODO.md content
        #[arg(short, long)]
        full: bool,

        /// Root directory (defaults to current directory)
        #[arg(short, long)]
        root: Option<PathBuf>,
    },

    /// Run TOML-defined workflows
    Workflow {
        #[command(subcommand)]
        action: commands::workflow::WorkflowAction,

        /// Root directory (defaults to current directory)
        #[arg(short, long, global = true)]
        root: Option<PathBuf>,
    },

    /// Run linters, formatters, and type checkers
    Lint {
        #[command(subcommand)]
        action: Option<LintAction>,

        /// Root directory (defaults to current directory)
        #[arg(short, long, global = true)]
        root: Option<PathBuf>,
    },

    /// Start a moss server (MCP, HTTP, LSP)
    Serve {
        #[command(subcommand)]
        protocol: ServeProtocol,

        /// Root directory (defaults to current directory)
        #[arg(short, long, global = true)]
        root: Option<PathBuf>,
    },

    /// Generate code from API spec
    Generate {
        #[command(subcommand)]
        target: GenerateTarget,
    },
}

#[derive(Subcommand)]
enum GenerateTarget {
    /// Generate API client from OpenAPI spec
    Client {
        /// OpenAPI spec JSON file
        spec: PathBuf,

        /// Target language: typescript, python, rust
        #[arg(short, long)]
        lang: String,

        /// Output file (stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Generate types from JSON Schema
    Types {
        /// JSON Schema file
        schema: PathBuf,

        /// Root type name
        #[arg(short, long, default_value = "Root")]
        name: String,

        /// Target language: typescript, python, rust
        #[arg(short, long)]
        lang: String,

        /// Output file (stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum LintAction {
    /// Run linters on target (default when no subcommand given)
    Run {
        /// Target path to check (defaults to current directory)
        target: Option<String>,

        /// Fix issues automatically where possible
        #[arg(short, long)]
        fix: bool,

        /// Specific tools to run (comma-separated, e.g., "ruff,oxlint")
        #[arg(short, long)]
        tools: Option<String>,

        /// Filter by category: lint, fmt, type
        #[arg(short, long)]
        category: Option<String>,

        /// Output in SARIF format
        #[arg(long)]
        sarif: bool,

        /// Watch for file changes and re-run on save
        #[arg(short, long)]
        watch: bool,
    },

    /// List available linting tools
    List,
}

#[derive(Subcommand)]
enum ServeProtocol {
    /// Start MCP server for LLM integration (stdio transport)
    Mcp,

    /// Start HTTP server (REST API)
    Http {
        /// Port to listen on
        #[arg(short, long, default_value = "8080")]
        port: u16,

        /// Output OpenAPI spec and exit (don't start server)
        #[arg(long)]
        openapi: bool,
    },

    /// Start LSP server for IDE integration
    Lsp,
}

/// Reset SIGPIPE to default behavior so piping to `head` etc. doesn't panic.
#[cfg(unix)]
fn reset_sigpipe() {
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn reset_sigpipe() {}

fn main() {
    reset_sigpipe();
    let cli = Cli::parse();

    let exit_code = match cli.command {
        Commands::View {
            target,
            root,
            depth,
            line_numbers,
            deps,
            kind,
            types_only,
            raw,
            focus,
            resolve_imports,
            include_private,
            full,
            docs,
            context,
            exclude,
            only,
        } => commands::view::cmd_view(
            target.as_deref(),
            root.as_deref(),
            depth,
            line_numbers,
            deps,
            kind.as_deref(),
            types_only,
            raw,
            focus.as_deref(),
            resolve_imports,
            include_private,
            full,
            docs,
            context,
            cli.json,
            &exclude,
            &only,
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
            exclude,
            only,
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
            &exclude,
            &only,
        ),
        Commands::Index { action, root } => {
            commands::index::cmd_index(action, root.as_deref(), cli.json)
        }
        Commands::Daemon { action, root } => {
            commands::daemon::cmd_daemon(action, root.as_deref(), cli.json)
        }
        Commands::Update { check } => commands::update::cmd_update(check, cli.json),
        Commands::Grammars { action } => commands::grammars::cmd_grammars(action, cli.json),
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
            callees,
            callers,
            lint,
            hotspots,
            check_refs,
            stale_docs,
            check_examples,
            exclude,
            only,
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
            callees,
            callers,
            lint,
            hotspots,
            check_refs,
            stale_docs,
            check_examples,
            cli.json,
            &exclude,
            &only,
        ),
        Commands::Filter { action, root } => {
            commands::filter::cmd_filter(action, root.as_deref(), cli.json)
        }
        Commands::Grep {
            pattern,
            root,
            limit,
            ignore_case,
            exclude,
            only,
        } => commands::grep::cmd_grep(
            &pattern,
            root.as_deref(),
            limit,
            ignore_case,
            cli.json,
            cli.jq.as_deref(),
            &exclude,
            &only,
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
        Commands::Plans { name, limit } => {
            commands::plans::cmd_plans(name.as_deref(), limit, cli.json)
        }
        Commands::Todo {
            action,
            item,
            full,
            root,
        } => {
            let root = root.as_deref().unwrap_or(Path::new("."));
            let index = item.as_ref().and_then(|s| s.parse::<usize>().ok());
            commands::todo::cmd_todo(
                action.as_deref(),
                item.as_deref(),
                index,
                full,
                cli.json,
                root,
            )
        }
        Commands::Package {
            action,
            ecosystem,
            root,
        } => {
            commands::package::cmd_package(action, ecosystem.as_deref(), root.as_deref(), cli.json)
        }
        Commands::Workflow { action, root } => {
            commands::workflow::cmd_workflow(action, root.as_deref(), cli.json)
        }
        Commands::Lint { action, root } => {
            let action = action.unwrap_or(LintAction::Run {
                target: None,
                fix: false,
                tools: None,
                category: None,
                sarif: false,
                watch: false,
            });
            match action {
                LintAction::Run {
                    target,
                    fix,
                    tools,
                    category,
                    sarif,
                    watch,
                } => {
                    if watch {
                        commands::lint::cmd_lint_watch(
                            target.as_deref(),
                            root.as_deref(),
                            fix,
                            tools.as_deref(),
                            category.as_deref(),
                            cli.json,
                        )
                    } else {
                        commands::lint::cmd_lint_run(
                            target.as_deref(),
                            root.as_deref(),
                            fix,
                            tools.as_deref(),
                            category.as_deref(),
                            sarif,
                            cli.json,
                        )
                    }
                }
                LintAction::List => {
                    commands::lint::cmd_lint_list(root.as_deref(), cli.json, cli.jq.as_deref())
                }
            }
        }
        Commands::Serve { protocol, root } => match protocol {
            ServeProtocol::Mcp => serve::mcp::cmd_serve_mcp(root.as_deref(), cli.json),
            ServeProtocol::Http { port, openapi } => {
                if openapi {
                    // Output OpenAPI spec and exit
                    use serve::http::ApiDoc;
                    use utoipa::OpenApi;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&ApiDoc::openapi()).unwrap()
                    );
                    0
                } else {
                    let root = root.unwrap_or_else(|| std::path::PathBuf::from("."));
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(serve::http::run_http_server(&root, port))
                }
            }
            ServeProtocol::Lsp => {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(serve::lsp::run_lsp_server(root.as_deref()))
            }
        },
        Commands::Generate { target } => match target {
            GenerateTarget::Client { spec, lang, output } => {
                let Some(generator) = moss_openapi::find_generator(&lang) else {
                    eprintln!("Unknown language: {}. Available:", lang);
                    for (lang, variant) in moss_openapi::list_generators() {
                        eprintln!("  {} ({})", lang, variant);
                    }
                    std::process::exit(1);
                };

                let content = match std::fs::read_to_string(&spec) {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("Failed to read {}: {}", spec.display(), e);
                        std::process::exit(1);
                    }
                };
                let spec_json: serde_json::Value = match serde_json::from_str(&content) {
                    Ok(j) => j,
                    Err(e) => {
                        eprintln!("Failed to parse JSON: {}", e);
                        std::process::exit(1);
                    }
                };

                let code = generator.generate(&spec_json);

                if let Some(path) = output {
                    if let Err(e) = std::fs::write(&path, &code) {
                        eprintln!("Failed to write {}: {}", path.display(), e);
                        std::process::exit(1);
                    }
                    eprintln!("Generated {}", path.display());
                } else {
                    print!("{}", code);
                }
                0
            }
            GenerateTarget::Types {
                schema,
                name,
                lang,
                output,
            } => {
                let Some(generator) = moss_jsonschema::find_generator(&lang) else {
                    eprintln!("Unknown language: {}. Available:", lang);
                    for l in moss_jsonschema::list_generators() {
                        eprintln!("  {}", l);
                    }
                    std::process::exit(1);
                };

                let content = match std::fs::read_to_string(&schema) {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("Failed to read {}: {}", schema.display(), e);
                        std::process::exit(1);
                    }
                };
                let schema_json: serde_json::Value = match serde_json::from_str(&content) {
                    Ok(j) => j,
                    Err(e) => {
                        eprintln!("Failed to parse JSON: {}", e);
                        std::process::exit(1);
                    }
                };

                let code = generator.generate(&schema_json, &name);

                if let Some(path) = output {
                    if let Err(e) = std::fs::write(&path, &code) {
                        eprintln!("Failed to write {}: {}", path.display(), e);
                        std::process::exit(1);
                    }
                    eprintln!("Generated {}", path.display());
                } else {
                    print!("{}", code);
                }
                0
            }
        },
    };

    std::process::exit(exit_code);
}
