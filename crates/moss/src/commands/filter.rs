//! Filter command for managing filter aliases.

use crate::config::MossConfig;
use crate::filter::{AliasStatus, list_aliases};
use clap::Subcommand;
use std::path::Path;

#[derive(Subcommand)]
pub enum FilterAction {
    /// List available filter aliases
    Aliases,
}

/// Handle filter subcommands.
pub fn cmd_filter(action: FilterAction, root: Option<&Path>, json: bool) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    match action {
        FilterAction::Aliases => cmd_filter_aliases(&root, json),
    }
}

/// List available filter aliases.
fn cmd_filter_aliases(root: &Path, json: bool) -> i32 {
    let config = MossConfig::load(root);

    // Detect languages in the project
    let languages = detect_project_languages(root);
    let lang_refs: Vec<&str> = languages.iter().map(|s| s.as_str()).collect();

    let aliases = list_aliases(&config.aliases, &lang_refs);

    if json {
        let output: Vec<_> = aliases
            .iter()
            .map(|a| {
                serde_json::json!({
                    "name": a.name,
                    "patterns": a.patterns,
                    "status": match a.status {
                        AliasStatus::Builtin => "builtin",
                        AliasStatus::Custom => "custom",
                        AliasStatus::Disabled => "disabled",
                        AliasStatus::Overridden => "overridden",
                    }
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!("Aliases:");
        for alias in &aliases {
            let status_suffix = match alias.status {
                AliasStatus::Builtin => "",
                AliasStatus::Custom => "  (custom)",
                AliasStatus::Disabled => "  (disabled)",
                AliasStatus::Overridden => "  (overridden)",
            };

            if alias.patterns.is_empty() {
                println!("  @{:<12} (disabled){}", alias.name, status_suffix);
            } else {
                // Show first few patterns
                let patterns_str = if alias.patterns.len() > 3 {
                    format!(
                        "{}, ... (+{})",
                        alias.patterns[..3].join(", "),
                        alias.patterns.len() - 3
                    )
                } else {
                    alias.patterns.join(", ")
                };
                println!("  @{:<12} {}{}", alias.name, patterns_str, status_suffix);
            }
        }

        if !languages.is_empty() {
            println!("\nDetected languages: {}", languages.join(", "));
        }
    }

    0
}

/// Detect programming languages in the project.
pub fn detect_project_languages(root: &Path) -> Vec<String> {
    use std::collections::HashSet;

    let mut languages = HashSet::new();

    // Walk the project directory (limited depth for performance)
    let walker = ignore::WalkBuilder::new(root)
        .max_depth(Some(5))
        .hidden(false) // Include hidden directories
        .git_ignore(true)
        .build();

    for entry in walker.flatten() {
        let path = entry.path();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            match ext {
                "go" => {
                    languages.insert("go".to_string());
                }
                "py" | "pyi" => {
                    languages.insert("python".to_string());
                }
                "rs" => {
                    languages.insert("rust".to_string());
                }
                "js" | "mjs" | "cjs" => {
                    languages.insert("javascript".to_string());
                }
                "ts" | "mts" | "cts" => {
                    languages.insert("typescript".to_string());
                }
                "java" => {
                    languages.insert("java".to_string());
                }
                "rb" => {
                    languages.insert("ruby".to_string());
                }
                "c" | "h" => {
                    languages.insert("c".to_string());
                }
                "cpp" | "cc" | "cxx" | "hpp" => {
                    languages.insert("cpp".to_string());
                }
                _ => {}
            }
        }
    }

    let mut result: Vec<_> = languages.into_iter().collect();
    result.sort();
    result
}
