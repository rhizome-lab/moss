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
//! [sigil]
//! todo = ["TODO.md", "TASKS.md"]   # @todo expands to these files
//! config = [".moss/config.toml"]   # @config (default)
//!
//! [todo]
//! file = "TASKS.md"           # custom todo file (default: auto-detect)
//! primary_section = "Backlog" # default section for add/done/rm
//! show_all = true             # show all sections by default
//!
//! [view]
//! depth = 2                   # default tree depth (0=names, 1=signatures, 2=children)
//! line_numbers = true         # show line numbers by default
//! show_docs = true            # show full docstrings by default
//!
//! [analyze]
//! threshold = 10              # only show functions with complexity >= 10
//! compact = true              # use compact output for --overview
//!
//! [text-search]
//! limit = 50                  # default max results
//! ignore_case = true          # case-insensitive by default
//!
//! [pretty]
//! enabled = true              # auto-enable when TTY (default: auto)
//! colors = "auto"             # "auto", "always", or "never"
//! highlight = true            # syntax highlighting on signatures
//! ```

use crate::commands::analyze::AnalyzeConfig;
use crate::commands::text_search::TextSearchConfig;
use crate::commands::todo::TodoConfig;
use crate::commands::view::ViewConfig;
use crate::daemon::DaemonConfig;
use crate::filter::FilterConfig;
use crate::merge::Merge;
use crate::output::PrettyConfig;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

/// Index configuration.
#[derive(Debug, Clone, Deserialize, Merge, Default)]
#[serde(default)]
pub struct IndexConfig {
    /// Whether to create and use the file index. Default: true
    pub enabled: Option<bool>,
}

/// Sigil configuration for @ target prefix expansion.
///
/// Example:
/// ```toml
/// [sigil]
/// todo = ["TODO.md", "TASKS.md"]
/// config = [".moss/config.toml"]
/// ```
#[derive(Debug, Clone, Deserialize, Default, Merge)]
#[serde(default)]
pub struct SigilConfig {
    /// Map sigil names to file paths. Values are lists (may expand to multiple files).
    #[serde(flatten)]
    pub targets: HashMap<String, Vec<String>>,
}

impl SigilConfig {
    /// Get default sigil mappings.
    fn defaults() -> HashMap<String, Vec<String>> {
        let mut defaults = HashMap::new();
        defaults.insert("config".to_string(), vec![".moss/config.toml".to_string()]);
        defaults
    }

    /// Get targets for a sigil, falling back to defaults.
    /// Returns None if sigil is unknown or disabled (empty array).
    pub fn get(&self, name: &str) -> Option<Vec<String>> {
        if let Some(targets) = self.targets.get(name) {
            if targets.is_empty() {
                // Empty array disables the sigil
                return None;
            }
            return Some(targets.clone());
        }
        // Fall back to defaults
        Self::defaults().get(name).cloned()
    }
}

impl IndexConfig {
    pub fn enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }
}

/// Root configuration structure.
#[derive(Debug, Clone, Deserialize, Default, Merge)]
#[serde(default)]
pub struct MossConfig {
    pub daemon: DaemonConfig,
    pub index: IndexConfig,
    pub filter: FilterConfig,
    pub sigil: SigilConfig,
    pub todo: TodoConfig,
    pub view: ViewConfig,
    pub analyze: AnalyzeConfig,
    #[serde(rename = "text-search")]
    pub text_search: TextSearchConfig,
    pub pretty: PrettyConfig,
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

    /// Default config with serde defaults (enabled fields default to true).
    fn default_enabled() -> Self {
        Self::default()
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = MossConfig::default_enabled();
        assert!(config.daemon.enabled());
        assert!(config.daemon.auto_start());
        assert!(config.index.enabled());
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
        assert!(!config.daemon.enabled());
        assert!(!config.daemon.auto_start());
        assert!(config.index.enabled());
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
        // enabled is None (not specified), accessor returns true
        assert!(config.daemon.enabled());
        assert!(!config.daemon.auto_start());
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
        assert!(config.todo.show_all());
    }

    #[test]
    fn test_merge_preserves_explicit_values() {
        // Simulate: global sets enabled=false, project only sets auto_start=true
        // The explicit enabled=false should be preserved, not overwritten by default
        let global = MossConfig {
            daemon: DaemonConfig {
                enabled: Some(false), // explicitly disabled
                auto_start: None,
            },
            ..Default::default()
        };

        let project = MossConfig {
            daemon: DaemonConfig {
                enabled: None,          // not specified
                auto_start: Some(true), // explicitly enabled
            },
            ..Default::default()
        };

        let merged = global.merge(project);

        // enabled should stay false (from global), not become true (default)
        assert!(!merged.daemon.enabled());
        // auto_start should be true (from project)
        assert!(merged.daemon.auto_start());
    }

    #[test]
    fn test_pretty_config() {
        use crate::output::ColorMode;

        let dir = TempDir::new().unwrap();
        let moss_dir = dir.path().join(".moss");
        std::fs::create_dir_all(&moss_dir).unwrap();

        let config_path = moss_dir.join("config.toml");
        let mut file = std::fs::File::create(&config_path).unwrap();
        writeln!(
            file,
            r#"
[pretty]
enabled = true
colors = "always"
highlight = false
"#
        )
        .unwrap();

        let config = MossConfig::load(dir.path());
        assert_eq!(config.pretty.enabled, Some(true));
        assert_eq!(config.pretty.colors, Some(ColorMode::Always));
        assert_eq!(config.pretty.highlight, Some(false));
        assert!(!config.pretty.highlight());
    }
}
