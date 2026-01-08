//! Rule management commands - add, list, update rules from URLs.

use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum RulesAction {
    /// Add a rule from a URL
    Add {
        /// URL to download the rule from
        url: String,

        /// Install to global rules (~/.config/moss/rules/) instead of project
        #[arg(long)]
        global: bool,
    },

    /// List installed rules
    List {
        /// Show source URLs for imported rules
        #[arg(long)]
        sources: bool,
    },

    /// Update imported rules from their sources
    Update {
        /// Specific rule ID to update (updates all if omitted)
        rule_id: Option<String>,
    },

    /// Remove an imported rule
    Remove {
        /// Rule ID to remove
        rule_id: String,
    },
}

/// Lock file entry tracking an imported rule
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuleLockEntry {
    source: String,
    sha256: String,
    added: String,
}

/// Lock file format
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct RulesLock {
    rules: HashMap<String, RuleLockEntry>,
}

impl RulesLock {
    fn load(path: &Path) -> Self {
        if !path.exists() {
            return Self::default();
        }
        std::fs::read_to_string(path)
            .ok()
            .and_then(|content| toml::from_str(&content).ok())
            .unwrap_or_default()
    }

    fn save(&self, path: &Path) -> std::io::Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        std::fs::write(path, content)
    }
}

/// Run the rules command
pub fn cmd_rules(action: RulesAction, json: bool) -> i32 {
    match action {
        RulesAction::Add { url, global } => cmd_add(&url, global, json),
        RulesAction::List { sources } => cmd_list(sources, json),
        RulesAction::Update { rule_id } => cmd_update(rule_id.as_deref(), json),
        RulesAction::Remove { rule_id } => cmd_remove(&rule_id, json),
    }
}

fn rules_dir(global: bool) -> Option<PathBuf> {
    if global {
        dirs::config_dir().map(|d| d.join("moss").join("rules"))
    } else {
        Some(PathBuf::from(".moss").join("rules"))
    }
}

fn lock_file_path(global: bool) -> Option<PathBuf> {
    if global {
        dirs::config_dir().map(|d| d.join("moss").join("rules.lock"))
    } else {
        Some(PathBuf::from(".moss").join("rules.lock"))
    }
}

fn cmd_add(url: &str, global: bool, json: bool) -> i32 {
    let Some(rules_dir) = rules_dir(global) else {
        eprintln!("Could not determine rules directory");
        return 1;
    };

    // Create rules directory if needed
    if let Err(e) = std::fs::create_dir_all(&rules_dir) {
        eprintln!("Failed to create rules directory: {}", e);
        return 1;
    }

    // Download the rule
    let content = match download_url(url) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to download rule: {}", e);
            return 1;
        }
    };

    // Extract rule ID from content
    let rule_id = match extract_rule_id(&content) {
        Some(id) => id,
        None => {
            eprintln!("Could not extract rule ID from downloaded content");
            eprintln!("Rule must have TOML frontmatter with 'id' field");
            return 1;
        }
    };

    // Save rule file
    let rule_path = rules_dir.join(format!("{}.scm", rule_id));
    if let Err(e) = std::fs::write(&rule_path, &content) {
        eprintln!("Failed to save rule: {}", e);
        return 1;
    }

    // Update lock file
    let Some(lock_path) = lock_file_path(global) else {
        eprintln!("Could not determine lock file path");
        return 1;
    };

    let mut lock = RulesLock::load(&lock_path);
    lock.rules.insert(
        rule_id.clone(),
        RuleLockEntry {
            source: url.to_string(),
            sha256: sha256_hex(&content),
            added: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        },
    );

    if let Err(e) = lock.save(&lock_path) {
        eprintln!("Warning: Failed to update lock file: {}", e);
    }

    if json {
        println!(
            "{}",
            serde_json::json!({
                "added": rule_id,
                "path": rule_path,
                "source": url
            })
        );
    } else {
        println!("Added rule '{}' from {}", rule_id, url);
        println!("Saved to: {}", rule_path.display());
    }

    0
}

