//! Workflow execution engine.

use std::path::Path;
use std::process::Command;

use super::config::WorkflowConfig;
use super::strategies::{
    evaluate_condition, CacheStrategy, ContextStrategy, ExponentialRetry, FixedRetry, FlatContext,
    InMemoryCache, NoCache, NoRetry, RetryStrategy,
};

/// Result of running a workflow.
#[derive(Debug)]
pub struct WorkflowResult {
    pub success: bool,
    pub output: String,
    pub steps_executed: usize,
}

/// Run a workflow from a TOML file.
pub fn run_workflow(
    workflow_path: &Path,
    task: &str,
    root: &Path,
) -> Result<WorkflowResult, String> {
    let config = super::config::load_workflow(workflow_path)?;

    // Build strategies from config
    let mut context = build_context_strategy(&config);
    let mut cache = build_cache_strategy(&config);
    let mut retry = build_retry_strategy(&config);

    // Add initial task to context
    if !task.is_empty() {
        context.add("task", task);
    }

    // Execute based on workflow type
    if config.is_step_based() {
        run_step_workflow(
            &config,
            root,
            context.as_mut(),
            cache.as_mut(),
            retry.as_mut(),
        )
    } else if config.is_state_machine() {
        run_state_machine(
            &config,
            root,
            context.as_mut(),
            cache.as_mut(),
            retry.as_mut(),
        )
    } else {
        Err("Workflow must have either steps or states".to_string())
    }
}

fn build_context_strategy(config: &WorkflowConfig) -> Box<dyn ContextStrategy> {
    match config.workflow.context.strategy.as_str() {
        "task_tree" | "task_list" | "flat" | _ => Box::new(FlatContext::new(10)),
    }
}

fn build_cache_strategy(config: &WorkflowConfig) -> Box<dyn CacheStrategy> {
    match config.workflow.cache.strategy.as_str() {
        "in_memory" => Box::new(InMemoryCache::new(config.workflow.cache.preview_length)),
        "none" | _ => Box::new(NoCache),
    }
}

fn build_retry_strategy(config: &WorkflowConfig) -> Box<dyn RetryStrategy> {
    match config.workflow.retry.strategy.as_str() {
        "fixed" => Box::new(FixedRetry::new(
            config.workflow.retry.max_attempts,
            config.workflow.retry.base_delay,
        )),
        "exponential" => Box::new(ExponentialRetry::new(
            config.workflow.retry.max_attempts,
            config.workflow.retry.base_delay,
            config.workflow.retry.max_delay,
        )),
        "none" | _ => Box::new(NoRetry),
    }
}

/// Execute a step-based workflow.
fn run_step_workflow(
    config: &WorkflowConfig,
    root: &Path,
    context: &mut dyn ContextStrategy,
    cache: &mut dyn CacheStrategy,
    _retry: &mut dyn RetryStrategy,
) -> Result<WorkflowResult, String> {
    let mut output = String::new();
    let mut combined_outputs: Vec<(String, String)> = Vec::new();
    let mut steps_executed = 0;

    for step in &config.steps {
        // Check condition if present
        if let Some(ref condition) = step.condition {
            if !evaluate_condition(condition, &context.get_context(), &output) {
                continue; // Skip this step
            }
        }

        // Check cache
        if let Some(cached) = cache.get(&step.action) {
            context.add(&step.name, &cached);
            if config.workflow.combine_outputs {
                combined_outputs.push((step.name.clone(), cached.clone()));
            }
            output = cached;
            steps_executed += 1;
            continue;
        }

        // Execute the action
        match execute_action(&step.action, root) {
            Ok(result) => {
                context.add(&step.name, &result);
                cache.set(&step.action, &result);
                if config.workflow.combine_outputs {
                    combined_outputs.push((step.name.clone(), result.clone()));
                }
                output = result;
                steps_executed += 1;
            }
            Err(e) => {
                if step.on_error == "skip" {
                    continue;
                } else if step.on_error == "abort" {
                    return Ok(WorkflowResult {
                        success: false,
                        output: format!("Step '{}' failed: {}", step.name, e),
                        steps_executed,
                    });
                }
                // Default: continue but record error
                context.add(&step.name, &format!("ERROR: {}", e));
            }
        }
    }

    // Combine outputs if requested
    if config.workflow.combine_outputs {
        output = combined_outputs
            .iter()
            .map(|(name, out)| format!("=== {} ===\n{}", name, out.trim()))
            .collect::<Vec<_>>()
            .join("\n\n");
    }

    Ok(WorkflowResult {
        success: true,
        output,
        steps_executed,
    })
}

