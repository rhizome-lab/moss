//! Sessions command - analyze Claude Code and other agent session logs.

use crate::sessions::analyze_session;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
                let age = mtime.elapsed().map(|d| d.as_secs()).unwrap_or(0);
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
    let paths = resolve_session_paths(session_id, project);

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

/// Resolve a session ID pattern to matching paths.
/// Supports:
/// - Exact ID: "3585080f-a55a-4a39-9666-02d970c3e144"
/// - Prefix: "3585" or "3585*"
/// - Glob: "agent-*"
/// - Full path: "/path/to/session.jsonl"
fn resolve_session_paths(session_id: &str, project: Option<&Path>) -> Vec<PathBuf> {
    // If it's already a path, use it directly
    if session_id.contains('/') || session_id.ends_with(".jsonl") {
        let path = PathBuf::from(session_id);
        if path.exists() {
            return vec![path];
        }
        return vec![];
    }

    let sessions_dir = match get_sessions_dir(project) {
        Some(d) => d,
        None => return vec![],
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

// ============================================================================
// Web server for session viewing
// ============================================================================

use axum::{
    extract::{Path as AxumPath, State},
    http::StatusCode,
    response::Html,
    routing::get,
    Router,
};

struct SessionsState {
    project: Option<PathBuf>,
}

/// Start the sessions web server.
pub async fn cmd_sessions_serve(project: Option<&Path>, port: u16) -> i32 {
    let state = Arc::new(SessionsState {
        project: project.map(|p| p.to_path_buf()),
    });

    let app = Router::new()
        .route("/", get(sessions_index))
        .route("/session/{id}", get(session_detail))
        .route("/session/{id}/raw", get(session_raw))
        .with_state(state);

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    eprintln!("Sessions viewer at http://{}", addr);

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Failed to bind to port {}: {}", port, e);
            return 1;
        }
    };

    if let Err(e) = axum::serve(listener, app).await {
        eprintln!("Server error: {}", e);
        return 1;
    }

    0
}

/// Index page: list all sessions.
async fn sessions_index(State(state): State<Arc<SessionsState>>) -> Html<String> {
    let sessions_dir = get_sessions_dir(state.project.as_deref());

    let mut sessions: Vec<(PathBuf, std::time::SystemTime, Option<String>)> = Vec::new();

    if let Some(dir) = &sessions_dir {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                    if let Ok(meta) = path.metadata() {
                        if let Ok(mtime) = meta.modified() {
                            // Detect format
                            let format = crate::sessions::FormatRegistry::new()
                                .detect(&path)
                                .map(|f| f.name().to_string());
                            sessions.push((path, mtime, format));
                        }
                    }
                }
            }
        }
    }

    sessions.sort_by(|a, b| b.1.cmp(&a.1));
    sessions.truncate(100);

    let mut html = String::from(HTML_HEADER);
    html.push_str("<h1>Session Logs</h1>\n");

    if sessions.is_empty() {
        html.push_str("<p>No sessions found.</p>\n");
    } else {
        html.push_str("<table>\n<thead><tr><th>Session</th><th>Format</th><th>Age</th><th>Actions</th></tr></thead>\n<tbody>\n");
        for (path, mtime, format) in &sessions {
            let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let age = format_age(mtime.elapsed().map(|d| d.as_secs()).unwrap_or(0));
            let format_str = format.as_deref().unwrap_or("unknown");
            let short_id = if id.len() > 20 { &id[..20] } else { id };
            html.push_str(&format!(
                "<tr><td title=\"{}\">{}</td><td>{}</td><td>{}</td><td><a href=\"/session/{}\">view</a> | <a href=\"/session/{}/raw\">raw</a></td></tr>\n",
                id, short_id, format_str, age, id, id
            ));
        }
        html.push_str("</tbody></table>\n");
    }

    html.push_str(HTML_FOOTER);
    Html(html)
}

