//! Tool registry for discovering and running tools.
//!
//! # Extensibility
//!
//! Users can register custom tools via [`register()`]:
//!
//! ```ignore
//! use rhizome_moss_tools::{Tool, ToolInfo, ToolCategory, ToolResult, ToolError, register_tool};
//! use std::path::Path;
//!
//! struct MyTool;
//!
//! impl Tool for MyTool {
//!     fn info(&self) -> ToolInfo { /* ... */ }
//!     fn is_available(&self) -> bool { /* ... */ }
//!     fn detect(&self, root: &Path) -> f32 { /* ... */ }
//!     fn run(&self, paths: &[&Path], root: &Path) -> Result<ToolResult, ToolError> { /* ... */ }
//! }
//!
//! // Register before first use
//! register_tool(&MyTool);
//! ```

use crate::{Diagnostic, Tool, ToolCategory, ToolResult};
use rayon::prelude::*;
use std::path::Path;
use std::sync::{OnceLock, RwLock};

/// Global registry of tool plugins.
static TOOLS: RwLock<Vec<&'static dyn Tool>> = RwLock::new(Vec::new());
static INITIALIZED: OnceLock<()> = OnceLock::new();

/// Register a custom tool plugin.
///
/// Call this before any detection operations to add custom tools.
/// Built-in tools are registered automatically on first use.
pub fn register(tool: &'static dyn Tool) {
    TOOLS.write().unwrap().push(tool);
}

/// Initialize built-in tools (called automatically on first use).
fn init_builtin() {
    use crate::adapters::*;

    INITIALIZED.get_or_init(|| {
        let mut tools = TOOLS.write().unwrap();
        // Python tools
        static RUFF: Ruff = Ruff;
        static MYPY: Mypy = Mypy;
        static PYRIGHT: Pyright = Pyright;
        // JavaScript/TypeScript tools
        static OXLINT: Oxlint = Oxlint;
        static OXFMT: Oxfmt = Oxfmt;
        static ESLINT: Eslint = Eslint;
        static BIOME_LINT: BiomeLint = BiomeLint;
        static BIOME_FORMAT: BiomeFormat = BiomeFormat;
        static PRETTIER: Prettier = Prettier;
        static TSGO: Tsgo = Tsgo;
        static TSC: Tsc = Tsc;
        static DENO: Deno = Deno;
        // Rust tools
        static CLIPPY: Clippy = Clippy;
        static RUSTFMT: Rustfmt = Rustfmt;
        // Go tools
        static GOFMT: Gofmt = Gofmt;
        static GOVET: Govet = Govet;

        tools.push(&RUFF);
        tools.push(&MYPY);
        tools.push(&PYRIGHT);
        tools.push(&OXLINT);
        tools.push(&OXFMT);
        tools.push(&ESLINT);
        tools.push(&BIOME_LINT);
        tools.push(&BIOME_FORMAT);
        tools.push(&PRETTIER);
        tools.push(&TSGO);
        tools.push(&TSC);
        tools.push(&DENO);
        tools.push(&CLIPPY);
        tools.push(&RUSTFMT);
        tools.push(&GOFMT);
        tools.push(&GOVET);
    });
}

/// Get a tool by name from the global registry.
pub fn get_tool(name: &str) -> Option<&'static dyn Tool> {
    init_builtin();
    TOOLS
        .read()
        .unwrap()
        .iter()
        .find(|t| t.info().name == name)
        .copied()
}

/// List all available tool names from the global registry.
pub fn list_tools() -> Vec<&'static str> {
    init_builtin();
    TOOLS
        .read()
        .unwrap()
        .iter()
        .map(|t| t.info().name)
        .collect()
}

/// Detect relevant tools for a project using the global registry.
pub fn detect_tools(root: &Path) -> Vec<(&'static dyn Tool, f32)> {
    init_builtin();
    let tools = TOOLS.read().unwrap();

    let mut relevant: Vec<_> = tools
        .iter()
        .map(|t| {
            let score = t.detect(root);
            (*t, score)
        })
        .filter(|(_, score)| *score > 0.0)
        .filter(|(t, _)| t.is_available())
        .collect();

    relevant.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    relevant
}

/// Registry of available tools.
pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    /// Create a registry with all built-in tools.
    pub fn with_builtins() -> Self {
        crate::default_registry()
    }

    /// Register a tool.
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.push(tool);
    }

    /// Get all registered tools.
    pub fn tools(&self) -> &[Box<dyn Tool>] {
        &self.tools
    }

    /// Get tools by category.
    pub fn tools_by_category(&self, category: ToolCategory) -> Vec<&dyn Tool> {
        self.tools
            .iter()
            .filter(|t| t.info().category == category)
            .map(|t| t.as_ref())
            .collect()
    }

    /// Get available tools (installed on system).
    pub fn available_tools(&self) -> Vec<&dyn Tool> {
        self.tools
            .iter()
            .filter(|t| t.is_available())
            .map(|t| t.as_ref())
            .collect()
    }

    /// Detect which tools are relevant for a project.
    ///
    /// Returns tools sorted by relevance (highest first).
    /// Note: Only checks availability for tools with positive detection scores
    /// (avoids spawning processes for irrelevant tools).
    /// Uses parallel iteration for better performance.
    pub fn detect(&self, root: &Path) -> Vec<(&dyn Tool, f32)> {
        let mut relevant: Vec<_> = self
            .tools
            .par_iter()
            .map(|t| {
                let score = t.detect(root);
                (t.as_ref(), score)
            })
            .filter(|(_, score)| *score > 0.0)
            .filter(|(t, _)| t.is_available())
            .collect();

        relevant.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        relevant
    }

    /// Run all relevant tools on a project.
    pub fn run_detected(&self, root: &Path, paths: &[&Path]) -> Vec<ToolResult> {
        let detected = self.detect(root);
        detected
            .into_iter()
            .filter_map(|(tool, _)| match tool.run(paths, root) {
                Ok(result) => Some(result),
                Err(e) => Some(ToolResult::failure(tool.info().name, e)),
            })
            .collect()
    }

    /// Run specific tools by name.
    pub fn run_named(&self, names: &[&str], root: &Path, paths: &[&Path]) -> Vec<ToolResult> {
        self.tools
            .iter()
            .filter(|t| names.contains(&t.info().name))
            .filter_map(|tool| match tool.run(paths, root) {
                Ok(result) => Some(result),
                Err(e) => Some(ToolResult::failure(tool.info().name, e)),
            })
            .collect()
    }

    /// Collect all diagnostics from multiple tool results.
    pub fn collect_diagnostics(results: &[ToolResult]) -> Vec<Diagnostic> {
        results.iter().flat_map(|r| r.diagnostics.clone()).collect()
    }
}
