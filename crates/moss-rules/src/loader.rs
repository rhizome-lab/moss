//! Rule loading from multiple sources.
//!
//! Rules are loaded in this order (later overrides earlier by `id`):
//! 1. Embedded builtins (compiled into moss)
//! 2. User global rules (`~/.config/moss/rules/*.scm`)
//! 3. Project rules (`.moss/rules/*.scm`)

use crate::builtin::BUILTIN_RULES;
use crate::{Rule, Severity};
use glob::Pattern;
use moss_derive::Merge;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Configuration for syntax rules analysis.
/// Maps rule ID to per-rule configuration.
/// e.g., { "rust/unnecessary-let" = { severity = "warning" } }
#[derive(Debug, Clone, Deserialize, Default, Merge)]
#[serde(transparent)]
pub struct RulesConfig(pub HashMap<String, RuleOverride>);

/// Per-rule configuration override.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct RuleOverride {
    /// Override the rule's severity.
    pub severity: Option<String>,
    /// Enable or disable the rule.
    pub enabled: Option<bool>,
    /// Additional file patterns to allow (skip) for this rule.
    #[serde(default)]
    pub allow: Vec<String>,
}

/// Load all rules from all sources, merged by ID.
/// Order: builtins → ~/.config/moss/rules/ → .moss/rules/
/// Then applies config overrides (severity, disable).
pub fn load_all_rules(project_root: &Path, config: &RulesConfig) -> Vec<Rule> {
    let mut rules_by_id: HashMap<String, Rule> = HashMap::new();

    // 1. Load embedded builtins
    for builtin in BUILTIN_RULES {
        if let Some(rule) = parse_rule_content(builtin.content, builtin.id, true) {
            rules_by_id.insert(rule.id.clone(), rule);
        }
    }

    // 2. Load user global rules (~/.config/moss/rules/)
    if let Some(config_dir) = dirs::config_dir() {
        let user_rules_dir = config_dir.join("moss").join("rules");
        for rule in load_rules_from_dir(&user_rules_dir) {
            rules_by_id.insert(rule.id.clone(), rule);
        }
    }

    // 3. Load project rules (.moss/rules/)
    let project_rules_dir = project_root.join(".moss").join("rules");
    for rule in load_rules_from_dir(&project_rules_dir) {
        rules_by_id.insert(rule.id.clone(), rule);
    }

    // 4. Apply config overrides
    for (rule_id, override_cfg) in &config.0 {
        if let Some(rule) = rules_by_id.get_mut(rule_id) {
            if let Some(ref severity_str) = override_cfg.severity {
                if let Ok(severity) = severity_str.parse() {
                    rule.severity = severity;
                }
            }
            if let Some(enabled) = override_cfg.enabled {
                rule.enabled = enabled;
            }
            // Merge additional allow patterns from config
            for pattern_str in &override_cfg.allow {
                if let Ok(pattern) = Pattern::new(pattern_str) {
                    rule.allow.push(pattern);
                }
            }
        }
    }

    // Filter out disabled rules
    rules_by_id.into_values().filter(|r| r.enabled).collect()
}

/// Load rules from a directory.
fn load_rules_from_dir(rules_dir: &Path) -> Vec<Rule> {
    let mut rules = Vec::new();

    if !rules_dir.exists() {
        return rules;
    }

    let entries = match std::fs::read_dir(rules_dir) {
        Ok(e) => e,
        Err(_) => return rules,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e == "scm").unwrap_or(false) {
            if let Some(rule) = parse_rule_file(&path) {
                rules.push(rule);
            }
        }
    }

    rules
}

/// Parse a rule file with TOML frontmatter.
fn parse_rule_file(path: &Path) -> Option<Rule> {
    let content = std::fs::read_to_string(path).ok()?;
    let default_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    let mut rule = parse_rule_content(&content, default_id, false)?;
    rule.source_path = path.to_path_buf();
    Some(rule)
}

/// Parse rule content string with TOML frontmatter.
pub fn parse_rule_content(content: &str, default_id: &str, is_builtin: bool) -> Option<Rule> {
    let lines: Vec<&str> = content.lines().collect();

    let mut in_frontmatter = false;
    let mut frontmatter_lines = Vec::new();
    let mut query_lines = Vec::new();

    for line in &lines {
        let trimmed = line.trim();
        if trimmed == "# ---" {
            if in_frontmatter {
                in_frontmatter = false;
            } else {
                in_frontmatter = true;
            }
            continue;
        }

        if in_frontmatter {
            let fm_line = line.strip_prefix('#').unwrap_or(line).trim_start();
            frontmatter_lines.push(fm_line);
        } else if !in_frontmatter && !frontmatter_lines.is_empty() {
            query_lines.push(*line);
        } else if frontmatter_lines.is_empty() && !trimmed.is_empty() && !trimmed.starts_with('#') {
            query_lines.push(*line);
        }
    }

    let (frontmatter_str, query_str) = if frontmatter_lines.is_empty() {
        (String::new(), content.to_string())
    } else {
        (frontmatter_lines.join("\n"), query_lines.join("\n"))
    };

    let frontmatter: toml::Value = if frontmatter_str.is_empty() {
        toml::Value::Table(toml::map::Map::new())
    } else {
        match toml::from_str(&frontmatter_str) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Warning: invalid frontmatter: {}", e);
                return None;
            }
        }
    };

    let id = frontmatter
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| default_id.to_string());

    let severity = frontmatter
        .get("severity")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(Severity::Warning);

    let message = frontmatter
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("Rule violation")
        .to_string();

    let allow: Vec<Pattern> = frontmatter
        .get("allow")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .filter_map(|s| Pattern::new(s).ok())
                .collect()
        })
        .unwrap_or_default();

    let languages: Vec<String> = frontmatter
        .get("languages")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    let enabled = frontmatter
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let requires: HashMap<String, String> = frontmatter
        .get("requires")
        .and_then(|v| v.as_table())
        .map(|tbl| {
            tbl.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    let fix = frontmatter
        .get("fix")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Some(Rule {
        id,
        query_str: query_str.trim().to_string(),
        severity,
        message,
        allow,
        source_path: PathBuf::new(),
        languages,
        enabled,
        builtin: is_builtin,
        requires,
        fix,
    })
}