/// Session detail page: show analysis.
async fn session_detail(
    State(state): State<Arc<SessionsState>>,
    AxumPath(id): AxumPath<String>,
) -> Result<Html<String>, StatusCode> {
    let paths = resolve_session_paths(&id, state.project.as_deref());
    let path = paths.first().ok_or(StatusCode::NOT_FOUND)?;

    let analysis = analyze_session(path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut html = String::from(HTML_HEADER);
    html.push_str(&format!("<h1>Session: {}</h1>\n", id));
    html.push_str("<p><a href=\"/\">← Back to list</a></p>\n");

    // Render analysis as HTML
    html.push_str("<div class=\"analysis\">\n");
    html.push_str(&render_analysis_html(&analysis));
    html.push_str("</div>\n");

    html.push_str(HTML_FOOTER);
    Ok(Html(html))
}

/// Raw session dump.
async fn session_raw(
    State(state): State<Arc<SessionsState>>,
    AxumPath(id): AxumPath<String>,
) -> Result<String, StatusCode> {
    let paths = resolve_session_paths(&id, state.project.as_deref());
    let path = paths.first().ok_or(StatusCode::NOT_FOUND)?;

    std::fs::read_to_string(path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// Render SessionAnalysis as HTML.
fn render_analysis_html(a: &crate::sessions::SessionAnalysis) -> String {
    let mut html = String::new();

    // Summary
    let total_calls: usize = a.tool_stats.values().map(|s| s.calls).sum();
    let total_errors: usize = a.tool_stats.values().map(|s| s.errors).sum();
    let success_rate = if total_calls > 0 {
        ((total_calls - total_errors) as f64 / total_calls as f64) * 100.0
    } else {
        100.0
    };

    html.push_str("<h2>Summary</h2>\n<ul>\n");
    html.push_str(&format!("<li><strong>Format:</strong> {}</li>\n", a.format));
    html.push_str(&format!(
        "<li><strong>Tool calls:</strong> {}</li>\n",
        total_calls
    ));
    html.push_str(&format!(
        "<li><strong>Success rate:</strong> {:.1}%</li>\n",
        success_rate
    ));
    html.push_str(&format!(
        "<li><strong>Total turns:</strong> {}</li>\n",
        a.total_turns
    ));
    html.push_str("</ul>\n");

    // Token usage
    if a.token_stats.api_calls > 0 {
        html.push_str("<h2>Token Usage</h2>\n<ul>\n");
        html.push_str(&format!(
            "<li><strong>API calls:</strong> {}</li>\n",
            a.token_stats.api_calls
        ));
        if a.token_stats.api_calls > 0 {
            let avg_context = (a.token_stats.total_input + a.token_stats.cache_read)
                / a.token_stats.api_calls as u64;
            html.push_str(&format!(
                "<li><strong>Avg context:</strong> {} tokens</li>\n",
                avg_context
            ));
        }
        if a.token_stats.max_context > 0 {
            html.push_str(&format!(
                "<li><strong>Context range:</strong> {} - {}</li>\n",
                a.token_stats.min_context, a.token_stats.max_context
            ));
        }
        if a.token_stats.cache_read > 0 {
            html.push_str(&format!(
                "<li><strong>Cache read:</strong> {} tokens</li>\n",
                a.token_stats.cache_read
            ));
        }
        html.push_str("</ul>\n");
    }

    // Tool usage table
    if !a.tool_stats.is_empty() {
        let mut tools: Vec<_> = a.tool_stats.values().collect();
        tools.sort_by(|a, b| b.calls.cmp(&a.calls));

        html.push_str("<h2>Tool Usage</h2>\n<table>\n");
        html.push_str("<thead><tr><th>Tool</th><th>Calls</th><th>Errors</th><th>Success</th></tr></thead>\n<tbody>\n");
        for tool in tools {
            let rate = if tool.calls > 0 {
                ((tool.calls - tool.errors) as f64 / tool.calls as f64) * 100.0
            } else {
                100.0
            };
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{:.0}%</td></tr>\n",
                tool.name, tool.calls, tool.errors, rate
            ));
        }
        html.push_str("</tbody></table>\n");
    }

    // Error patterns
    if !a.error_patterns.is_empty() {
        html.push_str("<h2>Error Patterns</h2>\n");
        for pattern in &a.error_patterns {
            html.push_str(&format!(
                "<h3>{} ({})</h3>\n",
                pattern.category, pattern.count
            ));
            html.push_str("<ul>\n");
            for example in &pattern.examples {
                html.push_str(&format!("<li><code>{}</code></li>\n", html_escape(example)));
            }
            html.push_str("</ul>\n");
        }
    }

    html
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

const HTML_HEADER: &str = r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Moss Sessions</title>
<style>
body { font-family: system-ui, -apple-system, sans-serif; max-width: 1200px; margin: 0 auto; padding: 1rem; background: #1a1a2e; color: #eee; }
h1, h2, h3 { color: #fff; }
a { color: #6eb5ff; }
table { border-collapse: collapse; width: 100%; margin: 1rem 0; }
th, td { border: 1px solid #333; padding: 0.5rem; text-align: left; }
th { background: #16213e; }
tr:nth-child(even) { background: #1a1a2e; }
tr:nth-child(odd) { background: #0f0f23; }
code { background: #0f0f23; padding: 0.2rem 0.4rem; border-radius: 3px; font-size: 0.9em; }
.analysis { margin-top: 1rem; }
ul { list-style: none; padding-left: 0; }
li { margin: 0.5rem 0; }
</style>
</head>
<body>
"#;

const HTML_FOOTER: &str = "</body></html>\n";
