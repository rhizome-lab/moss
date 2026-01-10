//! Initialize moss in a project directory.

use clap::Args;
use std::fs;
use std::path::Path;

#[derive(Args, Debug)]
pub struct InitArgs {
    /// Index the codebase after initialization
    #[arg(long)]
    pub index: bool,
}

/// Run init command.
pub fn run(args: InitArgs) -> i32 {
    let root = std::env::current_dir().unwrap();
    cmd_init(&root, args.index)
}

/// Common TODO file names to detect
const TODO_CANDIDATES: &[&str] = &[
    "TODO.md",
    "TASKS.md",
    "TODO.txt",
    "TASKS.txt",
    "TODO",
    "TASKS",
];

fn cmd_init(root: &Path, do_index: bool) -> i32 {
    let mut changes = Vec::new();

    // 1. Create .moss directory if needed
    let moss_dir = root.join(".moss");
    if !moss_dir.exists() {
        if let Err(e) = fs::create_dir_all(&moss_dir) {
            eprintln!("Failed to create .moss directory: {}", e);
            return 1;
        }
        changes.push("Created .moss/".to_string());
    }

    // 2. Detect TODO files for sigil config
    let todo_files = detect_todo_files(root);

    // 3. Create or update config.toml
    let config_path = moss_dir.join("config.toml");
    if !config_path.exists() {
        let aliases_section = if todo_files.is_empty() {
            String::new()
        } else {
            let files_str = todo_files
                .iter()
                .map(|f| format!("\"{}\"", f))
                .collect::<Vec<_>>()
                .join(", ");
            format!("\n[aliases]\ntodo = [{}]\n", files_str)
        };

        let default_config = format!(
            r#"# Moss configuration
# See: https://github.com/pterror/moss

[daemon]
# enabled = true
# auto_start = true

[analyze]
# clones = true

# [analyze.weights]
# health = 1.0
# complexity = 0.5
# security = 2.0
# clones = 0.3
{}"#,
            aliases_section
        );
        if let Err(e) = fs::write(&config_path, default_config) {
            eprintln!("Failed to create config.toml: {}", e);
            return 1;
        }
        changes.push("Created .moss/config.toml".to_string());
        for f in &todo_files {
            changes.push(format!("Detected TODO file: {}", f));
        }
    }

    // 3. Update .gitignore if needed
    let gitignore_path = root.join(".gitignore");
    let gitignore_changes = update_gitignore(&gitignore_path);
    changes.extend(gitignore_changes);

    // 4. Report changes
    if changes.is_empty() {
        println!("Already initialized.");
    } else {
        println!("Initialized moss:");
        for change in &changes {
            println!("  {}", change);
        }
    }

    // 5. Optionally index
    if do_index {
        println!("\nIndexing codebase...");
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut idx = match rt.block_on(crate::index::FileIndex::open(root)) {
            Ok(idx) => idx,
            Err(e) => {
                eprintln!("Failed to open index: {}", e);
                return 1;
            }
        };
        match rt.block_on(idx.refresh()) {
            Ok(count) => println!("Indexed {} files.", count),
            Err(e) => {
                eprintln!("Failed to index: {}", e);
                return 1;
            }
        }
    }

    0
}

/// Detect TODO files in the project root.
fn detect_todo_files(root: &Path) -> Vec<String> {
    TODO_CANDIDATES
        .iter()
        .filter(|name| root.join(name).exists())
        .map(|s| s.to_string())
        .collect()
}

/// Entries we want in .gitignore
/// - .moss/* ignores root .moss/ contents (patterns with / only match at root)
/// - !.moss/... un-ignores specific files (works because /* ignores contents, not the dir)
/// NOTE: We omit **/.moss/ because it would block un-ignore patterns entirely.
const GITIGNORE_ENTRIES: &[&str] = &[
    ".moss/*",
    "!.moss/config.toml",
    "!.moss/duplicate-functions-allow",
    "!.moss/duplicate-types-allow",
    "!.moss/hotspots-allow",
    "!.moss/large-files-allow",
    "!.moss/memory/",
];

/// Update .gitignore with moss entries. Returns list of changes made.
fn update_gitignore(path: &Path) -> Vec<String> {
    let mut changes = Vec::new();

    // Read existing content
    let content = fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = content.lines().collect();

    // Check which entries are missing and find best insertion point
    let mut to_add = Vec::new();
    let mut insert_after: Option<usize> = None; // Line index to insert after

    for entry in GITIGNORE_ENTRIES {
        match find_entry(&lines, entry) {
            EntryStatus::Missing => {
                to_add.push(*entry);
            }
            EntryStatus::CommentedOut(line_num) => {
                eprintln!(
                    "Note: '{}' is commented out in .gitignore (line {}), skipping",
                    entry,
                    line_num + 1
                );
            }
            EntryStatus::Present(line_num) => {
                // Track where existing moss entries are for best insertion point
                insert_after = Some(insert_after.map_or(line_num, |prev| prev.max(line_num)));
            }
        }
    }

    if to_add.is_empty() {
        return changes;
    }

    // Build new content
    let mut new_lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();

    if let Some(idx) = insert_after {
        // Insert near existing moss entries
        let insert_pos = idx + 1;
        for (i, entry) in to_add.iter().enumerate() {
            new_lines.insert(insert_pos + i, entry.to_string());
            changes.push(format!("Added '{}' to .gitignore", entry));
        }
    } else {
        // Append at end with header
        if !new_lines.is_empty() && !new_lines.last().map_or(true, |l| l.is_empty()) {
            new_lines.push(String::new());
        }
        new_lines.push("# Moss".to_string());
        for entry in &to_add {
            new_lines.push(entry.to_string());
            changes.push(format!("Added '{}' to .gitignore", entry));
        }
    }

    let new_content = new_lines.join("\n") + "\n";
    if let Err(e) = fs::write(path, new_content) {
        eprintln!("Failed to update .gitignore: {}", e);
        return Vec::new();
    }

    changes
}

