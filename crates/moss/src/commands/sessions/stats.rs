//! Aggregate statistics across sessions.

use super::{analyze::cmd_sessions_analyze_multi, session_matches_grep};
use crate::sessions::{FormatRegistry, LogFormat, SessionFile};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Parse a date string (YYYY-MM-DD) to SystemTime.
pub(crate) fn parse_date(s: &str) -> Option<SystemTime> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let year: i32 = parts[0].parse().ok()?;
    let month: u32 = parts[1].parse().ok()?;
    let day: u32 = parts[2].parse().ok()?;

    // Convert to days since Unix epoch (rough calculation)
    // This is approximate but good enough for filtering
    let days_since_epoch = (year - 1970) as i64 * 365
        + (year - 1970) as i64 / 4 // leap years approx
        + match month {
            1 => 0,
            2 => 31,
            3 => 59,
            4 => 90,
            5 => 120,
            6 => 151,
            7 => 181,
            8 => 212,
            9 => 243,
            10 => 273,
            11 => 304,
            12 => 334,
            _ => return None,
        } as i64
        + day as i64
        - 1;

    let secs = days_since_epoch * 86400;
    if secs < 0 {
        return None;
    }
    Some(SystemTime::UNIX_EPOCH + Duration::from_secs(secs as u64))
}

/// Show aggregate statistics across all sessions.
#[allow(clippy::too_many_arguments)]
pub fn cmd_sessions_stats(
    root: Option<&Path>,
    limit: usize,
    format_name: Option<&str>,
    grep: Option<&str>,
    days: Option<u32>,
    since: Option<&str>,
    until: Option<&str>,
    project_filter: Option<&Path>,
    all_projects: bool,
    json: bool,
    pretty: bool,
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
    let mut sessions: Vec<SessionFile> = if all_projects {
        // List sessions from all projects in ~/.claude/projects/
        list_all_project_sessions(format)
    } else {
        let project = if let Some(p) = project_filter {
            Some(p)
        } else {
            root
        };
        format.list_sessions(project)
    };

    // Calculate date filters
    let now = SystemTime::now();

    let since_time = if let Some(d) = days {
        Some(now - Duration::from_secs(d as u64 * 86400))
    } else if let Some(s) = since {
        match parse_date(s) {
            Some(t) => Some(t),
            None => {
                eprintln!("Invalid date format: {} (use YYYY-MM-DD)", s);
                return 1;
            }
        }
    } else {
        None
    };

    let until_time = if let Some(u) = until {
        match parse_date(u) {
            Some(t) => Some(t + Duration::from_secs(86400)), // Include the entire day
            None => {
                eprintln!("Invalid date format: {} (use YYYY-MM-DD)", u);
                return 1;
            }
        }
    } else {
        None
    };

    // Apply date filters
    if let Some(since) = since_time {
        sessions.retain(|s| s.mtime >= since);
    }
    if let Some(until) = until_time {
        sessions.retain(|s| s.mtime <= until);
    }

    // Apply grep filter if provided
    if let Some(ref re) = grep_re {
        sessions.retain(|s| session_matches_grep(&s.path, re));
    }

    // Sort by time (newest first) and limit
    sessions.sort_by(|a, b| b.mtime.cmp(&a.mtime));
    if limit > 0 {
        sessions.truncate(limit);
    }

    if sessions.is_empty() {
        if json {
            println!("{{}}");
        } else {
            eprintln!("No {} sessions found", format_name.unwrap_or("Claude Code"));
            if days.is_some() || since.is_some() || until.is_some() {
                eprintln!("(with date filter applied)");
            }
        }
        return 0;
    }

    // Show what we're analyzing
    if !json {
        let date_range = if let Some(d) = days {
            format!(" (last {} days)", d)
        } else if since.is_some() || until.is_some() {
            let s = since.unwrap_or("*");
            let u = until.unwrap_or("*");
            format!(" ({} to {})", s, u)
        } else {
            String::new()
        };

        let project_info = if all_projects {
            " across all projects".to_string()
        } else if let Some(p) = project_filter {
            format!(" in {}", p.display())
        } else {
            String::new()
        };

        eprintln!(
            "Analyzing {} sessions{}{}...\n",
            sessions.len(),
            date_range,
            project_info
        );
    }

    // Collect paths and analyze
    let paths: Vec<_> = sessions.iter().map(|s| s.path.clone()).collect();
    cmd_sessions_analyze_multi(&paths, format_name, json, pretty)
}

/// List sessions from all projects in ~/.claude/projects/
pub(crate) fn list_all_project_sessions(format: &dyn LogFormat) -> Vec<SessionFile> {
    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => return Vec::new(),
    };

    let projects_dir = PathBuf::from(home).join(".claude/projects");
    if !projects_dir.exists() {
        return Vec::new();
    }

    let mut all_sessions = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&projects_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let proj_dir = entry.path();
            if !proj_dir.is_dir() {
                continue;
            }

            // List JSONL files in this project directory
            if let Ok(files) = std::fs::read_dir(&proj_dir) {
                for file in files.filter_map(|f| f.ok()) {
                    let path = file.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                        if let Ok(meta) = path.metadata() {
                            if let Ok(mtime) = meta.modified() {
                                // Use format's detect to verify it's the right format
                                if format.detect(&path) > 0.5 {
                                    all_sessions.push(SessionFile { path, mtime });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    all_sessions
}
