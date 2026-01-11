//! Daemon management commands for moss CLI.

use crate::daemon::{self, DaemonClient, Response, global_socket_path};
use clap::Subcommand;
use std::path::PathBuf;

/// Handle a daemon response, calling success_fn for Ok(resp.ok) case.
/// Returns exit code: 0 for success, 1 for error.
fn handle_response<F>(result: Result<Response, String>, json: bool, success_fn: F) -> i32
where
    F: FnOnce(&Response, bool),
{
    match result {
        Ok(resp) if resp.ok => {
            success_fn(&resp, json);
            0
        }
        Ok(resp) => {
            eprintln!("Error: {}", resp.error.unwrap_or_default());
            1
        }
        Err(e) => {
            eprintln!("Failed: {}", e);
            1
        }
    }
}

#[derive(Subcommand)]
pub enum DaemonAction {
    /// Show daemon status
    Status,

    /// Stop the daemon
    Stop,

    /// Start the daemon (background)
    Start,

    /// Run the daemon in foreground (for debugging)
    Run,

    /// Add a root to watch
    Add {
        /// Path to the project root
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Remove a root from watching
    Remove {
        /// Path to the project root
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// List all watched roots
    List,
}

/// Run a daemon management action
pub fn cmd_daemon(action: DaemonAction, json: bool) -> i32 {
    let client = DaemonClient::new();

    match action {
        DaemonAction::Status => {
            if !client.is_available() {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "running": false,
                            "socket": global_socket_path()
                        })
                    );
                } else {
                    eprintln!("Daemon is not running");
                    eprintln!("Socket: {}", global_socket_path().display());
                }
                return 1;
            }

            match client.status() {
                Ok(resp) if resp.ok => {
                    if json {
                        println!("{}", serde_json::to_string(&resp.data).unwrap_or_default());
                    } else if let Some(data) = resp.data {
                        println!("Daemon Status");
                        println!("  Running: yes");
                        if let Some(pid) = data.get("pid") {
                            println!("  PID: {}", pid);
                        }
                        if let Some(uptime) = data.get("uptime_secs") {
                            println!("  Uptime: {} seconds", uptime);
                        }
                        if let Some(roots) = data.get("roots_watched") {
                            println!("  Roots watched: {}", roots);
                        }
                    }
                    0
                }
                Ok(resp) => {
                    eprintln!("Error: {}", resp.error.unwrap_or_default());
                    1
                }
                Err(e) => {
                    eprintln!("Failed to get status: {}", e);
                    1
                }
            }
        }

        DaemonAction::Stop => {
            if !client.is_available() {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"success": false, "error": "daemon not running"})
                    );
                } else {
                    eprintln!("Daemon is not running");
                }
                return 1;
            }

            match client.shutdown() {
                Ok(()) => {
                    if json {
                        println!("{}", serde_json::json!({"success": true}));
                    } else {
                        println!("Daemon stopped");
                    }
                    0
                }
                Err(e) => {
                    // Connection reset is expected when daemon shuts down
                    if e.contains("Connection reset") || e.contains("Broken pipe") {
                        if json {
                            println!("{}", serde_json::json!({"success": true}));
                        } else {
                            println!("Daemon stopped");
                        }
                        0
                    } else {
                        eprintln!("Failed to stop daemon: {}", e);
                        1
                    }
                }
            }
        }

        DaemonAction::Start => {
            if client.is_available() {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"success": false, "error": "daemon already running"})
                    );
                } else {
                    eprintln!("Daemon is already running");
                }
                return 1;
            }

            if client.ensure_running() {
                if json {
                    println!("{}", serde_json::json!({"success": true}));
                } else {
                    println!("Daemon started");
                }
                0
            } else {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"success": false, "error": "failed to start daemon"})
                    );
                } else {
                    eprintln!("Failed to start daemon");
                }
                1
            }
        }

        DaemonAction::Run => match daemon::run_daemon() {
            Ok(code) => code,
            Err(e) => {
                eprintln!("Daemon error: {}", e);
                1
            }
        },

        DaemonAction::Add { path } => {
            let root = std::fs::canonicalize(&path).unwrap_or(path);

            if !client.ensure_running() {
                eprintln!("Failed to start daemon");
                return 1;
            }

            handle_response(client.add_root(&root), json, |resp, json| {
                if json {
                    println!("{}", serde_json::to_string(&resp.data).unwrap_or_default());
                } else if let Some(data) = &resp.data {
                    if data.get("added") == Some(&serde_json::json!(true)) {
                        println!("Added: {}", root.display());
                    } else {
                        println!(
                            "Already watching: {}",
                            data.get("reason").and_then(|r| r.as_str()).unwrap_or("")
                        );
                    }
                }
            })
        }

        DaemonAction::Remove { path } => {
            let root = std::fs::canonicalize(&path).unwrap_or(path);

            if !client.is_available() {
                eprintln!("Daemon is not running");
                return 1;
            }

            handle_response(client.remove_root(&root), json, |resp, json| {
                if json {
                    println!("{}", serde_json::to_string(&resp.data).unwrap_or_default());
                } else if let Some(data) = &resp.data {
                    if data.get("removed") == Some(&serde_json::json!(true)) {
                        println!("Removed: {}", root.display());
                    } else {
                        println!("Was not watching: {}", root.display());
                    }
                }
            })
        }

        DaemonAction::List => {
            if !client.is_available() {
                if json {
                    println!("{}", serde_json::json!([]));
                } else {
                    eprintln!("Daemon is not running");
                }
                return 1;
            }

            handle_response(client.list_roots(), json, |resp, json| {
                if json {
                    println!("{}", serde_json::to_string(&resp.data).unwrap_or_default());
                } else if let Some(data) = &resp.data
                    && let Some(roots) = data.as_array()
                {
                    if roots.is_empty() {
                        println!("No roots being watched");
                    } else {
                        println!("Watched roots:");
                        for root in roots {
                            if let Some(path) = root.as_str() {
                                println!("  {}", path);
                            }
                        }
                    }
                }
            })
        }
    }
}
