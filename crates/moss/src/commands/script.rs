//! Script command - run Lua scripts via @ prefix.
//!
//! Scripts live in `.moss/scripts/` and are invoked with `moss @script-name args...`

use std::path::Path;

#[cfg(feature = "lua")]
use crate::workflow::LuaRuntime;

/// Run a script from .moss/scripts/.
/// Called from main when @ prefix is detected.
pub fn run_script(name: &str, args: &[&str]) -> i32 {
    let root = Path::new(".");
    run_script_impl(name, args, root)
}

#[cfg(feature = "lua")]
fn run_script_impl(name: &str, args: &[&str], root: &Path) -> i32 {
    let script_path = root
        .join(".moss")
        .join("scripts")
        .join(format!("{}.lua", name));

    if !script_path.exists() {
        eprintln!("Script not found: {}", script_path.display());
        eprintln!("Create it at: .moss/scripts/{}.lua", name);
        return 1;
    }

    let runtime = match LuaRuntime::new(root) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to create Lua runtime: {}", e);
            return 1;
        }
    };

    // Set args as a Lua table
    if !args.is_empty() {
        let args_lua = args
            .iter()
            .enumerate()
            .map(|(i, a)| format!("[{}] = {:?}", i + 1, a))
            .collect::<Vec<_>>()
            .join(", ");
        if let Err(e) = runtime.run_string(&format!("args = {{ {} }}", args_lua)) {
            eprintln!("Failed to set args: {}", e);
            return 1;
        }
    } else {
        if let Err(e) = runtime.run_string("args = {}") {
            eprintln!("Failed to set args: {}", e);
            return 1;
        }
    }

    match runtime.run_file(&script_path) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("Script error: {}", e);
            1
        }
    }
}

#[cfg(not(feature = "lua"))]
fn run_script_impl(_name: &str, _args: &[&str], _root: &Path) -> i32 {
    eprintln!("Scripts require the 'lua' feature");
    eprintln!("Rebuild with: cargo build --features lua");
    1
}

/// List available scripts in .moss/scripts/.
pub fn list_scripts(root: &Path) -> Vec<String> {
    let scripts_dir = root.join(".moss").join("scripts");

    if !scripts_dir.exists() {
        return vec![];
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
    scripts
}
