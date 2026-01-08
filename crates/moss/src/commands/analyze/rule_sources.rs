//! Rule source system for conditional rule evaluation.
//!
//! Sources provide data that rules can use in `requires` predicates:
//! ```toml
//! requires = { rust.edition = ">=2024" }
//! requires = { env.CI = "true" }
//! requires = { path.matches = "**/tests/**" }
//! ```
//!
//! Built-in sources:
//! - `path` - file path matching (glob patterns)
//! - `env` - environment variables
//! - `git` - repository state (branch, staged, dirty)
//! - `config` - .moss/config.toml values
//! - Language sources: `rust`, `typescript`, `python`, `go`, etc.

use std::collections::HashMap;
use std::path::Path;

/// Context passed to sources for evaluation.
pub struct SourceContext<'a> {
    /// Absolute path to the file being analyzed.
    pub file_path: &'a Path,
    /// Path relative to project root.
    pub rel_path: &'a str,
    /// Project root directory.
    pub project_root: &'a Path,
}

/// A source of data for rule conditionals.
///
/// Each source owns a namespace (e.g., "rust", "env", "path") and provides
/// key-value data that rules can query in `requires` predicates.
pub trait RuleSource: Send + Sync {
    /// The namespace this source provides (e.g., "rust", "env", "path").
    fn namespace(&self) -> &str;

    /// Evaluate the source for a given file context.
    ///
    /// Returns a map of key-value pairs available under this namespace.
    /// For example, RustSource might return `{"edition": "2024", "resolver": "2"}`.
    ///
    /// Returns `None` if this source doesn't apply to the given file
    /// (e.g., RustSource returns None for Python files).
    fn evaluate(&self, ctx: &SourceContext) -> Option<HashMap<String, String>>;
}

/// Registry of all available rule sources.
#[derive(Default)]
pub struct SourceRegistry {
    sources: Vec<Box<dyn RuleSource>>,
}

impl SourceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a source. Sources are evaluated in registration order.
    pub fn register(&mut self, source: Box<dyn RuleSource>) {
        self.sources.push(source);
    }

    /// Evaluate all sources for a file, returning combined namespace.key -> value map.
    pub fn evaluate(&self, ctx: &SourceContext) -> HashMap<String, String> {
        let mut result = HashMap::new();
        for source in &self.sources {
            if let Some(values) = source.evaluate(ctx) {
                let ns = source.namespace();
                for (key, value) in values {
                    result.insert(format!("{}.{}", ns, key), value);
                }
            }
        }
        result
    }

    /// Get a specific value by full key (e.g., "rust.edition").
    pub fn get(&self, ctx: &SourceContext, key: &str) -> Option<String> {
        // Parse namespace.key
        let (ns, field) = key.split_once('.')?;

        for source in &self.sources {
            if source.namespace() == ns {
                if let Some(values) = source.evaluate(ctx) {
                    return values.get(field).cloned();
                }
            }
        }
        None
    }
}

// ============================================================================
// Built-in sources
// ============================================================================

/// Environment variable source.
///
/// Provides `env.VAR_NAME` for any environment variable.
pub struct EnvSource;

impl RuleSource for EnvSource {
    fn namespace(&self) -> &str {
        "env"
    }

    fn evaluate(&self, _ctx: &SourceContext) -> Option<HashMap<String, String>> {
        // Return all env vars - could be optimized to lazy evaluation
        Some(std::env::vars().collect())
    }
}

/// Path-based source for glob matching.
///
/// Provides `path.matches` for checking if file matches a pattern.
/// Note: This is evaluated specially since it needs the pattern from requires.
pub struct PathSource;

impl RuleSource for PathSource {
    fn namespace(&self) -> &str {
        "path"
    }

    fn evaluate(&self, ctx: &SourceContext) -> Option<HashMap<String, String>> {
        let mut result = HashMap::new();
        result.insert("rel".to_string(), ctx.rel_path.to_string());
        result.insert(
            "abs".to_string(),
            ctx.file_path.to_string_lossy().to_string(),
        );
        if let Some(ext) = ctx.file_path.extension() {
            result.insert("ext".to_string(), ext.to_string_lossy().to_string());
        }
        if let Some(name) = ctx.file_path.file_name() {
            result.insert("filename".to_string(), name.to_string_lossy().to_string());
        }
        Some(result)
    }
}

/// Git repository state source.
///
/// Provides `git.branch`, `git.dirty`, `git.staged`.
pub struct GitSource;

impl RuleSource for GitSource {
    fn namespace(&self) -> &str {
        "git"
    }

    fn evaluate(&self, ctx: &SourceContext) -> Option<HashMap<String, String>> {
        let mut result = HashMap::new();

        // Get current branch
        if let Ok(output) = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(ctx.project_root)
            .output()
        {
            if output.status.success() {
                let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
                result.insert("branch".to_string(), branch);
            }
        }

        // Check if file is staged
        if let Ok(output) = std::process::Command::new("git")
            .args(["diff", "--cached", "--name-only"])
            .current_dir(ctx.project_root)
            .output()
        {
            if output.status.success() {
                let staged = String::from_utf8_lossy(&output.stdout);
                let is_staged = staged.lines().any(|l| l == ctx.rel_path);
                result.insert("staged".to_string(), is_staged.to_string());
            }
        }

        // Check if repo is dirty
        if let Ok(output) = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(ctx.project_root)
            .output()
        {
            if output.status.success() {
                let dirty = !output.stdout.is_empty();
                result.insert("dirty".to_string(), dirty.to_string());
            }
        }

        Some(result)
    }
}