enum EntryStatus {
    Missing,
    Present(usize),
    CommentedOut(usize),
}

/// Check if an entry exists in gitignore lines.
fn find_entry(lines: &[&str], pattern: &str) -> EntryStatus {
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Exact match
        if trimmed == pattern {
            return EntryStatus::Present(i);
        }

        // Check if commented version exists
        if trimmed.starts_with('#') {
            let uncommented = trimmed.trim_start_matches('#').trim();
            if uncommented == pattern {
                return EntryStatus::CommentedOut(i);
            }
        }
    }
    EntryStatus::Missing
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_init_creates_moss_dir() {
        let tmp = tempdir().unwrap();
        let result = cmd_init(tmp.path(), false);
        assert_eq!(result, 0);
        assert!(tmp.path().join(".moss").exists());
        assert!(tmp.path().join(".moss/config.toml").exists());
    }

    #[test]
    fn test_init_idempotent() {
        let tmp = tempdir().unwrap();
        let result1 = cmd_init(tmp.path(), false);
        let result2 = cmd_init(tmp.path(), false);
        assert_eq!(result1, 0);
        assert_eq!(result2, 0);
    }

    #[test]
    fn test_init_updates_gitignore() {
        let tmp = tempdir().unwrap();
        fs::write(tmp.path().join(".gitignore"), "node_modules\n").unwrap();

        cmd_init(tmp.path(), false);

        let content = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(content.contains(".moss/*"));
        assert!(content.contains("!.moss/config.toml"));
    }

    #[test]
    fn test_init_skips_commented_entries() {
        let tmp = tempdir().unwrap();
        fs::write(tmp.path().join(".gitignore"), "# .moss\n").unwrap();

        cmd_init(tmp.path(), false);

        let content = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        let lines: Vec<&str> = content.lines().collect();

        // .moss should remain commented (not added as active entry)
        assert!(lines.iter().any(|l| l.trim() == "# .moss"));
        assert!(!lines.iter().any(|l| l.trim() == ".moss"));

        // But negation entries should still be added
        assert!(lines.iter().any(|l| l.trim() == "!.moss/config.toml"));
        assert!(
            lines
                .iter()
                .any(|l| l.trim() == "!.moss/duplicate-functions-allow")
        );
        assert!(
            lines
                .iter()
                .any(|l| l.trim() == "!.moss/duplicate-types-allow")
        );
    }

    #[test]
    fn test_init_inserts_near_existing() {
        let tmp = tempdir().unwrap();
        // Existing .gitignore already has .moss/*
        fs::write(
            tmp.path().join(".gitignore"),
            "node_modules\n.moss/*\nother_stuff\n",
        )
        .unwrap();

        cmd_init(tmp.path(), false);

        let content = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        let lines: Vec<&str> = content.lines().collect();

        // New entries should be inserted after .moss/*, before other_stuff
        let moss_idx = lines.iter().position(|l| *l == ".moss/*").unwrap();
        let other_idx = lines.iter().position(|l| *l == "other_stuff").unwrap();
        let config_idx = lines
            .iter()
            .position(|l| *l == "!.moss/config.toml")
            .unwrap();

        // All new entries should be between .moss/* and other_stuff
        assert!(config_idx > moss_idx, "config should be after .moss/*");
        assert!(
            config_idx < other_idx,
            "config should be before other_stuff"
        );
    }

    #[test]
    fn test_init_detects_todo_files() {
        let tmp = tempdir().unwrap();
        fs::write(tmp.path().join("TODO.md"), "# TODO\n").unwrap();
        fs::write(tmp.path().join("TASKS.md"), "# Tasks\n").unwrap();

        cmd_init(tmp.path(), false);

        let config = fs::read_to_string(tmp.path().join(".moss/config.toml")).unwrap();
        assert!(config.contains("[aliases]"));
        assert!(config.contains("TODO.md"));
        assert!(config.contains("TASKS.md"));
    }

    #[test]
    fn test_init_no_todo_files() {
        let tmp = tempdir().unwrap();

        cmd_init(tmp.path(), false);

        let config = fs::read_to_string(tmp.path().join(".moss/config.toml")).unwrap();
        // Should not have aliases section if no TODO files found
        assert!(!config.contains("[aliases]"));
    }
}
