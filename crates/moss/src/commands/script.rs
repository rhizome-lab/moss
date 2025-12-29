//! Script command - run Lua scripts via @ prefix.
//!
//! Scripts live in `.moss/scripts/` and are invoked with `moss @script-name args...`
//! Builtin scripts (todo, config) are embedded and used as fallback.

use std::path::Path;

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
