//! Global daemon for watching multiple codebases and keeping indexes fresh.
//!
//! The daemon watches file changes across registered roots and incrementally
//! refreshes their indexes. Index queries go directly to SQLite files.

use crate::config::MossConfig;
use crate::index::FileIndex;
use crate::merge::Merge;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::UnixListener;

/// Get global daemon socket path (~/.config/moss/daemon.sock)
pub fn global_socket_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("moss")
        .join("daemon.sock")
}

/// Daemon configuration.
#[derive(Debug, Clone, Deserialize, Merge, Default)]
#[serde(default)]
pub struct DaemonConfig {
    /// Whether to use the daemon. Default: true
    pub enabled: Option<bool>,
    /// Whether to auto-start the daemon. Default: true
    pub auto_start: Option<bool>,
}

impl DaemonConfig {
    pub fn enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }

    pub fn auto_start(&self) -> bool {
        self.auto_start.unwrap_or(true)
    }
}

/// Daemon request - minimal protocol for managing watched roots.
/// Index queries go directly to SQLite files, not through daemon.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "cmd")]
pub enum Request {
    /// Add a root to watch for file changes
    #[serde(rename = "add")]
    Add { root: PathBuf },
    /// Remove a root from watching
    #[serde(rename = "remove")]
    Remove { root: PathBuf },
    /// List all watched roots
    #[serde(rename = "list")]
    List,
    /// Get daemon status
    #[serde(rename = "status")]
    Status,
    /// Shutdown daemon
    #[serde(rename = "shutdown")]
    Shutdown,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    fn ok(data: serde_json::Value) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
        }
    }
    fn err(msg: &str) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(msg.to_string()),
        }
    }
}

/// Client for communicating with the global daemon.
pub struct DaemonClient {
    socket_path: PathBuf,
}

impl DaemonClient {
    pub fn new() -> Self {
        Self {
            socket_path: global_socket_path(),
        }
    }

    pub fn is_available(&self) -> bool {
        if !self.socket_path.exists() {
            return false;
        }
        self.send(&Request::Status).is_ok()
    }

    /// Ensure daemon is running, starting it if necessary.
    pub fn ensure_running(&self) -> bool {
        if self.is_available() {
            return true;
        }
        let _ = std::fs::remove_file(&self.socket_path);
        self.start_daemon().is_ok()
    }

    fn start_daemon(&self) -> Result<(), String> {
        use std::process::{Command, Stdio};

        // Ensure config directory exists
        if let Some(parent) = self.socket_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }

        let current_exe =
            std::env::current_exe().map_err(|e| format!("Failed to get executable: {}", e))?;

        Command::new(&current_exe)
            .arg("daemon")
            .arg("run")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to spawn daemon: {}", e))?;

        // Wait for socket
        for _ in 0..20 {
            if self.socket_path.exists() {
                std::thread::sleep(Duration::from_millis(100));
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err("Daemon started but socket not created".to_string())
    }

    pub fn send(&self, request: &Request) -> Result<Response, String> {
        let mut stream = UnixStream::connect(&self.socket_path)
            .map_err(|e| format!("Failed to connect: {}", e))?;

        stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(5))).ok();

        let json = serde_json::to_string(request).map_err(|e| e.to_string())?;
        stream
            .write_all(json.as_bytes())
            .map_err(|e| e.to_string())?;
        stream.write_all(b"\n").map_err(|e| e.to_string())?;

        let mut reader = BufReader::new(&stream);
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| e.to_string())?;

        serde_json::from_str(&line).map_err(|e| e.to_string())
    }

    pub fn add_root(&self, root: &Path) -> Result<Response, String> {
        self.send(&Request::Add {
            root: root.to_path_buf(),
        })
    }

    pub fn remove_root(&self, root: &Path) -> Result<Response, String> {
        self.send(&Request::Remove {
            root: root.to_path_buf(),
        })
    }

    pub fn list_roots(&self) -> Result<Response, String> {
        self.send(&Request::List)
    }

    pub fn status(&self) -> Result<Response, String> {
        self.send(&Request::Status)
    }

    pub fn shutdown(&self) -> Result<(), String> {
        let _ = self.send(&Request::Shutdown);
        Ok(())
    }
}

impl Default for DaemonClient {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Daemon Server
// ============================================================================

/// A watched root with its file watcher.
struct WatchedRoot {
    #[allow(dead_code)] // Watcher must be kept alive
    watcher: RecommendedWatcher,
    last_refresh: Instant,
}

/// Global daemon server managing multiple roots.
struct DaemonServer {
    roots: Mutex<HashMap<PathBuf, WatchedRoot>>,
    refresh_tx: Sender<PathBuf>,
    start_time: Instant,
}

impl DaemonServer {
    fn new(refresh_tx: Sender<PathBuf>) -> Self {
        Self {
            roots: Mutex::new(HashMap::new()),
            refresh_tx,
            start_time: Instant::now(),
        }
    }