/// Execute a state machine workflow.
fn run_state_machine(
    config: &WorkflowConfig,
    root: &Path,
    context: &mut dyn ContextStrategy,
    cache: &mut dyn CacheStrategy,
    _retry: &mut dyn RetryStrategy,
) -> Result<WorkflowResult, String> {
    let initial_state = config
        .workflow
        .initial_state
        .as_ref()
        .ok_or("State machine workflow must have initial_state")?;

    let states: std::collections::HashMap<_, _> =
        config.states.iter().map(|s| (s.name.as_str(), s)).collect();

    let mut current_state_name = initial_state.as_str();
    let mut output = String::new();
    let mut turns = 0;
    let max_turns = config.workflow.limits.max_turns;

    loop {
        if turns >= max_turns {
            return Ok(WorkflowResult {
                success: false,
                output: format!("Max turns ({}) exceeded", max_turns),
                steps_executed: turns,
            });
        }

        let state = states
            .get(current_state_name)
            .ok_or_else(|| format!("Unknown state: {}", current_state_name))?;

        // Check if terminal
        if state.terminal {
            return Ok(WorkflowResult {
                success: true,
                output,
                steps_executed: turns,
            });
        }

        // Execute action if present
        if let Some(ref action) = state.action {
            // Check cache
            if let Some(cached) = cache.get(action) {
                output = cached;
            } else {
                match execute_action(action, root) {
                    Ok(result) => {
                        cache.set(action, &result);
                        output = result;
                    }
                    Err(e) => {
                        output = format!("ERROR: {}", e);
                    }
                }
            }
            context.add(&state.name, &output);
        }

        turns += 1;

        // Find next state based on transitions
        let mut next_state: Option<&str> = None;
        for transition in &state.transitions {
            if let Some(ref condition) = transition.condition {
                if evaluate_condition(condition, &context.get_context(), &output) {
                    next_state = transition.next.as_deref();
                    break;
                }
            } else {
                // Unconditional transition
                next_state = transition.next.as_deref();
                break;
            }
        }

        match next_state {
            Some(next) => current_state_name = next,
            None => {
                return Ok(WorkflowResult {
                    success: false,
                    output: format!("No valid transition from state '{}'", current_state_name),
                    steps_executed: turns,
                });
            }
        }
    }
}

/// Execute an action.
///
/// Action types:
/// - `shell: <command>` - Execute shell command
/// - `<args>` - Execute moss command (default)
fn execute_action(action: &str, root: &Path) -> Result<String, String> {
    // Check for shell: prefix
    if let Some(shell_cmd) = action.strip_prefix("shell:") {
        return execute_shell(shell_cmd.trim(), root);
    }

    // Parse action into command and args
    let parts: Vec<&str> = action.split_whitespace().collect();
    if parts.is_empty() {
        return Err("Empty action".to_string());
    }

    // Use current executable for moss commands
    let current_exe =
        std::env::current_exe().map_err(|e| format!("Failed to get current executable: {}", e))?;

    // Build moss command
    let output = Command::new(&current_exe)
        .args(&parts)
        .current_dir(root)
        .output()
        .map_err(|e| format!("Failed to execute action: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("Action failed: {}", stderr))
    }
}

/// Execute a shell command.
fn execute_shell(cmd: &str, root: &Path) -> Result<String, String> {
    let shell = if cfg!(windows) { "cmd" } else { "sh" };
    let flag = if cfg!(windows) { "/C" } else { "-c" };

    let output = Command::new(shell)
        .args([flag, cmd])
        .current_dir(root)
        .output()
        .map_err(|e| format!("Failed to execute shell command: {}", e))?;

    // Combine stdout and stderr for shell commands
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if output.status.success() {
        Ok(stdout.to_string())
    } else {
        // Return stderr if present, otherwise stdout
        if stderr.is_empty() {
            Err(format!("Shell command failed: {}", stdout))
        } else {
            Err(format!("Shell command failed: {}", stderr))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_step_workflow_execution() {
        let toml = r#"
[workflow]
name = "test"

[[steps]]
name = "step1"
action = "view --help"
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(toml.as_bytes()).unwrap();

        // This test would need moss in PATH to actually run
        // Just verify parsing works
        let config = super::super::config::load_workflow(file.path()).unwrap();
        assert_eq!(config.steps.len(), 1);
    }
}
