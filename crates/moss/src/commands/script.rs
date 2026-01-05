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
    pub const AGENT: &str = include_str!("scripts/agent.lua");
    pub const BENCHMARK: &str = include_str!("scripts/benchmark.lua");

    /// Get builtin script by name.
    pub fn get(name: &str) -> Option<&'static str> {
        match name {
            "todo" => Some(TODO),
            "config" => Some(CONFIG),
            "agent" => Some(AGENT),
            "benchmark" => Some(BENCHMARK),
            _ => None,
        }
    }

    /// List all builtin script names.
    pub fn list() -> &'static [&'static str] {
        &["agent", "benchmark", "config", "todo"]
    }
}

/// Builtin Lua modules (for require()).
pub mod modules {
    pub const CLI: &str = include_str!("scripts/cli.lua");
    pub const TYPE: &str = include_str!("scripts/type.lua");
    pub const TYPE_DESCRIBE: &str = include_str!("scripts/type/describe.lua");
    pub const TYPE_VALIDATE: &str = include_str!("scripts/type/validate.lua");
    pub const TYPE_GENERATE: &str = include_str!("scripts/type/generate.lua");
    pub const TEST: &str = include_str!("scripts/test.lua");
    pub const TEST_PROPERTY: &str = include_str!("scripts/test/property.lua");
    pub const AGENT_RISK: &str = include_str!("scripts/agent/risk.lua");
    pub const AGENT_PARSER: &str = include_str!("scripts/agent/parser.lua");
    pub const AGENT_SESSION: &str = include_str!("scripts/agent/session.lua");
    pub const AGENT_CONTEXT: &str = include_str!("scripts/agent/context.lua");
    pub const AGENT_COMMANDS: &str = include_str!("scripts/agent/commands.lua");
    pub const AGENT_ROLES: &str = include_str!("scripts/agent/roles.lua");

    /// Get builtin module by name.
    pub fn get(name: &str) -> Option<&'static str> {
        match name {
            "agent" => Some(super::builtins::AGENT),
            "agent.risk" => Some(AGENT_RISK),
            "agent.parser" => Some(AGENT_PARSER),
            "agent.session" => Some(AGENT_SESSION),
            "agent.context" => Some(AGENT_CONTEXT),
            "agent.commands" => Some(AGENT_COMMANDS),
            "agent.roles" => Some(AGENT_ROLES),
            "cli" => Some(CLI),
            "type" => Some(TYPE),
            "type.describe" => Some(TYPE_DESCRIBE),
            "type.validate" => Some(TYPE_VALIDATE),
            "type.generate" => Some(TYPE_GENERATE),
            "test" => Some(TEST),
            "test.property" => Some(TEST_PROPERTY),
            _ => None,
        }
    }
}

#[derive(Subcommand)]
pub enum ScriptAction {
    /// List available scripts
    List,

    /// Create a new script from template
    New {
        /// Script name (without .lua extension)
        name: String,

        /// Template to use: basic (default), command
        #[arg(short, long, default_value = "basic")]
        template: String,
    },

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

        /// Arguments to pass to the script (available as `args` table in Lua)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

pub fn cmd_script(action: ScriptAction, root: Option<&Path>, json: bool) -> i32 {
    match action {
        ScriptAction::List => cmd_script_list(root, json),
        ScriptAction::New { name, template } => cmd_script_new(&name, &template, root, json),
        ScriptAction::Show { script } => cmd_script_show(&script, root, json),
        ScriptAction::Run { script, task, args } => {
            let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            cmd_script_run(&script, task.as_deref(), &args_refs, root, json)
        }
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

mod templates {
    pub const BASIC: &str = include_str!("scripts/template.lua");
    pub const COMMAND: &str = include_str!("scripts/template-command.lua");

    pub fn get(name: &str) -> Option<&'static str> {
        match name {
            "basic" => Some(BASIC),
            "command" => Some(COMMAND),
            _ => None,
        }
    }

    pub fn list() -> &'static [&'static str] {
        &["basic", "command"]
    }
}

fn cmd_script_new(name: &str, template: &str, root: Option<&Path>, json: bool) -> i32 {
    let root = root.unwrap_or_else(|| Path::new("."));
    let scripts_dir = root.join(".moss").join("scripts");
    let script_path = scripts_dir.join(format!("{}.lua", name));

    // Get template
    let template_content = match templates::get(template) {
        Some(t) => t,
        None => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({"error": "Unknown template", "available": templates::list()})
                );
            } else {
                eprintln!(
                    "Unknown template '{}'. Available: {}",
                    template,
                    templates::list().join(", ")
                );
            }
            return 1;
        }
    };

    // Check if script already exists
    if script_path.exists() {
        if json {
            println!(
                "{}",
                serde_json::json!({"error": "Script already exists", "path": script_path})
            );
        } else {
            eprintln!("Script already exists: {}", script_path.display());
        }
        return 1;
    }

    // Check if it would shadow a builtin
    if builtins::get(name).is_some() {
        if !json {
            println!("Note: This will override the builtin '{}' script", name);
        }
    }

    // Create scripts directory if needed
    if let Err(e) = std::fs::create_dir_all(&scripts_dir) {
        if json {
            println!("{}", serde_json::json!({"error": e.to_string()}));
        } else {
            eprintln!("Failed to create scripts directory: {}", e);
        }
        return 1;
    }

    // Generate script from template
    let content = template_content.replace("{name}", name);

    if let Err(e) = std::fs::write(&script_path, &content) {
        if json {
            println!("{}", serde_json::json!({"error": e.to_string()}));
        } else {
            eprintln!("Failed to write script: {}", e);
        }
        return 1;
    }

    if json {
        println!(
            "{}",
            serde_json::json!({"created": script_path, "name": name})
        );
    } else {
        println!("Created: {}", script_path.display());
        println!("Run with: moss @{}", name);
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

/// Print Lua code with syntax highlighting via tree-sitter.
fn print_lua_highlighted(code: &str) {
    use crate::tree::highlight_source;
    println!("{}", highlight_source(code, "lua", true));
}

#[cfg(feature = "lua")]
fn cmd_script_run(
    script: &str,
    task: Option<&str>,
    args: &[&str],
    root: Option<&Path>,
    json: bool,
) -> i32 {
    let root = root.unwrap_or_else(|| Path::new("."));

    // Check for explicit .lua path first
    let script_path = if script.ends_with(".lua") {
        Some(root.join(script))
    } else {
        let path = root
            .join(".moss")
            .join("scripts")
            .join(format!("{}.lua", script));
        if path.exists() { Some(path) } else { None }
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
    if let Some(t) = task
        && let Err(e) = runtime.run_string(&format!("task = {:?}", t))
    {
        eprintln!("Failed to set task: {}", e);
        return 1;
    }

    // Set args table from command line arguments
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
fn cmd_script_run(
    _script: &str,
    _task: Option<&str>,
    _args: &[&str],
    _root: Option<&Path>,
    _json: bool,
) -> i32 {
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