/// Rust project source - parses Cargo.toml for edition, resolver, etc.
///
/// Provides `rust.edition`, `rust.resolver`, `rust.name`.
pub struct RustSource;

impl RustSource {
    /// Find the nearest Cargo.toml for a given file path.
    fn find_cargo_toml(file_path: &Path) -> Option<std::path::PathBuf> {
        let mut current = file_path.parent()?;
        loop {
            let cargo_toml = current.join("Cargo.toml");
            if cargo_toml.exists() {
                return Some(cargo_toml);
            }
            current = current.parent()?;
        }
    }

    /// Parse edition from Cargo.toml content.
    fn parse_cargo_toml(content: &str) -> HashMap<String, String> {
        let mut result = HashMap::new();

        // Simple TOML parsing for key fields
        // TODO: Use proper TOML parser for robustness
        for line in content.lines() {
            let line = line.trim();

            if let Some(rest) = line.strip_prefix("edition") {
                if let Some(value) = Self::parse_value(rest) {
                    result.insert("edition".to_string(), value);
                }
            } else if let Some(rest) = line.strip_prefix("resolver") {
                if let Some(value) = Self::parse_value(rest) {
                    result.insert("resolver".to_string(), value);
                }
            } else if let Some(rest) = line.strip_prefix("name") {
                if let Some(value) = Self::parse_value(rest) {
                    result.insert("name".to_string(), value);
                }
            } else if let Some(rest) = line.strip_prefix("version") {
                if let Some(value) = Self::parse_value(rest) {
                    result.insert("version".to_string(), value);
                }
            }
        }

        result
    }

    /// Parse a TOML value from ` = "value"` or ` = 'value'`.
    fn parse_value(rest: &str) -> Option<String> {
        let rest = rest.trim();
        let rest = rest.strip_prefix('=')?;
        let rest = rest.trim();

        // Handle quoted strings
        if let Some(rest) = rest.strip_prefix('"') {
            return rest.strip_suffix('"').map(|s| s.to_string());
        }
        if let Some(rest) = rest.strip_prefix('\'') {
            return rest.strip_suffix('\'').map(|s| s.to_string());
        }

        // Handle unquoted values (numbers, etc.)
        Some(rest.to_string())
    }
}

impl RuleSource for RustSource {
    fn namespace(&self) -> &str {
        "rust"
    }

    fn evaluate(&self, ctx: &SourceContext) -> Option<HashMap<String, String>> {
        // Only apply to Rust files
        let ext = ctx.file_path.extension()?;
        if ext != "rs" {
            return None;
        }

        // Find nearest Cargo.toml
        let cargo_toml = Self::find_cargo_toml(ctx.file_path)?;
        let content = std::fs::read_to_string(&cargo_toml).ok()?;

        Some(Self::parse_cargo_toml(&content))
    }
}

/// Create a registry with all built-in sources.
pub fn builtin_registry() -> SourceRegistry {
    let mut registry = SourceRegistry::new();
    registry.register(Box::new(EnvSource));
    registry.register(Box::new(PathSource));
    registry.register(Box::new(GitSource));
    registry.register(Box::new(RustSource));
    // TODO: Add ConfigSource, TypeScriptSource, PythonSource, GoSource, etc.
    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_source() {
        // SAFETY: Test runs single-threaded, no concurrent env access
        unsafe {
            std::env::set_var("MOSS_TEST_VAR", "hello");
        }

        let ctx = SourceContext {
            file_path: Path::new("/tmp/test.rs"),
            rel_path: "test.rs",
            project_root: Path::new("/tmp"),
        };

        let registry = builtin_registry();
        let value = registry.get(&ctx, "env.MOSS_TEST_VAR");
        assert_eq!(value, Some("hello".to_string()));

        // SAFETY: Test cleanup
        unsafe {
            std::env::remove_var("MOSS_TEST_VAR");
        }
    }

    #[test]
    fn test_path_source() {
        let ctx = SourceContext {
            file_path: Path::new("/project/src/lib.rs"),
            rel_path: "src/lib.rs",
            project_root: Path::new("/project"),
        };

        let registry = builtin_registry();
        assert_eq!(
            registry.get(&ctx, "path.rel"),
            Some("src/lib.rs".to_string())
        );
        assert_eq!(registry.get(&ctx, "path.ext"), Some("rs".to_string()));
        assert_eq!(
            registry.get(&ctx, "path.filename"),
            Some("lib.rs".to_string())
        );
    }

    #[test]
    fn test_rust_source_parse_cargo_toml() {
        let content = r#"
[package]
name = "my-crate"
version = "0.1.0"
edition = "2024"
resolver = "2"
"#;
        let result = RustSource::parse_cargo_toml(content);
        assert_eq!(result.get("name"), Some(&"my-crate".to_string()));
        assert_eq!(result.get("version"), Some(&"0.1.0".to_string()));
        assert_eq!(result.get("edition"), Some(&"2024".to_string()));
        assert_eq!(result.get("resolver"), Some(&"2".to_string()));
    }

    #[test]
    fn test_rust_source_real_file() {
        // Test against this project's actual Cargo.toml
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let file_path = manifest_dir.join("src/lib.rs");
        let ctx = SourceContext {
            file_path: &file_path,
            rel_path: "src/lib.rs",
            project_root: manifest_dir,
        };

        let registry = builtin_registry();
        // Should find edition from Cargo.toml
        let edition = registry.get(&ctx, "rust.edition");
        assert!(edition.is_some(), "Should find rust.edition");
    }
}
