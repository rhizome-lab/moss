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
    let scripts_dir = root.join(".moss").join("scripts");

    if !scripts_dir.exists() {
        if json {
            println!("[]");
        } else {
            println!("No scripts directory at .moss/scripts/");
        }
        return 0;
    }

    let mut scripts = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&scripts_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "lua").unwrap_or(false) {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    scripts.push(name.to_string());
                }
            }
        }
    }

    scripts.sort();

    if json {
        println!("{}", serde_json::to_string(&scripts).unwrap());
    } else if scripts.is_empty() {
        println!("No scripts found in .moss/scripts/");
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

    let script_path = if script.ends_with(".lua") {
        root.join(script)
    } else {
        root.join(".moss")
            .join("scripts")
            .join(format!("{}.lua", script))
    };

    if !script_path.exists() {
        eprintln!("Script not found: {}", script_path.display());
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

    match runtime.run_file(&script_path) {
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
