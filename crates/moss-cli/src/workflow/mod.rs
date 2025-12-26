//! TOML-based workflow engine.
//!
//! Workflows orchestrate moss primitives (view, edit, analyze) through:
//! - Step-based execution (linear sequence)
//! - State machine execution (conditional transitions)
//!
//! LLM integration is scaffolded but not yet implemented.

mod config;
mod execute;
#[allow(dead_code)]
mod llm;
#[allow(dead_code)]
mod strategies;

pub use config::load_workflow;
pub use execute::run_workflow;
