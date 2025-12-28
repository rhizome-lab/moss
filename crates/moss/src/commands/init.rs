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

    // 2. Create default config.toml if needed
    let config_path = moss_dir.join("config.toml");
    if !config_path.exists() {
        let default_config = r#"# Moss configuration
# See: https://github.com/pterror/moss

[daemon]
# enabled = true
# auto_start = true
"#;
        if let Err(e) = fs::write(&config_path, default_config) {
            eprintln!("Failed to create config.toml: {}", e);
            return 1;
        }
        changes.push("Created .moss/config.toml".to_string());
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
        let mut idx = match crate::index::FileIndex::open(root) {
            Ok(idx) => idx,
            Err(e) => {
                eprintln!("Failed to open index: {}", e);
                return 1;
            }
        };
        match idx.refresh() {
            Ok(count) => println!("Indexed {} files.", count),
            Err(e) => {
                eprintln!("Failed to index: {}", e);
                return 1;
            }
        }
    }

    0
}

/// Entries we want in .gitignore
const GITIGNORE_ENTRIES: &[&str] = &[".moss", "!.moss/config.toml", "!.moss/clone-allow"];

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
        assert!(content.contains(".moss"));
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
        assert!(lines.iter().any(|l| l.trim() == "!.moss/clone-allow"));
    }

    #[test]
    fn test_init_inserts_near_existing() {
        let tmp = tempdir().unwrap();
        fs::write(
            tmp.path().join(".gitignore"),
            "node_modules\n.moss\nother_stuff\n",
        )
        .unwrap();

        cmd_init(tmp.path(), false);

        let content = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        let lines: Vec<&str> = content.lines().collect();

        // !.moss/config.toml should be right after .moss, not at the end
        let moss_idx = lines.iter().position(|l| *l == ".moss").unwrap();
        let config_idx = lines
            .iter()
            .position(|l| *l == "!.moss/config.toml")
            .unwrap();
        assert_eq!(config_idx, moss_idx + 1);
    }
}