fn cmd_list(sources: bool, json: bool) -> i32 {
    let mut all_rules = Vec::new();

    // Load project lock
    let project_lock = lock_file_path(false)
        .map(|p| RulesLock::load(&p))
        .unwrap_or_default();

    // Load global lock
    let global_lock = lock_file_path(true)
        .map(|p| RulesLock::load(&p))
        .unwrap_or_default();

    // List project rules
    if let Some(dir) = rules_dir(false) {
        if dir.exists() {
            for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                if entry
                    .path()
                    .extension()
                    .map(|e| e == "scm")
                    .unwrap_or(false)
                {
                    let id = entry
                        .path()
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let source = project_lock.rules.get(&id).map(|e| e.source.clone());
                    all_rules.push(("project", id, source));
                }
            }
        }
    }

    // List global rules
    if let Some(dir) = rules_dir(true) {
        if dir.exists() {
            for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                if entry
                    .path()
                    .extension()
                    .map(|e| e == "scm")
                    .unwrap_or(false)
                {
                    let id = entry
                        .path()
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let source = global_lock.rules.get(&id).map(|e| e.source.clone());
                    all_rules.push(("global", id, source));
                }
            }
        }
    }

    if json {
        let rules: Vec<_> = all_rules
            .iter()
            .map(|(scope, id, source)| {
                serde_json::json!({
                    "scope": scope,
                    "id": id,
                    "source": source
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&rules).unwrap());
    } else if all_rules.is_empty() {
        println!("No custom rules installed.");
        println!();
        println!("Add a rule with: moss rules add <url>");
    } else {
        for (scope, id, source) in &all_rules {
            if sources {
                if let Some(src) = source {
                    println!("[{}] {} (from {})", scope, id, src);
                } else {
                    println!("[{}] {} (local)", scope, id);
                }
            } else {
                println!("[{}] {}", scope, id);
            }
        }
        println!();
        println!("{} rule(s) installed", all_rules.len());
    }

    0
}

fn cmd_update(rule_id: Option<&str>, json: bool) -> i32 {
    let mut updated = Vec::new();
    let mut errors = Vec::new();

    // Update project rules
    if let (Some(lock_path), Some(rules_dir)) = (lock_file_path(false), rules_dir(false)) {
        let lock = RulesLock::load(&lock_path);
        for (id, entry) in &lock.rules {
            if rule_id.is_some() && rule_id != Some(id.as_str()) {
                continue;
            }
            match download_url(&entry.source) {
                Ok(content) => {
                    let path = rules_dir.join(format!("{}.scm", id));
                    if let Err(e) = std::fs::write(&path, &content) {
                        errors.push((id.clone(), e.to_string()));
                    } else {
                        updated.push(id.clone());
                    }
                }
                Err(e) => {
                    errors.push((id.clone(), e.to_string()));
                }
            }
        }
    }

    // Update global rules
    if let (Some(lock_path), Some(rules_dir)) = (lock_file_path(true), rules_dir(true)) {
        let lock = RulesLock::load(&lock_path);
        for (id, entry) in &lock.rules {
            if rule_id.is_some() && rule_id != Some(id.as_str()) {
                continue;
            }
            match download_url(&entry.source) {
                Ok(content) => {
                    let path = rules_dir.join(format!("{}.scm", id));
                    if let Err(e) = std::fs::write(&path, &content) {
                        errors.push((id.clone(), e.to_string()));
                    } else {
                        updated.push(id.clone());
                    }
                }
                Err(e) => {
                    errors.push((id.clone(), e.to_string()));
                }
            }
        }
    }

    if json {
        println!(
            "{}",
            serde_json::json!({
                "updated": updated,
                "errors": errors
            })
        );
    } else {
        if updated.is_empty() && errors.is_empty() {
            println!("No imported rules to update.");
        } else {
            for id in &updated {
                println!("Updated: {}", id);
            }
            for (id, err) in &errors {
                eprintln!("Failed to update {}: {}", id, err);
            }
        }
    }

    if errors.is_empty() { 0 } else { 1 }
}

fn cmd_remove(rule_id: &str, json: bool) -> i32 {
    let mut removed = false;

    // Try project first
    if let (Some(lock_path), Some(rules_dir)) = (lock_file_path(false), rules_dir(false)) {
        let mut lock = RulesLock::load(&lock_path);
        if lock.rules.remove(rule_id).is_some() {
            let _ = lock.save(&lock_path);
            let rule_path = rules_dir.join(format!("{}.scm", rule_id));
            let _ = std::fs::remove_file(&rule_path);
            removed = true;
        }
    }

    // Try global if not found in project
    if !removed {
        if let (Some(lock_path), Some(rules_dir)) = (lock_file_path(true), rules_dir(true)) {
            let mut lock = RulesLock::load(&lock_path);
            if lock.rules.remove(rule_id).is_some() {
                let _ = lock.save(&lock_path);
                let rule_path = rules_dir.join(format!("{}.scm", rule_id));
                let _ = std::fs::remove_file(&rule_path);
                removed = true;
            }
        }
    }

    if json {
        println!(
            "{}",
            serde_json::json!({
                "removed": removed,
                "rule_id": rule_id
            })
        );
    } else if removed {
        println!("Removed rule '{}'", rule_id);
    } else {
        eprintln!("Rule '{}' not found in lock file", rule_id);
        return 1;
    }

    0
}

fn download_url(url: &str) -> Result<String, String> {
    // Use ureq for HTTP requests
    let response = ureq::get(url)
        .call()
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if response.status() != 200 {
        return Err(format!(
            "HTTP {}: {}",
            response.status(),
            response.status_text()
        ));
    }

    response
        .into_string()
        .map_err(|e| format!("Failed to read response: {}", e))
}

fn extract_rule_id(content: &str) -> Option<String> {
    // Parse TOML frontmatter to extract rule ID
    // Format: # ---\n# id = "rule-id"\n# ---
    let lines: Vec<&str> = content.lines().collect();

    let mut in_frontmatter = false;
    let mut toml_lines = Vec::new();

    for line in lines {
        let trimmed = line.trim();
        if trimmed == "# ---" {
            if in_frontmatter {
                break; // End of frontmatter
            }
            in_frontmatter = true;
            continue;
        }
        if in_frontmatter {
            if let Some(rest) = trimmed.strip_prefix("# ") {
                toml_lines.push(rest);
            } else if let Some(rest) = trimmed.strip_prefix("#") {
                toml_lines.push(rest);
            }
        }
    }

    if toml_lines.is_empty() {
        return None;
    }

    let toml_content = toml_lines.join("\n");
    let table: toml::Table = toml_content.parse().ok()?;
    table.get("id")?.as_str().map(|s| s.to_string())
}

fn sha256_hex(content: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    // Simple hash for now - could use actual SHA256 later
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
