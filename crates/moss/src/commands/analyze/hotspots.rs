//! Git history hotspot analysis

use super::is_source_file;
use crate::index;
use glob::Pattern;
use std::path::Path;

/// Hotspot data for a file
#[derive(Debug)]
struct FileHotspot {
    path: String,
    commits: usize,
    lines_added: usize,
    lines_deleted: usize,
    score: f64,
}

/// Analyze git history hotspots
pub fn cmd_hotspots(root: &Path, exclude_patterns: &[String], json: bool) -> i32 {
    // Compile exclusion patterns
    let excludes: Vec<Pattern> = exclude_patterns
        .iter()
        .filter_map(|p| Pattern::new(p).ok())
        .collect();
    // Check if git repo
    let git_dir = root.join(".git");
    if !git_dir.exists() {
        eprintln!("Not a git repository");
        return 1;
    }

    // Get file commit counts and churn from git log
    let output = match std::process::Command::new("git")
        .args(["log", "--format=", "--numstat"])
        .current_dir(root)
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Failed to run git log: {}", e);
            return 1;
        }
    };

    if !output.status.success() {
        eprintln!("git log failed");
        return 1;
    }

    // Parse numstat output: added<TAB>deleted<TAB>path
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut file_stats: std::collections::HashMap<String, (usize, usize, usize)> =
        std::collections::HashMap::new();

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() == 3 {
            let added = parts[0].parse::<usize>().unwrap_or(0);
            let deleted = parts[1].parse::<usize>().unwrap_or(0);
            let path = parts[2].to_string();

            // Skip binary files (shown as -)
            if parts[0] == "-" || parts[1] == "-" {
                continue;
            }

            let entry = file_stats.entry(path).or_insert((0, 0, 0));
            entry.0 += 1; // commits
            entry.1 += added;
            entry.2 += deleted;
        }
    }

    // Get complexity from index
    let rt = tokio::runtime::Runtime::new().unwrap();
    let idx = match rt.block_on(index::FileIndex::open_if_enabled(root)) {
        Some(i) => i,
        None => {
            // No index, just use churn data
            let mut hotspots: Vec<FileHotspot> = file_stats
                .into_iter()
                .filter(|(path, _)| {
                    // Filter to source files, skip excluded
                    let p = Path::new(path);
                    p.exists() && is_source_file(p) && !excludes.iter().any(|pat| pat.matches(path))
                })
                .map(|(path, (commits, added, deleted))| {
                    let churn = added + deleted;
                    FileHotspot {
                        path,
                        commits,
                        lines_added: added,
                        lines_deleted: deleted,
                        score: (commits as f64) * (churn as f64).sqrt(),
                    }
                })
                .collect();

            hotspots.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
            hotspots.truncate(20);

            return print_hotspots(&hotspots, json);
        }
    };

    // Build hotspots from churn data (index is available but not used for complexity)
    let _ = idx; // Index available for future on-demand complexity computation
    let mut hotspots: Vec<FileHotspot> = Vec::new();

    for (path, (commits, added, deleted)) in file_stats {
        let p = Path::new(&path);
        if !p.exists() || !is_source_file(p) {
            continue;
        }
        // Skip excluded patterns
        if excludes.iter().any(|pat| pat.matches(&path)) {
            continue;
        }

        let churn = added + deleted;
        // Score: commits * sqrt(churn)
        let score = (commits as f64) * (churn as f64).sqrt();

        hotspots.push(FileHotspot {
            path,
            commits,
            lines_added: added,
            lines_deleted: deleted,
            score,
        });
    }

    hotspots.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    hotspots.truncate(20);

    print_hotspots(&hotspots, json)
}

/// Print hotspots report
fn print_hotspots(hotspots: &[FileHotspot], json: bool) -> i32 {
    if hotspots.is_empty() {
        if json {
            println!("[]");
        } else {
            println!("No hotspots found (no git history or source files)");
        }
        return 0;
    }

    if json {
        let output: Vec<_> = hotspots
            .iter()
            .map(|h| {
                serde_json::json!({
                    "path": h.path,
                    "commits": h.commits,
                    "lines_added": h.lines_added,
                    "lines_deleted": h.lines_deleted,
                    "churn": h.lines_added + h.lines_deleted,
                    "score": (h.score * 10.0).round() / 10.0,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!("Git Hotspots (high churn)");
        println!();
        println!(
            "{:<50} {:>8} {:>8} {:>8}",
            "File", "Commits", "Churn", "Score"
        );
        println!("{}", "-".repeat(80));

        for h in hotspots {
            let churn = h.lines_added + h.lines_deleted;
            let display_path = if h.path.len() > 48 {
                format!("...{}", &h.path[h.path.len() - 45..])
            } else {
                h.path.clone()
            };
            println!(
                "{:<50} {:>8} {:>8} {:>8.0}",
                display_path, h.commits, churn, h.score
            );
        }

        println!();
        println!("Score = commits × √churn");
        println!("High scores indicate bug-prone files that change often.");
    }

    0
}
