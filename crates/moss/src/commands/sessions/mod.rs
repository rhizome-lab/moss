//! Sessions command - analyze Claude Code and other agent session logs.

mod serve;

pub use serve::cmd_sessions_serve;

use crate::sessions::{FormatRegistry, LogFormat, SessionFile, analyze_session};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// List available sessions for a format.
pub fn cmd_sessions_list(
    project: Option<&Path>,
    limit: usize,
    format_name: Option<&str>,
    grep: Option<&str>,
    json: bool,
) -> i32 {
    let registry = FormatRegistry::new();

    // Get format (default to claude for backwards compatibility)
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

    // Compile grep pattern if provided
    let grep_re = grep.map(|p| regex::Regex::new(p).ok()).flatten();
    if grep.is_some() && grep_re.is_none() {
        eprintln!("Invalid grep pattern: {}", grep.unwrap());
        return 1;
    }

    // Get sessions from format (handles directory structure differences)
    let mut sessions: Vec<SessionFile> = format.list_sessions(project);

    // Apply grep filter if provided
    if let Some(ref re) = grep_re {
        sessions.retain(|s| session_matches_grep(&s.path, re));
    }

    sessions.sort_by(|a, b| b.mtime.cmp(&a.mtime));
    sessions.truncate(limit);

    if sessions.is_empty() {
        if json {
            println!("[]");
        } else {
            eprintln!("No {} sessions found", format_name.unwrap_or("Claude Code"));
        }
        return 0;
    }

    if json {
        let output: Vec<_> = sessions
            .iter()
            .map(|s| {
                let id = s.path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                let age = s.mtime.elapsed().map(|d| d.as_secs()).unwrap_or(0);
                serde_json::json!({
                    "id": id,
                    "path": s.path,
                    "age_seconds": age
                })
            })
            .collect();
        println!("{}", serde_json::to_string(&output).unwrap());
    } else {
        for s in &sessions {
            let id = s
                .path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("");
            let age = format_age(s.mtime.elapsed().map(|d| d.as_secs()).unwrap_or(0));
            println!("{} ({})", id, age);
        }
    }

    0
}

/// Show aggregate statistics across all sessions.
pub fn cmd_sessions_stats(
    project: Option<&Path>,
    limit: usize,
    format_name: Option<&str>,
    grep: Option<&str>,
    json: bool,
) -> i32 {
    let registry = FormatRegistry::new();

    // Get format (default to claude for backwards compatibility)
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

    // Compile grep pattern if provided
    let grep_re = grep.map(|p| regex::Regex::new(p).ok()).flatten();
    if grep.is_some() && grep_re.is_none() {
        eprintln!("Invalid grep pattern: {}", grep.unwrap());
        return 1;
    }

    // Get sessions from format
    let mut sessions: Vec<SessionFile> = format.list_sessions(project);

    // Apply grep filter if provided
    if let Some(ref re) = grep_re {
        sessions.retain(|s| session_matches_grep(&s.path, re));
    }

    // Sort by time (newest first) and limit
    sessions.sort_by(|a, b| b.mtime.cmp(&a.mtime));
    sessions.truncate(limit);

    if sessions.is_empty() {
        if json {
            println!("{{}}");
        } else {
            eprintln!("No {} sessions found", format_name.unwrap_or("Claude Code"));
        }
        return 0;
    }

    // Collect paths and analyze
    let paths: Vec<_> = sessions.iter().map(|s| s.path.clone()).collect();
    cmd_sessions_analyze_multi(&paths, format_name, json)
}

/// Check if a session file matches a grep pattern.
/// Searches through the raw JSONL content for any match.
fn session_matches_grep(path: &Path, pattern: &regex::Regex) -> bool {
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

/// Show/analyze a specific session or sessions matching a pattern.
pub fn cmd_sessions_show(
    session_id: &str,
    project: Option<&Path>,
    jq_filter: Option<&str>,
    format: Option<&str>,
    analyze: bool,
    json: bool,
) -> i32 {
    // Find matching session files
    let paths = resolve_session_paths(session_id, project, format);

    if paths.is_empty() {
        eprintln!("No sessions found matching: {}", session_id);
        return 1;
    }

    // If --analyze with multiple sessions, aggregate
    if analyze && paths.len() > 1 {
        return cmd_sessions_analyze_multi(&paths, format, json);
    }

    // If --analyze with single session
    if analyze {
        return cmd_sessions_analyze(&paths[0], format, json);
    }

    // If --jq with multiple sessions, apply to all
    if let Some(filter) = jq_filter {
        let mut exit_code = 0;
        for path in &paths {
            let code = cmd_sessions_jq(path, filter);
            if code != 0 {
                exit_code = code;
            }
        }
        return exit_code;
    }

    // Default: dump the raw JSONL (only first match for non-glob)
    let path = &paths[0];
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to open {}: {}", path.display(), e);
            return 1;
        }
    };

    let reader = BufReader::new(file);
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();

    for line in reader.lines() {
        match line {
            Ok(l) => {
                let _ = writeln!(stdout, "{}", l);
            }
            Err(e) => {
                eprintln!("Read error: {}", e);
                return 1;
            }
        }
    }

    0
}

