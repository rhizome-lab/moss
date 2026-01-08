use clap::builder::styling::{AnsiColor, Styles};
use clap::{ColorChoice, CommandFactory, FromArgMatches, Parser, Subcommand};
use std::path::{Path, PathBuf};

use moss::commands;
use moss::commands::aliases::AliasesArgs;
use moss::commands::analyze::AnalyzeArgs;
use moss::commands::context::ContextArgs;
use moss::commands::edit::EditArgs;
use moss::commands::generate::GenerateArgs;
use moss::commands::history::HistoryArgs;
use moss::commands::rules::RulesAction;
use moss::commands::sessions::SessionsArgs;
use moss::commands::text_search::TextSearchArgs;
use moss::commands::tools::ToolsAction;
use moss::commands::view::ViewArgs;
use moss::serve::{self, ServeArgs};

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
    Edit(EditArgs),

    /// View shadow git edit history
    History(HistoryArgs),

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

    /// Analyze codebase (health, complexity, security, duplicates, docs)
    Analyze(AnalyzeArgs),

    /// List filter aliases (used by --exclude/--only)
    Aliases(AliasesArgs),

    /// Show directory context (hierarchical .context.md files)
    Context(ContextArgs),

    /// Search for text patterns in files (fast ripgrep-based search)
    #[command(name = "text-search")]
    TextSearch(TextSearchArgs),

    /// Analyze Claude Code and other agent session logs
    Sessions(SessionsArgs),

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

    /// Run Lua scripts
    Script {
        #[command(subcommand)]
        action: commands::script::ScriptAction,

        /// Root directory (defaults to current directory)
        #[arg(short, long, global = true)]
        root: Option<PathBuf>,
    },

    /// External ecosystem tools (linters, formatters, test runners)
    Tools {
        #[command(subcommand)]
        action: ToolsAction,

        /// Root directory (defaults to current directory)
        #[arg(short, long, global = true)]
        root: Option<PathBuf>,
    },

    /// Start a moss server (MCP, HTTP, LSP)
    Serve(ServeArgs),

    /// Generate code from API spec
    Generate(GenerateArgs),

    /// Manage custom analysis rules
    Rules {
        #[command(subcommand)]
        action: RulesAction,
    },
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
        Commands::Edit(args) => commands::edit::run(args, cli.json),
        Commands::History(args) => commands::history::run(args, format),
        Commands::Index { action, root } => {
            commands::index::cmd_index(action, root.as_deref(), cli.json)
        }
        Commands::Init(args) => commands::init::run(args),
        Commands::Daemon { action } => commands::daemon::cmd_daemon(action, cli.json),
        Commands::Update { check } => commands::update::cmd_update(check, cli.json),
        Commands::Grammars { action } => commands::grammars::cmd_grammars(action, cli.json),
        Commands::Analyze(args) => commands::analyze::run(args, format),
        Commands::Aliases(args) => commands::aliases::run(args, cli.json),
        Commands::Context(args) => commands::context::run(args, format),
        Commands::TextSearch(args) => commands::text_search::run(args, format),
        Commands::Sessions(args) => commands::sessions::run(args, cli.json, cli.pretty),
        Commands::Package {
            action,
            ecosystem,
            root,
        } => commands::package::cmd_package(action, ecosystem.as_deref(), root.as_deref(), format),
        Commands::Script { action, root } => {
            commands::script::cmd_script(action, root.as_deref(), cli.json)
        }
        Commands::Tools { action, root } => {
            commands::tools::run(action, root.as_deref(), format, cli.json)
        }
        Commands::Serve(args) => serve::run(args, cli.json),
        Commands::Generate(args) => commands::generate::run(args),
        Commands::Rules { action } => commands::rules::cmd_rules(action, cli.json),
    };

    std::process::exit(exit_code);
}
