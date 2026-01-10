//! Sessions command - analyze Claude Code and other agent session logs.

mod analyze;
mod list;
mod plans;
#[cfg(feature = "sessions-web")]
mod serve;
mod show;
mod stats;

pub use list::cmd_sessions_list;
#[cfg(feature = "sessions-web")]
pub use serve::cmd_sessions_serve;
pub use show::cmd_sessions_show;
pub use stats::cmd_sessions_stats;

use clap::{Args, Subcommand};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use crate::sessions::FormatRegistry;

/// Sessions command arguments
#[derive(Args)]
pub struct SessionsArgs {
    #[command(subcommand)]
    pub command: Option<SessionsCommand>,

    /// Session ID or path (optional - lists sessions if omitted)
    pub session: Option<String>,

    /// Root directory (defaults to current directory)
    #[arg(short, long, global = true)]
    pub root: Option<PathBuf>,

    /// Apply jq filter to each JSONL line
    #[arg(long, global = true)]
    pub jq: Option<String>,

    /// Force specific format: claude, codex, gemini, moss
    #[arg(long, global = true)]
    pub format: Option<String>,

    /// Filter sessions by grep pattern (searches prompt/commands)
    #[arg(long, global = true)]
    pub grep: Option<String>,

    /// Run full analysis instead of dumping raw log
    #[arg(short, long, global = true)]
    pub analyze: bool,

    /// Show aggregate statistics across all sessions
    #[arg(long, global = true)]
    pub stats: bool,

    /// Start web server for viewing sessions
    #[arg(long, global = true)]
    pub serve: bool,

    /// Port for web server (default: 3939)
    #[arg(long, default_value = "3939", global = true)]
    pub port: u16,

    /// Limit number of sessions to list
    #[arg(short, long, default_value = "20", global = true)]
    pub limit: usize,
}

#[derive(Subcommand)]
pub enum SessionsCommand {
    /// List and view agent plans (from ~/.claude/plans/, etc.)
    Plans {
        /// Plan name to view (omit to list all plans)
        name: Option<String>,
    },
}

/// Run the sessions command
pub fn run(args: SessionsArgs, json: bool, pretty: bool) -> i32 {
    // Handle subcommands first
    if let Some(cmd) = args.command {
        return match cmd {
            SessionsCommand::Plans { name } => plans::cmd_plans(name.as_deref(), args.limit, json),
        };
    }

    // Existing flag-based dispatch
    if args.serve {
        #[cfg(feature = "sessions-web")]
        {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
            rt.block_on(cmd_sessions_serve(args.root.as_deref(), args.port))
        }
        #[cfg(not(feature = "sessions-web"))]
        {
            eprintln!("Sessions web server requires the 'sessions-web' feature");
            eprintln!("Rebuild with: cargo build --features sessions-web");
            1
        }
    } else if args.stats {
        cmd_sessions_stats(
            args.root.as_deref(),
            args.limit,
            args.format.as_deref(),
            args.grep.as_deref(),
            json,
            pretty,
        )
    } else if let Some(session_id) = args.session {
        cmd_sessions_show(
            &session_id,
            args.root.as_deref(),
            args.jq.as_deref(),
            args.format.as_deref(),
            args.analyze,
            json,
            pretty,
        )
    } else {
        cmd_sessions_list(
            args.root.as_deref(),
            args.limit,
            args.format.as_deref(),
            args.grep.as_deref(),
            json,
        )
    }
}

/// Check if a session file matches a grep pattern.
/// Searches through the raw JSONL content for any match.
pub(crate) fn session_matches_grep(path: &Path, pattern: &regex::Regex) -> bool {
    let Ok(file) = File::open(path) else {
        return false;
    };
    let reader = BufReader::new(file);
    for line in reader.lines().map_while(Result::ok) {
        if pattern.is_match(&line) {
            return true;
        }
    }
    false
}

