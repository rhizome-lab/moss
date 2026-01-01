//! Find stale documentation where covered code has changed

use std::path::Path;

/// A doc file with stale code coverage
#[derive(Debug)]
struct StaleDoc {
    doc_path: String,
    doc_modified: u64,
    stale_covers: Vec<StaleCover>,
}

/// A stale coverage declaration
#[derive(Debug)]
struct StaleCover {
    pattern: String,
    code_modified: u64,
    matching_files: Vec<String>,
}

/// Find docs with stale code coverage
pub fn cmd_stale_docs(root: &Path, json: bool) -> i32 {
    use regex::Regex;

    // Find markdown files with <!-- covers: ... --> declarations
    let covers_re = Regex::new(r"<!--\s*covers:\s*(.+?)\s*-->").unwrap();

    let md_files: Vec<_> = walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().and_then(|s| s.to_str()) == Some("md")
                && !e
                    .path()
                    .components()
                    .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
        })
        .map(|e| e.path().to_path_buf())
        .collect();

    if md_files.is_empty() {
        if json {
            println!("{{\"stale_docs\": [], \"files_checked\": 0}}");
        } else {
            println!("No markdown files found.");
        }
        return 0;
    }

    let mut stale_docs: Vec<StaleDoc> = Vec::new();
    let mut files_with_covers = 0;

    for md_file in &md_files {
        let content = match std::fs::read_to_string(md_file) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Find all covers declarations
        let covers: Vec<String> = covers_re
            .captures_iter(&content)
            .map(|cap| cap[1].to_string())
            .collect();

        if covers.is_empty() {
            continue;
        }

        files_with_covers += 1;

        let rel_path = md_file
            .strip_prefix(root)
            .unwrap_or(md_file)
            .display()
            .to_string();

        // Get doc modification time
        let doc_modified = std::fs::metadata(md_file)
            .and_then(|m| m.modified())
            .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs())
            .unwrap_or(0);

        let mut stale_covers: Vec<StaleCover> = Vec::new();

        for cover_pattern in covers {
            // Parse comma-separated patterns
            for pattern in cover_pattern.split(',').map(|s| s.trim()) {
                if pattern.is_empty() {
                    continue;
                }

                // Find matching files using glob
                let matching = find_covered_files(root, pattern);

                if matching.is_empty() {
                    continue;
                }

                // Check if any matching file was modified after the doc
                let code_modified = matching
                    .iter()
                    .filter_map(|f| {
                        std::fs::metadata(root.join(f))
                            .and_then(|m| m.modified())
                            .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs())
                            .ok()
                    })
                    .max()
                    .unwrap_or(0);

                if code_modified > doc_modified {
                    stale_covers.push(StaleCover {
                        pattern: pattern.to_string(),
                        code_modified,
                        matching_files: matching,
                    });
                }
            }
        }

        if !stale_covers.is_empty() {
            stale_docs.push(StaleDoc {
                doc_path: rel_path,
                doc_modified,
                stale_covers,
            });
        }
    }

    if json {
        let output = serde_json::json!({
            "stale_docs": stale_docs.iter().map(|d| {
                serde_json::json!({
                    "doc": d.doc_path,
                    "doc_modified": d.doc_modified,
                    "stale_covers": d.stale_covers.iter().map(|c| {
                        serde_json::json!({
                            "pattern": c.pattern,
                            "code_modified": c.code_modified,
                            "files": c.matching_files,
                        })
                    }).collect::<Vec<_>>(),
                })
            }).collect::<Vec<_>>(),
            "files_checked": md_files.len(),
            "files_with_covers": files_with_covers,
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!("Stale Documentation Check");
        println!();
        println!("Files checked: {}", md_files.len());
        println!("Files with covers: {}", files_with_covers);
        println!();

        if stale_docs.is_empty() {
            println!("No stale docs found. All covered code is older than docs.");
        } else {
            println!("Stale docs ({}):", stale_docs.len());
            println!();
            for doc in &stale_docs {
                println!("  {}", doc.doc_path);
                for cover in &doc.stale_covers {
                    let days_stale = (cover.code_modified - doc.doc_modified) / 86400;
                    println!(
                        "    {} ({} files, ~{} days stale)",
                        cover.pattern,
                        cover.matching_files.len(),
                        days_stale
                    );
                }
            }
        }
    }

    if stale_docs.is_empty() { 0 } else { 1 }
}

/// Find files matching a cover pattern (glob or path prefix)
fn find_covered_files(root: &Path, pattern: &str) -> Vec<String> {
    // Check if it's a glob pattern
    if pattern.contains('*') {
        // Use glob matching
        let full_pattern = root.join(pattern);
        glob::glob(full_pattern.to_str().unwrap_or(""))
            .ok()
            .map(|paths| {
                paths
                    .filter_map(|p| p.ok())
                    .filter(|p| p.is_file())
                    .filter_map(|p| p.strip_prefix(root).ok().map(|r| r.display().to_string()))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        // Treat as exact path or prefix
        let target = root.join(pattern);
        if target.is_file() {
            vec![pattern.to_string()]
        } else if target.is_dir() {
            // Find all files in directory
            walkdir::WalkDir::new(&target)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
                .filter_map(|e| {
                    e.path()
                        .strip_prefix(root)
                        .ok()
                        .map(|r| r.display().to_string())
                })
                .collect()
        } else {
            vec![]
        }
    }
}
