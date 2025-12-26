//! Daemon management commands for moss CLI.

use crate::daemon;
use crate::paths::get_moss_dir;
use clap::Subcommand;
use std::path::Path;

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
}

/// Run a daemon management action
pub fn cmd_daemon(action: DaemonAction, root: Option<&Path>, json: bool) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    let client = daemon::DaemonClient::new(&root);

    let moss_dir = get_moss_dir(&root);
    match action {
        DaemonAction::Status => {
            if !client.is_available() {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "running": false,
                            "socket": moss_dir.join("daemon.sock").to_string_lossy()
                        })
                    );
                } else {
                    eprintln!("Daemon is not running");
                    eprintln!("Socket: {}", moss_dir.join("daemon.sock").display());
                }
                return 1;
            }

            match client.status() {
                Ok(status) => {
                    if json {
                        println!(
                            "{}",
                            serde_json::json!({
                                "running": true,
                                "uptime_secs": status.uptime_secs,
                                "files_indexed": status.files_indexed,
                                "symbols_indexed": status.symbols_indexed,
                                "queries_served": status.queries_served,
                                "pid": status.pid
                            })
                        );
                    } else {
                        println!("Daemon Status");
                        println!("  Running: yes");
                        if let Some(pid) = status.pid {
                            println!("  PID: {}", pid);
                        }
                        println!("  Uptime: {} seconds", status.uptime_secs);
                        println!("  Files indexed: {}", status.files_indexed);
                        println!("  Symbols indexed: {}", status.symbols_indexed);
                        println!("  Queries served: {}", status.queries_served);
                    }
                    0
                }
                Err(e) => {
                    eprintln!("Failed to get daemon status: {}", e);
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

            // Start the daemon process
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

        DaemonAction::Run => {
            // Run daemon in foreground (blocking)
            match daemon::run_daemon(&root) {
                Ok(code) => code,
                Err(e) => {
                    eprintln!("Daemon error: {}", e);
                    1
                }
            }
        }
    }
}
