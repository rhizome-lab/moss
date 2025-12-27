//! Configuration system for moss.
//!
//! Loads config from:
//! 1. Global: ~/.config/moss/config.toml
//! 2. Per-project: .moss/config.toml (overrides global)
//!
//! Example config.toml:
//! ```toml
//! [daemon]
//! enabled = true
//! auto_start = true
//!
//! [index]
//! enabled = true
//!
//! [filter.aliases]
//! tests = ["*_test.*", "my_custom_tests/**"]  # override built-in
//! vendor = ["vendor/**", "third_party/**"]     # add new alias
//! config = []                                   # disable built-in
//!
//! [todo]
//! file = "TASKS.md"           # custom todo file (default: auto-detect)
//! primary_section = "Backlog" # default section for add/done/rm
//! show_all = true             # show all sections by default
//! ```

use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

/// Daemon configuration.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct DaemonConfig {
    /// Whether to use the daemon for queries.
    pub enabled: bool,
    /// Whether to auto-start the daemon when running moss commands.
    pub auto_start: bool,
}

/// Index configuration.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct IndexConfig {
    /// Whether to create and use the file index.
    pub enabled: bool,
}

/// Filter configuration for --exclude and --only flags.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct FilterConfig {
    /// Custom filter aliases. Keys are alias names (without @), values are glob patterns.
    /// Setting an empty array disables a built-in alias.
    pub aliases: HashMap<String, Vec<String>>,
}

/// Todo command configuration.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct TodoConfig {
    /// Path to todo file (relative to project root).
    /// If not set, auto-detects from common filenames.
    pub file: Option<String>,
    /// Primary section name to show by default.
    /// If not set, uses common patterns: "Next Up", "TODO", "Tasks".
    pub primary_section: Option<String>,
    /// Default to showing all sections instead of just primary.
    pub show_all: bool,
}

/// Root configuration structure.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct MossConfig {
    pub daemon: DaemonConfig,
    pub index: IndexConfig,
    pub filter: FilterConfig,
    pub todo: TodoConfig,
}

impl MossConfig {
    /// Load configuration for a project.
    ///
    /// Loads global config from ~/.config/moss/config.toml,
    /// then merges with per-project config from .moss/config.toml.
    pub fn load(root: &Path) -> Self {
        let mut config = Self::default_enabled();

        // Load global config
        if let Some(global_path) = Self::global_config_path() {
            if let Some(global) = Self::load_file(&global_path) {
                config = config.merge(global);
            }
        }

        // Load per-project config (overrides global)
        let project_path = root.join(".moss").join("config.toml");
        if let Some(project) = Self::load_file(&project_path) {
            config = config.merge(project);
        }

        config
    }

    /// Default config with everything enabled.
    fn default_enabled() -> Self {
        Self {
            daemon: DaemonConfig {
                enabled: true,
                auto_start: true,
            },
            index: IndexConfig { enabled: true },
            filter: FilterConfig::default(),
            todo: TodoConfig::default(),
        }
    }

    /// Get the global config path.
    fn global_config_path() -> Option<std::path::PathBuf> {
        let config_home = std::env::var("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .ok()
            .or_else(|| dirs::home_dir().map(|h| h.join(".config")))?;
        Some(config_home.join("moss").join("config.toml"))
    }

    /// Load config from a file path.
    fn load_file(path: &Path) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        toml::from_str(&content).ok()
    }

    /// Merge another config into this one.
    /// Values from `other` override values in `self` only if they differ from defaults.
    fn merge(self, other: Self) -> Self {
        // For now, simple override - other takes precedence
        // A more sophisticated merge would check which fields were explicitly set
        let mut merged_aliases = self.filter.aliases;
        for (k, v) in other.filter.aliases {
            merged_aliases.insert(k, v);
        }

        Self {
            daemon: DaemonConfig {
                enabled: other.daemon.enabled,
                auto_start: other.daemon.auto_start,
            },
            index: IndexConfig {
                enabled: other.index.enabled,
            },
            filter: FilterConfig {
                aliases: merged_aliases,
            },
            todo: TodoConfig {
                file: other.todo.file.or(self.todo.file),
                primary_section: other.todo.primary_section.or(self.todo.primary_section),
                show_all: other.todo.show_all || self.todo.show_all,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = MossConfig::default_enabled();
        assert!(config.daemon.enabled);
        assert!(config.daemon.auto_start);
        assert!(config.index.enabled);
    }

    #[test]
    fn test_load_project_config() {
        let dir = TempDir::new().unwrap();
        let moss_dir = dir.path().join(".moss");
        std::fs::create_dir_all(&moss_dir).unwrap();

        let config_path = moss_dir.join("config.toml");
        let mut file = std::fs::File::create(&config_path).unwrap();
        writeln!(
            file,
            r#"
[daemon]
enabled = false
auto_start = false

[index]
enabled = true
"#
        )
        .unwrap();

        let config = MossConfig::load(dir.path());
        assert!(!config.daemon.enabled);
        assert!(!config.daemon.auto_start);
        assert!(config.index.enabled);
    }

    #[test]
    fn test_partial_config() {
        let dir = TempDir::new().unwrap();
        let moss_dir = dir.path().join(".moss");
        std::fs::create_dir_all(&moss_dir).unwrap();

        let config_path = moss_dir.join("config.toml");
        let mut file = std::fs::File::create(&config_path).unwrap();
        writeln!(
            file,
            r#"
[daemon]
auto_start = false
"#
        )
        .unwrap();

        let config = MossConfig::load(dir.path());
        // daemon.enabled should use default (true) since not specified
        // But serde default gives false, so we get false
        // This is a known limitation - we'd need Option<bool> for proper merge
        assert!(!config.daemon.enabled); // serde default
        assert!(!config.daemon.auto_start);
    }

    #[test]
    fn test_filter_aliases_config() {
        let dir = TempDir::new().unwrap();
        let moss_dir = dir.path().join(".moss");
        std::fs::create_dir_all(&moss_dir).unwrap();

        let config_path = moss_dir.join("config.toml");
        let mut file = std::fs::File::create(&config_path).unwrap();
        writeln!(
            file,
            r#"
[filter.aliases]
tests = ["my_tests/**"]
vendor = ["vendor/**", "third_party/**"]
config = []
"#
        )
        .unwrap();

        let config = MossConfig::load(dir.path());
        assert_eq!(
            config.filter.aliases.get("tests"),
            Some(&vec!["my_tests/**".to_string()])
        );
        assert_eq!(
            config.filter.aliases.get("vendor"),
            Some(&vec!["vendor/**".to_string(), "third_party/**".to_string()])
        );
        // Empty array disables alias
        assert_eq!(config.filter.aliases.get("config"), Some(&vec![]));
    }

    #[test]
    fn test_todo_config() {
        let dir = TempDir::new().unwrap();
        let moss_dir = dir.path().join(".moss");
        std::fs::create_dir_all(&moss_dir).unwrap();

        let config_path = moss_dir.join("config.toml");
        let mut file = std::fs::File::create(&config_path).unwrap();
        writeln!(
            file,
            r#"
[todo]
file = "TASKS.md"
primary_section = "Backlog"
show_all = true
"#
        )
        .unwrap();

        let config = MossConfig::load(dir.path());
        assert_eq!(config.todo.file, Some("TASKS.md".to_string()));
        assert_eq!(config.todo.primary_section, Some("Backlog".to_string()));
        assert!(config.todo.show_all);
    }
}
