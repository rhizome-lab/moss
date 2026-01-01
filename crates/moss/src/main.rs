use clap::builder::styling::{AnsiColor, Styles};
use clap::{ColorChoice, CommandFactory, FromArgMatches, Parser, Subcommand};
use std::path::{Path, PathBuf};

use moss::commands;
use moss::commands::analyze::AnalyzeArgs;
use moss::commands::edit::EditAction;
use moss::commands::text_search::TextSearchArgs;
use moss::commands::view::ViewArgs;
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

    /// Human-friendly output with colors and formatting
    #[arg(long, global = true, conflicts_with = "compact")]
    pretty: bool,

    /// Compact output without colors (overrides TTY detection)
    #[arg(long, global = true, conflicts_with = "pretty")]
    compact: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// View a node in the codebase tree (directory, file, or symbol)
    View(ViewArgs),

    /// Edit a node in the codebase tree (structural code modification)
    Edit {
        /// Target to edit (path like src/main.py/Foo/bar)
        target: String,

        /// Edit action to perform
        #[command(subcommand)]
        action: EditAction,

        /// Root directory (defaults to current directory)
        #[arg(short, long, global = true)]
        root: Option<PathBuf>,

        /// Dry run - show what would be changed without applying
        #[arg(long, global = true)]
        dry_run: bool,

        /// Exclude files matching patterns or aliases (e.g., @tests, *.test.js)
        #[arg(long, value_delimiter = ',', global = true)]
        exclude: Vec<String>,

        /// Only include files matching patterns or aliases
        #[arg(long, value_delimiter = ',', global = true)]
        only: Vec<String>,

        /// Allow glob patterns to match multiple symbols
        #[arg(long, global = true)]
        multiple: bool,
    },

    /// Manage file index
    Index {
        #[command(subcommand)]
        action: commands::index::IndexAction,

        /// Root directory (defaults to current directory)
        #[arg(short, long, global = true)]
        root: Option<PathBuf>,
    },

    /// Initialize moss in current directory
    Init(commands::init::InitArgs),

    /// Manage the global moss daemon
    Daemon {
        #[command(subcommand)]
        action: commands::daemon::DaemonAction,
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
    Analyze(AnalyzeArgs),

    /// Manage filter aliases
    Filter {
        #[command(subcommand)]
        action: commands::filter::FilterAction,

        /// Root directory (defaults to current directory)
        #[arg(short, long, global = true)]
        root: Option<PathBuf>,
    },

    /// Search for text patterns in files (fast ripgrep-based search)
    #[command(name = "text-search")]
    TextSearch(TextSearchArgs),

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

    /// Run Lua scripts
    Script {
        #[command(subcommand)]
        action: commands::script::ScriptAction,

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

    /// Run native test runners (cargo test, go test, bun test, etc.)
    Test {
        #[command(subcommand)]
        action: Option<TestAction>,

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
enum TestAction {
    /// Run tests (default when no subcommand given)
    Run {
        /// Specific test runner to use (cargo, go, bun, npm, pytest)
        #[arg(short = 'R', long)]
        runner: Option<String>,

        /// Additional arguments to pass to the test runner
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// List available test runners
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

/// Help output styling.
const HELP_STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default().bold())
    .usage(AnsiColor::Green.on_default().bold())
    .literal(AnsiColor::Cyan.on_default().bold())
    .placeholder(AnsiColor::Cyan.on_default());

/// Determine color choice for help output.
/// Checks args, config, and NO_COLOR before parsing since --help may exit early.
fn help_color_choice() -> ColorChoice {
    // NO_COLOR standard takes precedence
    if std::env::var("NO_COLOR").is_ok() {
        return ColorChoice::Never;
    }

    let args: Vec<String> = std::env::args().collect();
    let has_compact = args.iter().any(|a| a == "--compact");
    let has_pretty = args.iter().any(|a| a == "--pretty");

    // CLI flags override config
    if has_compact {
        return ColorChoice::Never;
    }
    if has_pretty {
        return ColorChoice::Always;
    }

    // Check config for color preference
    let config = moss::config::MossConfig::load(Path::new("."));
    match config.pretty.colors {
        Some(moss::output::ColorMode::Always) => ColorChoice::Always,
        Some(moss::output::ColorMode::Never) => ColorChoice::Never,
        _ => ColorChoice::Auto,
    }
}

/// Reset SIGPIPE to default behavior so piping to `head` etc. doesn't panic.
#[cfg(unix)]
fn reset_sigpipe() {
    // SAFETY: libc::signal is a standard POSIX function. We reset SIGPIPE to default
    // behavior (terminate on broken pipe) instead of Rust's default (ignore, causing
    // write errors). This prevents panics when output is piped to commands like `head`.
    // No memory safety concerns - just changes signal disposition.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn reset_sigpipe() {}

/// Run a script from .moss/scripts/ directory.
/// Returns Some(exit_code) if this was a script invocation, None otherwise.
fn try_run_script() -> Option<i32> {
    let args: Vec<String> = std::env::args().collect();

    // Need at least program name and @script-name
    if args.len() < 2 {
        return None;
    }

    let first_arg = &args[1];
    if !first_arg.starts_with('@') {
        return None;
    }

    let script_name = &first_arg[1..]; // Strip @
    if script_name.is_empty() {
        eprintln!("Error: script name required after @");
        return Some(1);
    }

    // Script args are everything after @script-name
    let script_args: Vec<&str> = args[2..].iter().map(|s| s.as_str()).collect();

    Some(commands::script::run_script(script_name, &script_args))
}

fn main() {
    reset_sigpipe();

    // Check for @script-name invocation before clap parsing
    if let Some(exit_code) = try_run_script() {
        std::process::exit(exit_code);
    }

    // Parse with custom styles and color choice
    let cli = Cli::command()
        .styles(HELP_STYLES)
        .color(help_color_choice())
        .get_matches();
    let cli = Cli::from_arg_matches(&cli).expect("clap mismatch");

    // Resolve output format at top level - pretty config is TTY-based, not root-specific
    let config = moss::config::MossConfig::load(Path::new("."));
    let format = moss::output::OutputFormat::from_cli(
        cli.json,
        cli.jq.as_deref(),
        cli.pretty,
        cli.compact,
        &config.pretty,
    );

    let exit_code = match cli.command {
        Commands::View(args) => commands::view::run(args, format),
        Commands::Edit {
            target,
            action,
            root,
            dry_run,
            exclude,
            only,
            multiple,
        } => commands::edit::cmd_edit(
            &target,
            action,
            root.as_deref(),
            dry_run,
            cli.json,
            &exclude,
            &only,
            multiple,
        ),
        Commands::Index { action, root } => {
            commands::index::cmd_index(action, root.as_deref(), cli.json)
        }
        Commands::Init(args) => commands::init::run(args),
        Commands::Daemon { action } => commands::daemon::cmd_daemon(action, cli.json),
        Commands::Update { check } => commands::update::cmd_update(check, cli.json),
        Commands::Grammars { action } => commands::grammars::cmd_grammars(action, cli.json),
        Commands::Analyze(args) => commands::analyze::run(args, format),
        Commands::Filter { action, root } => {
            commands::filter::cmd_filter(action, root.as_deref(), cli.json)
        }
        Commands::TextSearch(args) => commands::text_search::run(args, format),
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
        Commands::Package {
            action,
            ecosystem,
            root,
        } => commands::package::cmd_package(action, ecosystem.as_deref(), root.as_deref(), format),
        Commands::Script { action, root } => {
            commands::script::cmd_script(action, root.as_deref(), cli.json)
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
                            format,
                        )
                    }
                }
                LintAction::List => commands::lint::cmd_lint_list(root.as_deref(), &format),
            }
        }
        Commands::Test { action, root } => {
            let action = action.unwrap_or(TestAction::Run {
                runner: None,
                args: vec![],
            });
            match action {
                TestAction::Run { runner, args } => {
                    commands::test::cmd_test_run(root.as_deref(), runner.as_deref(), &args)
                }
                TestAction::List => commands::test::cmd_test_list(root.as_deref()),
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
