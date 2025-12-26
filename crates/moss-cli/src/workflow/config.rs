//! Workflow configuration and TOML parsing.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Top-level workflow configuration from TOML.
#[derive(Debug, Deserialize, Serialize)]
pub struct WorkflowConfig {
    pub workflow: WorkflowMetadata,
    #[serde(default)]
    pub steps: Vec<WorkflowStep>,
    #[serde(default)]
    pub states: Vec<WorkflowState>,
}

/// Workflow metadata section.
#[derive(Debug, Deserialize, Serialize)]
pub struct WorkflowMetadata {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub initial_state: Option<String>,
    #[serde(default)]
    pub limits: WorkflowLimits,
    #[serde(default)]
    pub context: ContextConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub retry: RetryConfig,
    #[serde(default)]
    pub llm: Option<LlmConfig>,
}

/// Workflow execution limits.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct WorkflowLimits {
    #[serde(default = "default_max_turns")]
    pub max_turns: usize,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}

fn default_max_turns() -> usize {
    20
}

/// Context management strategy configuration.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct ContextConfig {
    #[serde(default = "default_context_strategy")]
    pub strategy: String,
}

fn default_context_strategy() -> String {
    "flat".to_string()
}

/// Cache strategy configuration.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct CacheConfig {
    #[serde(default = "default_cache_strategy")]
    pub strategy: String,
    #[serde(default)]
    pub preview_length: Option<usize>,
}

fn default_cache_strategy() -> String {
    "none".to_string()
}

/// Retry strategy configuration.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct RetryConfig {
    #[serde(default = "default_retry_strategy")]
    pub strategy: String,
    #[serde(default = "default_max_attempts")]
    pub max_attempts: usize,
    #[serde(default = "default_base_delay")]
    pub base_delay: f64,
    #[serde(default)]
    pub max_delay: Option<f64>,
}

fn default_retry_strategy() -> String {
    "none".to_string()
}

fn default_max_attempts() -> usize {
    3
}

fn default_base_delay() -> f64 {
    1.0
}

/// LLM configuration (optional).
#[derive(Debug, Deserialize, Serialize)]
pub struct LlmConfig {
    #[serde(default = "default_llm_strategy")]
    pub strategy: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub allow_parallel: bool,
}

fn default_llm_strategy() -> String {
    "simple".to_string()
}

/// A step in a step-based workflow.
#[derive(Debug, Deserialize, Serialize)]
pub struct WorkflowStep {
    pub name: String,
    pub action: String,
    #[serde(default)]
    pub on_error: String,
    #[serde(default)]
    pub condition: Option<String>,
}

/// A state in a state machine workflow.
#[derive(Debug, Deserialize, Serialize)]
pub struct WorkflowState {
    pub name: String,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub terminal: bool,
    #[serde(default)]
    pub transitions: Vec<Transition>,
    #[serde(default)]
    pub parallel: Option<Vec<ParallelAction>>,
}

/// A transition between states.
#[derive(Debug, Deserialize, Serialize)]
pub struct Transition {
    #[serde(default)]
    pub condition: Option<String>,
    #[serde(default)]
    pub next: Option<String>,
}

/// A parallel action within a state.
#[derive(Debug, Deserialize, Serialize)]
pub struct ParallelAction {
    pub name: String,
    pub action: String,
}

/// Load and parse a workflow from a TOML file.
pub fn load_workflow(path: &Path) -> Result<WorkflowConfig, String> {
    let content =
        fs::read_to_string(path).map_err(|e| format!("Failed to read workflow file: {}", e))?;

    toml::from_str(&content).map_err(|e| format!("Failed to parse workflow TOML: {}", e))
}

impl WorkflowConfig {
    /// Check if this is a step-based workflow.
    pub fn is_step_based(&self) -> bool {
        !self.steps.is_empty()
    }

    /// Check if this is a state machine workflow.
    pub fn is_state_machine(&self) -> bool {
        !self.states.is_empty()
    }

    /// Check if this workflow uses LLM.
    #[allow(dead_code)] // Used in tests, will be used when LLM integration is complete
    pub fn uses_llm(&self) -> bool {
        self.workflow.llm.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_step_workflow() {
        let toml = r#"
[workflow]
name = "test"
description = "Test workflow"

[[steps]]
name = "step1"
action = "analyze --health"

[[steps]]
name = "step2"
action = "view ."
"#;
        let config: WorkflowConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.workflow.name, "test");
        assert!(config.is_step_based());
        assert!(!config.uses_llm());
        assert_eq!(config.steps.len(), 2);
    }

    #[test]
    fn test_parse_state_machine_workflow() {
        let toml = r#"
[workflow]
name = "test-sm"
initial_state = "start"

[[states]]
name = "start"
action = "analyze --health"

[[states.transitions]]
condition = "has_errors"
next = "fix"

[[states.transitions]]
next = "done"

[[states]]
name = "done"
terminal = true
"#;
        let config: WorkflowConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.workflow.name, "test-sm");
        assert!(config.is_state_machine());
        assert_eq!(config.states.len(), 2);
        assert_eq!(config.states[0].transitions.len(), 2);
    }

    #[test]
    fn test_parse_llm_workflow() {
        let toml = r#"
[workflow]
name = "agentic"

[workflow.llm]
strategy = "simple"
model = "claude-3-opus"
system_prompt = "You are a helpful assistant."
"#;
        let config: WorkflowConfig = toml::from_str(toml).unwrap();
        assert!(config.uses_llm());
        let llm = config.workflow.llm.unwrap();
        assert_eq!(llm.model, Some("claude-3-opus".to_string()));
    }
}
