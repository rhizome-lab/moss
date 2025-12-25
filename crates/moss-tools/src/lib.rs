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
mod diagnostic;
mod registry;
mod sarif;
mod tools;

pub use diagnostic::{Diagnostic, DiagnosticSeverity, Fix, Location};
pub use registry::ToolRegistry;
pub use sarif::SarifReport;
pub use tools::{
    has_config_file, has_files_with_extensions, Tool, ToolCategory, ToolError, ToolInfo, ToolResult,
};

/// Create a registry with all built-in tools.
pub fn default_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    for tool in adapters::all_adapters() {
        registry.register(tool);
    }
    registry
}
