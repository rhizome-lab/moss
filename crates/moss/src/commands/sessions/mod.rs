//! Sessions command - analyze Claude Code and other agent session logs.

pub mod analyze;
pub mod list;
pub mod plans;
#[cfg(feature = "sessions-web")]
mod serve;
pub mod show;
pub mod stats;

pub use list::cmd_sessions_list;
#[cfg(feature = "sessions-web")]
pub use serve::cmd_sessions_serve;
pub use show::cmd_sessions_show;
pub use stats::cmd_sessions_stats;

use clap::{Args, Subcommand};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use crate::sessions::{FormatRegistry, LogFormat};

/// Format an age in seconds to a human-readable string.
pub(crate) fn format_age(secs: u64) -> String {
    if secs < 60 {
        format!("{}s ago", secs)
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

/// Resolve a session identifier to one or more file paths.
/// Supports: full path, session ID, glob pattern.
pub(crate) fn resolve_session_paths(
    session_id: &str,
    project: Option<&Path>,
    format_name: Option<&str>,
) -> Vec<PathBuf> {
    let session_path = Path::new(session_id);

    // If it's a full path, use it directly
    if session_path.is_file() {
        return vec![session_path.to_path_buf()];
    }

    // If it looks like a glob pattern, expand it
    if session_id.contains('*') || session_id.contains('?') {
        if let Ok(entries) = glob::glob(session_id) {
            let paths: Vec<_> = entries
                .filter_map(|e| e.ok())
                .filter(|p| p.is_file())
                .collect();
            if !paths.is_empty() {
                return paths;
            }
        }
    }

    // Otherwise, try to find it as a session ID in the format's directory
    let registry = FormatRegistry::new();
    let format: &dyn LogFormat = match format_name {
        Some(name) => match registry.get(name) {
            Some(f) => f,
            None => return Vec::new(),
        },
        None => registry.get("claude").unwrap(),
    };

    let sessions = format.list_sessions(project);

    // Match by session ID prefix (file stem)
    for s in &sessions {
        if let Some(stem) = s.path.file_stem().and_then(|s| s.to_str()) {
            if stem == session_id || stem.starts_with(session_id) {
                return vec![s.path.clone()];
            }
        }
    }

    // No match
    Vec::new()
}

/// Sessions command arguments
#[derive(Args)]
pub struct SessionsArgs {
    #[command(subcommand)]
    pub command: Option<SessionsCommand>,

    /// Root directory (defaults to current directory)
    #[arg(short, long, global = true)]
    pub root: Option<PathBuf>,

    /// Force specific format: claude, codex, gemini, moss
    #[arg(long, global = true)]
    pub format: Option<String>,

    /// Limit number of sessions
    #[arg(short, long, default_value = "20", global = true)]
    pub limit: usize,
}

#[derive(Subcommand)]
pub enum SessionsCommand {
    /// List sessions
    List {
        /// Filter sessions by grep pattern (searches prompt/commands)
        #[arg(long)]
        grep: Option<String>,

        /// Filter sessions from the last N days
        #[arg(long)]
        days: Option<u32>,

        /// Filter sessions since date (YYYY-MM-DD)
        #[arg(long)]
        since: Option<String>,

        /// Filter sessions until date (YYYY-MM-DD)
        #[arg(long)]
        until: Option<String>,

        /// Filter by specific project path
        #[arg(long)]
        project: Option<PathBuf>,

        /// Show sessions from all projects (not just current)
        #[arg(long)]
        all_projects: bool,
    },

    /// Show a specific session
    Show {
        /// Session ID or path
        session: String,

        /// Apply jq filter to each JSONL line
        #[arg(long)]
        jq: Option<String>,

        /// Run full analysis instead of dumping raw log
        #[arg(short, long)]
        analyze: bool,
    },

    /// Show aggregate statistics across sessions
    Stats {
        /// Filter sessions by grep pattern
        #[arg(long)]
        grep: Option<String>,

        /// Filter sessions from the last N days
        #[arg(long)]
        days: Option<u32>,

        /// Filter sessions since date (YYYY-MM-DD)
        #[arg(long)]
        since: Option<String>,

        /// Filter sessions until date (YYYY-MM-DD)
        #[arg(long)]
        until: Option<String>,

        /// Filter by specific project path
        #[arg(long)]
        project: Option<PathBuf>,

        /// Show sessions from all projects (not just current)
        #[arg(long)]
        all_projects: bool,
    },

    /// Start web server for viewing sessions
    #[cfg(feature = "sessions-web")]
    Serve {
        /// Port for web server
        #[arg(long, default_value = "3939")]
        port: u16,
    },

    /// List and view agent plans (from ~/.claude/plans/, etc.)
    Plans {
        /// Plan name to view (omit to list all plans)
        name: Option<String>,
    },
}

/// Run the sessions command
pub fn run(args: SessionsArgs, json: bool, pretty: bool) -> i32 {
    match args.command {
        Some(SessionsCommand::List {
            grep,
            days,
            since,
            until,
            project,
            all_projects,
        }) => cmd_sessions_list_filtered(
            args.root.as_deref(),
            args.limit,
            args.format.as_deref(),
            grep.as_deref(),
            days,
            since.as_deref(),
            until.as_deref(),
            project.as_deref(),
            all_projects,
            json,
        ),

        Some(SessionsCommand::Show {
            session,
            jq,
            analyze,
        }) => cmd_sessions_show(
            &session,
            args.root.as_deref(),
            jq.as_deref(),
            args.format.as_deref(),
            analyze,
            json,
            pretty,
        ),

        Some(SessionsCommand::Stats {
            grep,
            days,
            since,
            until,
            project,
            all_projects,
        }) => cmd_sessions_stats(
            args.root.as_deref(),
            args.limit,
            args.format.as_deref(),
            grep.as_deref(),
            days,
            since.as_deref(),
            until.as_deref(),
            project.as_deref(),
            all_projects,
            json,
            pretty,
        ),

        #[cfg(feature = "sessions-web")]
        Some(SessionsCommand::Serve { port }) => {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
            rt.block_on(cmd_sessions_serve(args.root.as_deref(), port))
        }

        Some(SessionsCommand::Plans { name }) => {
            plans::cmd_plans(name.as_deref(), args.limit, json)
        }

        // Default: list sessions
        None => cmd_sessions_list(
            args.root.as_deref(),
            args.limit,
            args.format.as_deref(),
            None,
            json,
        ),
    }
}

/// List sessions with filtering support
fn cmd_sessions_list_filtered(
    root: Option<&Path>,
    limit: usize,
    format_name: Option<&str>,
    grep: Option<&str>,
    days: Option<u32>,
    since: Option<&str>,
    until: Option<&str>,
    project: Option<&Path>,
    all_projects: bool,
    json: bool,
) -> i32 {
    // For now, delegate to stats module's filtering logic but output as list
    // TODO: Refactor to share filtering between list and stats
    use crate::sessions::{FormatRegistry, LogFormat, SessionFile};
    use std::time::{Duration, SystemTime};

    let registry = FormatRegistry::new();
    let format: &dyn LogFormat = match format_name {
        Some(name) => match registry.get(name) {
            Some(f) => f,
            None => {
                eprintln!("Unknown format: {}", name);
                return 1;
            }
        },
        None => registry.get("claude").unwrap(),
    };

    // Compile grep pattern
    let grep_re = grep.map(|p| regex::Regex::new(p).ok()).flatten();
    if grep.is_some() && grep_re.is_none() {
        eprintln!("Invalid grep pattern: {}", grep.unwrap());
        return 1;
    }

    // Get sessions
    let mut sessions: Vec<SessionFile> = if all_projects {
        stats::list_all_project_sessions(format)
    } else {
        let proj = project.or(root);
        format.list_sessions(proj)
    };

    // Date filtering
    let now = SystemTime::now();
    if let Some(d) = days {
        let since_time = now - Duration::from_secs(d as u64 * 86400);
        sessions.retain(|s| s.mtime >= since_time);
    }
    if let Some(s) = since {
        if let Some(since_time) = stats::parse_date(s) {
            sessions.retain(|s| s.mtime >= since_time);
        }
    }
    if let Some(u) = until {
        if let Some(until_time) = stats::parse_date(u) {
            let until_time = until_time + Duration::from_secs(86400);
            sessions.retain(|s| s.mtime <= until_time);
        }
    }

    // Grep filtering
    if let Some(ref re) = grep_re {
        sessions.retain(|s| session_matches_grep(&s.path, re));
    }

    // Sort and limit
    sessions.sort_by(|a, b| b.mtime.cmp(&a.mtime));
    if limit > 0 {
        sessions.truncate(limit);
    }

    // Output
    if json {
        let paths: Vec<_> = sessions
            .iter()
            .map(|s| s.path.display().to_string())
            .collect();
        println!("{}", serde_json::to_string_pretty(&paths).unwrap());
    } else {
        for s in &sessions {
            println!("{}", s.path.display());
        }
        if !sessions.is_empty() {
            eprintln!("\n{} sessions", sessions.len());
        }
    }

    0
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