/// Get the Claude Code sessions directory for a project.
pub(crate) fn get_sessions_dir(project: Option<&Path>) -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let claude_dir = PathBuf::from(home).join(".claude/projects");

    // Helper to convert a path to Claude's format: /home/user/foo -> -home-user-foo
    let path_to_claude_dir = |path: &Path| -> Option<PathBuf> {
        let path_str = path.to_string_lossy().replace('/', "-");
        // Try with leading dash first (Claude's format)
        let proj_dir = claude_dir.join(format!("-{}", path_str.trim_start_matches('-')));
        if proj_dir.exists() {
            return Some(proj_dir);
        }
        // Try without leading dash
        let proj_dir = claude_dir.join(&path_str);
        if proj_dir.exists() {
            return Some(proj_dir);
        }
        None
    };

    // 1. Explicit project path
    if let Some(proj) = project
        && let Some(dir) = path_to_claude_dir(proj)
    {
        return Some(dir);
    }

    // 2. Git root of current directory
    if let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        && output.status.success()
        && let Some(dir) =
            path_to_claude_dir(Path::new(String::from_utf8_lossy(&output.stdout).trim()))
    {
        return Some(dir);
    }

    // 3. Current directory
    if let Ok(cwd) = std::env::current_dir()
        && let Some(dir) = path_to_claude_dir(&cwd)
    {
        return Some(dir);
    }

    None
}

/// Resolve a session ID pattern to matching paths.
/// Supports:
/// - Exact ID: "3585080f-a55a-4a39-9666-02d970c3e144"
/// - Prefix: "3585" or "3585*"
/// - Glob: "agent-*"
/// - Full path: "/path/to/session.jsonl"
pub(crate) fn resolve_session_paths(
    session_id: &str,
    project: Option<&Path>,
    format: Option<&str>,
) -> Vec<PathBuf> {
    // If it's already a path, use it directly
    if session_id.contains('/') || session_id.ends_with(".jsonl") {
        let path = PathBuf::from(session_id);
        if path.exists() {
            return vec![path];
        }
        return vec![];
    }

    // Use format-specific sessions directory
    let registry = FormatRegistry::new();
    let sessions_dir = if let Some(fmt_name) = format {
        if let Some(fmt) = registry.get(fmt_name) {
            let dir = fmt.sessions_dir(project);
            if dir.exists() {
                dir
            } else {
                return vec![];
            }
        } else {
            return vec![];
        }
    } else {
        // Default to claude for backwards compatibility
        match get_sessions_dir(project) {
            Some(d) => d,
            None => return vec![],
        }
    };

    // Check for exact match first
    let exact_path = sessions_dir.join(format!("{}.jsonl", session_id));
    if exact_path.exists() {
        return vec![exact_path];
    }

    // Convert to pattern for matching
    let pattern = session_id.trim_end_matches('*');
    let is_glob = session_id.contains('*');

    let mut matches: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&sessions_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if !name_str.ends_with(".jsonl") {
                continue;
            }
            let stem = name_str.trim_end_matches(".jsonl");

            // Match by prefix
            if stem.starts_with(pattern) {
                matches.push(entry.path());
            }
        }
    }

    // Sort by modification time (newest first)
    matches.sort_by(|a, b| {
        let mtime_a = a.metadata().and_then(|m| m.modified()).ok();
        let mtime_b = b.metadata().and_then(|m| m.modified()).ok();
        mtime_b.cmp(&mtime_a)
    });

    // If not a glob pattern, return only the first match
    if !is_glob && matches.len() > 1 {
        matches.truncate(1);
    }

    matches
}

/// Format age in human-readable form.
pub(crate) fn format_age(seconds: u64) -> String {
    if seconds < 60 {
        format!("{}s ago", seconds)
    } else if seconds < 3600 {
        format!("{}m ago", seconds / 60)
    } else if seconds < 86400 {
        format!("{}h ago", seconds / 3600)
    } else {
        format!("{}d ago", seconds / 86400)
    }
}
