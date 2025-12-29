//! Script command - run Lua scripts via @ prefix or `moss script` subcommand.
//!
//! Scripts live in `.moss/scripts/` and are invoked with `moss @script-name args...`
//! Builtin scripts (todo, config) are embedded and used as fallback.

use std::path::Path;

use clap::Subcommand;

#[cfg(feature = "lua")]
use crate::workflow::LuaRuntime;

/// Builtin scripts embedded in the binary.
/// User scripts in .moss/scripts/ take precedence.
pub mod builtins {
    pub const TODO: &str = include_str!("scripts/todo.lua");
    pub const CONFIG: &str = include_str!("scripts/config.lua");

    /// Get builtin script by name.
    pub fn get(name: &str) -> Option<&'static str> {
        match name {
            "todo" => Some(TODO),
            "config" => Some(CONFIG),
            _ => None,
        }
    }

    /// List all builtin script names.
    pub fn list() -> &'static [&'static str] {
        &["config", "todo"]
    }
}

#[derive(Subcommand)]
pub enum ScriptAction {
    /// List available scripts
    List,

    /// Show script source (resolved path and highlighted code)
    Show {
        /// Script name
        script: String,
    },

    /// Run a script
    Run {
        /// Script name or path to .lua file
        script: String,

        /// Task description (available as `task` variable in Lua)
        #[arg(short, long)]
        task: Option<String>,
    },
}

pub fn cmd_script(action: ScriptAction, root: Option<&Path>, json: bool) -> i32 {
    match action {
        ScriptAction::List => cmd_script_list(root, json),
        ScriptAction::Show { script } => cmd_script_show(&script, root, json),
        ScriptAction::Run { script, task } => cmd_script_run(&script, task.as_deref(), root, json),
    }
}

fn cmd_script_list(root: Option<&Path>, json: bool) -> i32 {
    let root = root.unwrap_or_else(|| Path::new("."));
    let scripts = list_scripts(root);

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

fn cmd_script_show(script: &str, root: Option<&Path>, json: bool) -> i32 {
    let root = root.unwrap_or_else(|| Path::new("."));

    // Check user script first
    let user_path = root
        .join(".moss")
        .join("scripts")
        .join(format!("{}.lua", script));

    if user_path.exists() {
        // User script
        if json {
            let content = std::fs::read_to_string(&user_path).unwrap_or_default();
            println!(
                "{}",
                serde_json::json!({
                    "name": script,
                    "source": "user",
                    "path": user_path.display().to_string(),
                    "content": content
                })
            );
        } else {
            println!("# {} (user script)", script);
            println!("# Path: {}", user_path.display());
            println!();
            // Read and print with basic highlighting
            if let Ok(content) = std::fs::read_to_string(&user_path) {
                print_lua_highlighted(&content);
            }
        }
        return 0;
    }

    // Check builtin
    if let Some(content) = builtins::get(script) {
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "name": script,
                    "source": "builtin",
                    "path": format!("(embedded) crates/moss/src/commands/scripts/{}.lua", script),
                    "content": content
                })
            );
        } else {
            println!("# {} (builtin)", script);
            println!(
                "# Path: crates/moss/src/commands/scripts/{}.lua (embedded)",
                script
            );
            println!();
            print_lua_highlighted(content);
        }
        return 0;
    }

    eprintln!("Script not found: {}", script);
    1
}

/// Print Lua code with basic syntax highlighting (comments in dim)
fn print_lua_highlighted(code: &str) {
    for line in code.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("--") {
            // Comment - print dim
            println!("\x1b[2m{}\x1b[0m", line);
        } else if trimmed.starts_with("local ") || trimmed.starts_with("function ") {
            // Keywords - print bold
            println!("\x1b[1m{}\x1b[0m", line);
        } else {
            println!("{}", line);
        }
    }
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
        builtins::get(script)
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

/// Run a script from .moss/scripts/ or builtins.
/// Called from main when @ prefix is detected.
pub fn run_script(name: &str, args: &[&str]) -> i32 {
    let root = Path::new(".");
    run_script_impl(name, args, root)
}

#[cfg(feature = "lua")]
fn run_script_impl(name: &str, args: &[&str], root: &Path) -> i32 {
    // Check user script first
    let script_path = root
        .join(".moss")
        .join("scripts")
        .join(format!("{}.lua", name));

    let runtime = match LuaRuntime::new(root) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to create Lua runtime: {}", e);
            return 1;
        }
    };

    // Set args as a Lua table
    let args_lua = if args.is_empty() {
        "args = {}".to_string()
    } else {
        let entries = args
            .iter()
            .enumerate()
            .map(|(i, a)| format!("[{}] = {:?}", i + 1, a))
            .collect::<Vec<_>>()
            .join(", ");
        format!("args = {{ {} }}", entries)
    };

    if let Err(e) = runtime.run_string(&args_lua) {
        eprintln!("Failed to set args: {}", e);
        return 1;
    }

    // Try user script first
    if script_path.exists() {
        match runtime.run_file(&script_path) {
            Ok(()) => return 0,
            Err(e) => {
                eprintln!("Script error: {}", e);
                return 1;
            }
        }
    }

    // Fall back to builtin
    if let Some(builtin_code) = builtins::get(name) {
        match runtime.run_string(builtin_code) {
            Ok(()) => return 0,
            Err(e) => {
                eprintln!("Script error: {}", e);
                return 1;
            }
        }
    }

    eprintln!("Script not found: {}", name);
    eprintln!("Create it at: .moss/scripts/{}.lua", name);
    eprintln!("Or use a builtin: {}", builtins::list().join(", "));
    1
}

#[cfg(not(feature = "lua"))]
fn run_script_impl(_name: &str, _args: &[&str], _root: &Path) -> i32 {
    eprintln!("Scripts require the 'lua' feature");
    eprintln!("Rebuild with: cargo build --features lua");
    1
}

/// List available scripts (user + builtins).
pub fn list_scripts(root: &Path) -> Vec<String> {
    let mut scripts: Vec<String> = builtins::list().iter().map(|s| s.to_string()).collect();

    let scripts_dir = root.join(".moss").join("scripts");
    if scripts_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&scripts_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "lua").unwrap_or(false) {
                    if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                        if !scripts.contains(&name.to_string()) {
                            scripts.push(name.to_string());
                        }
                    }
                }
            }
        }
    }

    scripts.sort();
    scripts
}
