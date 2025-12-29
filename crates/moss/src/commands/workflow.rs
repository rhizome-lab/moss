//! Script command - Lua scripts and TOML workflows.

use std::path::Path;

use clap::Subcommand;

#[cfg(feature = "lua")]
use crate::workflow::LuaRuntime;

#[derive(Subcommand)]
pub enum WorkflowAction {
    /// List available scripts
    List,

    /// Run a script
    Run {
        /// Script name or path to .lua file
        script: String,

        /// Task description (available as `task` variable in Lua)
        #[arg(short, long)]
        task: Option<String>,
    },
}

pub fn cmd_workflow(action: WorkflowAction, root: Option<&Path>, json: bool) -> i32 {
    match action {
        WorkflowAction::List => cmd_script_list(root, json),
        WorkflowAction::Run { script, task } => {
            cmd_script_run(&script, task.as_deref(), root, json)
        }
    }
}

fn cmd_script_list(root: Option<&Path>, json: bool) -> i32 {
    let root = root.unwrap_or_else(|| Path::new("."));
    let scripts = super::script::list_scripts(root);

    if json {
        println!("{}", serde_json::to_string(&scripts).unwrap());
    } else if scripts.is_empty() {
        println!("No scripts found");
    } else {
        for name in scripts {
            println!("{}", name);
        }
    }

    0
}

#[cfg(feature = "lua")]
fn cmd_script_run(script: &str, task: Option<&str>, root: Option<&Path>, json: bool) -> i32 {
    let root = root.unwrap_or_else(|| Path::new("."));

    // Check for explicit .lua path first
    let script_path = if script.ends_with(".lua") {
        Some(root.join(script))
    } else {
        let path = root
            .join(".moss")
            .join("scripts")
            .join(format!("{}.lua", script));
        if path.exists() {
            Some(path)
        } else {
            None
        }
    };

    // Get builtin if no user script
    let builtin_code = if script_path.is_none() {
        super::script::builtins::get(script)
    } else {
        None
    };

    if script_path.is_none() && builtin_code.is_none() {
        if json {
            println!(
                "{}",
                serde_json::json!({"error": format!("Script not found: {}", script)})
            );
        } else {
            eprintln!("Script not found: {}", script);
            eprintln!("Create it at: .moss/scripts/{}.lua", script);
        }
        return 1;
    }

    let runtime = match LuaRuntime::new(root) {
        Ok(r) => r,
        Err(e) => {
            if json {
                println!("{}", serde_json::json!({"error": e.to_string()}));
            } else {
                eprintln!("Failed to create Lua runtime: {}", e);
            }
            return 1;
        }
    };

    // Set task variable if provided
    if let Some(t) = task {
        if let Err(e) = runtime.run_string(&format!("task = {:?}", t)) {
            eprintln!("Failed to set task: {}", e);
            return 1;
        }
    }

    // Set args = {} for consistency with @ invocation
    if let Err(e) = runtime.run_string("args = {}") {
        eprintln!("Failed to set args: {}", e);
        return 1;
    }

    let result = if let Some(path) = script_path {
        runtime.run_file(&path)
    } else {
        runtime.run_string(builtin_code.unwrap())
    };

    match result {
        Ok(()) => {
            if json {
                println!("{}", serde_json::json!({"success": true}));
            }
            0
        }
        Err(e) => {
            if json {
                println!("{}", serde_json::json!({"error": e.to_string()}));
            } else {
                eprintln!("Script error: {}", e);
            }
            1
        }
    }
}

#[cfg(not(feature = "lua"))]
fn cmd_script_run(_script: &str, _task: Option<&str>, _root: Option<&Path>, _json: bool) -> i32 {
    eprintln!("Scripts require the 'lua' feature");
    eprintln!("Rebuild with: cargo build --features lua");
    1
}