    fn add_root(&self, root: PathBuf) -> Response {
        let mut roots = self.roots.lock().unwrap();

        if roots.contains_key(&root) {
            return Response::ok(serde_json::json!({"added": false, "reason": "already watching"}));
        }

        // Check if indexing is enabled for this root
        let config = MossConfig::load(&root);
        if !config.index.enabled() {
            return Response::err("Indexing disabled for this root");
        }

        // Initial index refresh
        match FileIndex::open(&root) {
            Ok(mut idx) => {
                if let Err(e) = idx.refresh() {
                    return Response::err(&format!("Failed to index: {}", e));
                }
                if let Err(e) = idx.incremental_call_graph_refresh() {
                    eprintln!("Warning: call graph refresh failed: {}", e);
                }
            }
            Err(e) => return Response::err(&format!("Failed to open index: {}", e)),
        }

        // Set up file watcher
        let tx = self.refresh_tx.clone();
        let root_clone = root.clone();
        let (notify_tx, notify_rx) = channel();

        let mut watcher = match RecommendedWatcher::new(notify_tx, Config::default()) {
            Ok(w) => w,
            Err(e) => return Response::err(&format!("Failed to create watcher: {}", e)),
        };

        if let Err(e) = watcher.watch(&root, RecursiveMode::Recursive) {
            return Response::err(&format!("Failed to watch: {}", e));
        }

        // Spawn thread to handle file events
        std::thread::spawn(move || {
            let debounce = Duration::from_millis(500);
            let mut last_event = Instant::now();

            for res in notify_rx {
                if let Ok(event) = res {
                    // Skip .moss directory
                    if event
                        .paths
                        .iter()
                        .all(|p| p.to_string_lossy().contains(".moss"))
                    {
                        continue;
                    }

                    if last_event.elapsed() >= debounce {
                        let _ = tx.send(root_clone.clone());
                        last_event = Instant::now();
                    }
                }
            }
        });

        roots.insert(
            root.clone(),
            WatchedRoot {
                watcher,
                last_refresh: Instant::now(),
            },
        );

        Response::ok(serde_json::json!({"added": true, "root": root}))
    }

    fn remove_root(&self, root: &Path) -> Response {
        let mut roots = self.roots.lock().unwrap();
        if roots.remove(root).is_some() {
            Response::ok(serde_json::json!({"removed": true}))
        } else {
            Response::ok(serde_json::json!({"removed": false, "reason": "not watching"}))
        }
    }

    fn list_roots(&self) -> Response {
        let roots = self.roots.lock().unwrap();
        let list: Vec<&PathBuf> = roots.keys().collect();
        Response::ok(serde_json::json!(list))
    }

    fn status(&self) -> Response {
        let roots = self.roots.lock().unwrap();
        Response::ok(serde_json::json!({
            "uptime_secs": self.start_time.elapsed().as_secs(),
            "roots_watched": roots.len(),
            "pid": std::process::id(),
        }))
    }

    fn handle_request(&self, req: Request) -> Response {
        match req {
            Request::Add { root } => self.add_root(root),
            Request::Remove { root } => self.remove_root(&root),
            Request::List => self.list_roots(),
            Request::Status => self.status(),
            Request::Shutdown => Response::ok(serde_json::json!({"message": "shutting down"})),
        }
    }

    fn refresh_root(&self, root: &Path) {
        let mut roots = self.roots.lock().unwrap();
        if let Some(watched) = roots.get_mut(root) {
            match FileIndex::open(root) {
                Ok(mut idx) => {
                    if let Err(e) = idx.incremental_refresh() {
                        eprintln!("Refresh error for {:?}: {}", root, e);
                    }
                    watched.last_refresh = Instant::now();
                }
                Err(e) => eprintln!("Failed to open index for {:?}: {}", root, e),
            }
        }
    }
}

/// Run the global daemon server.
#[tokio::main]
pub async fn run_daemon() -> Result<i32, Box<dyn std::error::Error>> {
    let socket_path = global_socket_path();

    // Ensure config directory exists
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Remove stale socket
    let _ = std::fs::remove_file(&socket_path);

    // Channel for refresh requests from watchers
    let (refresh_tx, refresh_rx) = channel::<PathBuf>();

    let server = Arc::new(DaemonServer::new(refresh_tx));

    // Spawn refresh handler
    let server_refresh = server.clone();
    std::thread::spawn(move || {
        for root in refresh_rx {
            server_refresh.refresh_root(&root);
        }
    });

    // Start socket server
    let listener = UnixListener::bind(&socket_path)?;
    eprintln!("Daemon listening on {}", socket_path.display());

    loop {
        let (stream, _) = listener.accept().await?;
        let server = server.clone();

        tokio::spawn(async move {
            let (reader, mut writer) = stream.into_split();
            let mut reader = tokio::io::BufReader::new(reader);
            let mut line = String::new();

            while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
                let response = match serde_json::from_str::<Request>(&line) {
                    Ok(Request::Shutdown) => {
                        let resp = server.handle_request(Request::Shutdown);
                        let resp_str = serde_json::to_string(&resp).unwrap();
                        let _ = writer.write_all(resp_str.as_bytes()).await;
                        let _ = writer.write_all(b"\n").await;
                        std::process::exit(0);
                    }
                    Ok(req) => server.handle_request(req),
                    Err(e) => Response::err(&format!("Invalid request: {}", e)),
                };

                let resp_str = serde_json::to_string(&response).unwrap();
                let _ = writer.write_all(resp_str.as_bytes()).await;
                let _ = writer.write_all(b"\n").await;
                line.clear();
            }
        });
    }
}

// ============================================================================
// Auto-start helper
// ============================================================================

/// Ensure daemon is running and watching this root.
pub fn maybe_start_daemon(root: &Path) {
    let config = MossConfig::load(root);
    if !config.daemon.enabled() || !config.daemon.auto_start() || !config.index.enabled() {
        return;
    }

    let client = DaemonClient::new();
    if client.ensure_running() {
        // Add this root to the daemon
        let _ = client.add_root(root);
    }
}