/// Analyze a session and output statistics.
fn cmd_sessions_analyze(path: &Path, format: Option<&str>, json: bool) -> i32 {
    let analysis = if let Some(fmt) = format {
        crate::sessions::analyze_session_with_format(path, fmt)
    } else {
        analyze_session(path)
    };

    match analysis {
        Ok(a) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&a).unwrap());
            } else {
                println!("{}", a.to_markdown());
            }
            0
        }
        Err(e) => {
            eprintln!("Analysis failed: {}", e);
            1
        }
    }
}

/// Analyze multiple sessions and aggregate statistics.
fn cmd_sessions_analyze_multi(paths: &[PathBuf], format: Option<&str>, json: bool) -> i32 {
    use crate::sessions::{SessionAnalysis, ToolStats};

    let mut aggregate = SessionAnalysis::new(PathBuf::from("."), "aggregate");
    let mut session_count = 0;

    for path in paths {
        let analysis = if let Some(fmt) = format {
            crate::sessions::analyze_session_with_format(path, fmt)
        } else {
            analyze_session(path)
        };

        match analysis {
            Ok(a) => {
                session_count += 1;

                // Aggregate message counts
                for (k, v) in a.message_counts {
                    *aggregate.message_counts.entry(k).or_insert(0) += v;
                }

                // Aggregate tool stats
                for (k, v) in a.tool_stats {
                    let stat = aggregate
                        .tool_stats
                        .entry(k.clone())
                        .or_insert_with(|| ToolStats::new(&k));
                    stat.calls += v.calls;
                    stat.errors += v.errors;
                }

                // Aggregate token stats
                aggregate.token_stats.total_input += a.token_stats.total_input;
                aggregate.token_stats.total_output += a.token_stats.total_output;
                aggregate.token_stats.cache_read += a.token_stats.cache_read;
                aggregate.token_stats.cache_create += a.token_stats.cache_create;
                aggregate.token_stats.api_calls += a.token_stats.api_calls;
                if a.token_stats.min_context > 0 {
                    aggregate
                        .token_stats
                        .update_context(a.token_stats.min_context);
                }
                if a.token_stats.max_context > 0 {
                    aggregate
                        .token_stats
                        .update_context(a.token_stats.max_context);
                }

                // Aggregate file tokens
                for (k, v) in a.file_tokens {
                    *aggregate.file_tokens.entry(k).or_insert(0) += v;
                }

                aggregate.total_turns += a.total_turns;
                aggregate.parallel_opportunities += a.parallel_opportunities;
            }
            Err(e) => {
                eprintln!("Warning: Failed to analyze {}: {}", path.display(), e);
            }
        }
    }

    if session_count == 0 {
        eprintln!("No sessions could be analyzed");
        return 1;
    }

    // Update format to show aggregate info
    aggregate.format = format!("aggregate ({} sessions)", session_count);

    if json {
        println!("{}", serde_json::to_string_pretty(&aggregate).unwrap());
    } else {
        println!("{}", aggregate.to_markdown());
    }

    0
}

/// Apply jq filter to each line of a JSONL file.
fn cmd_sessions_jq(path: &Path, filter: &str) -> i32 {
    use jaq_core::load::{Arena, File as JaqFile, Loader};
    use jaq_core::{Compiler, Ctx, RcIter};
    use jaq_json::Val;

    // Set up loader with standard library
    let loader = Loader::new(jaq_std::defs().chain(jaq_json::defs()));
    let arena = Arena::default();

    // Parse the filter
    let program = JaqFile {
        code: filter,
        path: (),
    };

    let modules = match loader.load(&arena, program) {
        Ok(m) => m,
        Err(errs) => {
            for e in errs {
                eprintln!("jq parse error: {:?}", e);
            }
            return 1;
        }
    };

    // Compile the filter
    let filter_compiled = match Compiler::default()
        .with_funs(jaq_std::funs().chain(jaq_json::funs()))
        .compile(modules)
    {
        Ok(f) => f,
        Err(errs) => {
            for e in errs {
                eprintln!("jq compile error: {:?}", e);
            }
            return 1;
        }
    };

    // Process each line
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to open {}: {}", path.display(), e);
            return 1;
        }
    };

    let reader = BufReader::new(file);
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Read error: {}", e);
                return 1;
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        let json_val: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let val = Val::from(json_val);
        let inputs = RcIter::new(core::iter::empty());
        let out = filter_compiled.run((Ctx::new([], &inputs), val));

        for result in out {
            match result {
                Ok(v) => {
                    let _ = writeln!(stdout, "{}", v);
                }
                Err(e) => {
                    eprintln!("jq error: {:?}", e);
                }
            }
        }
    }

    0
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
