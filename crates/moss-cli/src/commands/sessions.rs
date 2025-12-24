//! Sessions command - analyze Claude Code and other agent session logs.

use crate::sessions::analyze_session;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// List available sessions in the Claude Code projects directory.
pub fn cmd_sessions_list(project: Option<&Path>, limit: usize, json: bool) -> i32 {
    let sessions_dir = get_sessions_dir(project);

    let Some(dir) = sessions_dir else {
        eprintln!("Could not find Claude Code sessions directory");
        return 1;
    };

    if !dir.exists() {
        eprintln!("Sessions directory not found: {}", dir.display());
        return 1;
    }

    // Find all .jsonl files, sorted by modification time (newest first)
    let mut sessions: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                if let Ok(meta) = path.metadata() {
                    if let Ok(mtime) = meta.modified() {
                        sessions.push((path, mtime));
                    }
                }
            }
        }
    }

    sessions.sort_by(|a, b| b.1.cmp(&a.1));
    sessions.truncate(limit);

    if sessions.is_empty() {
        if json {
            println!("[]");
        } else {
            eprintln!("No sessions found in {}", dir.display());
        }
        return 0;
    }

    if json {
        let output: Vec<_> = sessions
            .iter()
            .map(|(path, mtime)| {
                let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                let age = mtime
                    .elapsed()
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                serde_json::json!({
                    "id": id,
                    "path": path,
                    "age_seconds": age
                })
            })
            .collect();
        println!("{}", serde_json::to_string(&output).unwrap());
    } else {
        for (path, mtime) in &sessions {
            let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let age = format_age(mtime.elapsed().map(|d| d.as_secs()).unwrap_or(0));
            println!("{} ({})", id, age);
        }
    }

    0
}

/// Show/analyze a specific session.
pub fn cmd_sessions_show(
    session_id: &str,
    project: Option<&Path>,
    jq_filter: Option<&str>,
    format: Option<&str>,
    analyze: bool,
    json: bool,
) -> i32 {
    // Find the session file
    let session_path = resolve_session_path(session_id, project);

    let Some(path) = session_path else {
        eprintln!("Session not found: {}", session_id);
        return 1;
    };

    // If --analyze, run full analysis
    if analyze {
        return cmd_sessions_analyze(&path, format, json);
    }

    // If --jq, filter and output
    if let Some(filter) = jq_filter {
        return cmd_sessions_jq(&path, filter);
    }

    // Default: dump the raw JSONL
    let file = match File::open(&path) {
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
fn get_sessions_dir(project: Option<&Path>) -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let claude_dir = PathBuf::from(home).join(".claude/projects");

    if let Some(proj) = project {
        // Convert project path to Claude's format: /home/user/foo -> -home-user-foo
        let proj_str = proj.to_string_lossy().replace('/', "-");
        let proj_dir = claude_dir.join(&proj_str);
        if proj_dir.exists() {
            return Some(proj_dir);
        }
        // Try with leading dash
        let proj_dir = claude_dir.join(format!("-{}", proj_str.trim_start_matches('-')));
        if proj_dir.exists() {
            return Some(proj_dir);
        }
    }

    // Find the most recently modified project directory
    let mut dirs: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&claude_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                if let Ok(meta) = path.metadata() {
                    if let Ok(mtime) = meta.modified() {
                        dirs.push((path, mtime));
                    }
                }
            }
        }
    }

    dirs.sort_by(|a, b| b.1.cmp(&a.1));
    dirs.first().map(|(p, _)| p.clone())
}

/// Resolve a session ID to a full path.
fn resolve_session_path(session_id: &str, project: Option<&Path>) -> Option<PathBuf> {
    // If it's already a path, use it directly
    if session_id.contains('/') || session_id.ends_with(".jsonl") {
        let path = PathBuf::from(session_id);
        if path.exists() {
            return Some(path);
        }
    }

    // Otherwise, look in the sessions directory
    let sessions_dir = get_sessions_dir(project)?;
    let path = sessions_dir.join(format!("{}.jsonl", session_id));
    if path.exists() {
        Some(path)
    } else {
        // Try fuzzy match
        if let Ok(entries) = std::fs::read_dir(&sessions_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with(session_id) && name_str.ends_with(".jsonl") {
                    return Some(entry.path());
                }
            }
        }
        None
    }
}

/// Format age in human-readable form.
fn format_age(seconds: u64) -> String {
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
