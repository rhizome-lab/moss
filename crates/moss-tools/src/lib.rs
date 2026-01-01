//! Unified interface for external development tools.
//!
//! Provides adapters for linters, formatters, and type checkers with
//! standardized output formats (SARIF, JSON).
//!
//! # Tool Categories
//!
//! - **Linters**: Find code issues (oxlint, eslint, ruff check, biome lint)
//! - **Formatters**: Check/fix code style (prettier, black, rustfmt, biome format)
//! - **Type checkers**: Find type errors (tsc, mypy, pyright, cargo check)
//!
//! # Custom Tools
//!
//! Tools can be configured in `.moss/tools.toml`:
//!
//! ```toml
//! [tools.semgrep]
//! command = ["semgrep", "--sarif", "--config=auto", "."]
//! output = "sarif"
//! category = "linter"
//! extensions = ["py", "js", "go"]
//! detect = ["semgrep.yaml", ".semgrep.yml"]
//! ```
//!
//! # Example
//!
//! ```ignore
//! use moss_tools::{ToolRegistry, OutputFormat};
//!
//! let registry = ToolRegistry::default();
//! let results = registry.run_all(&["src/"], &["*.rs", "*.ts"])?;
//!
//! // Output as SARIF
//! println!("{}", results.to_sarif());
//! ```

pub mod adapters;
mod custom;
mod diagnostic;
mod registry;
mod sarif;
pub mod test_runners;
mod tools;

pub use custom::{CustomTool, CustomToolConfig, ToolsConfig, load_custom_tools};
pub use diagnostic::{Diagnostic, DiagnosticSeverity, Fix, Location};
pub use registry::ToolRegistry;
pub use sarif::SarifReport;
pub use tools::{Tool, ToolCategory, ToolError, ToolInfo, ToolResult, has_config_file};

use std::path::Path;

/// Create a registry with all built-in tools.
pub fn default_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    for tool in adapters::all_adapters() {
        registry.register(tool);
    }
    registry
}

/// Create a registry with built-in tools and custom tools from the given root.
pub fn registry_with_custom(root: &Path) -> ToolRegistry {
    let mut registry = default_registry();
    for tool in load_custom_tools(root) {
        registry.register(tool);
    }
    registry
}
