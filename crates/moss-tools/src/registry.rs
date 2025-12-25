//! Tool registry for discovering and running tools.

use crate::{Diagnostic, Tool, ToolCategory, ToolResult};
use std::path::Path;

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
    pub fn detect(&self, root: &Path) -> Vec<(&dyn Tool, f32)> {
        let mut relevant: Vec<_> = self
            .tools
            .iter()
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
