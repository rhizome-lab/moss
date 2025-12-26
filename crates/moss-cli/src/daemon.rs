use crate::config::MossConfig;
use crate::paths::get_moss_dir;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::UnixListener;

use crate::index::FileIndex;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "cmd")]
pub enum Request {
    #[serde(rename = "path")]
    Path { query: String },
    #[serde(rename = "file_name")]
    FileName { name: String },
    #[serde(rename = "file_stem")]
    FileStem { stem: String },
    #[serde(rename = "symbols")]
    Symbols { file: String },
    #[serde(rename = "callers")]
    Callers { symbol: String },
    #[serde(rename = "callees")]
    Callees { symbol: String, file: String },
    #[serde(rename = "callees_resolved")]
    CalleesResolved { symbol: String, file: String },
    #[serde(rename = "expand")]
    Expand {
        symbol: String,
        file: Option<String>,
    },
    #[serde(rename = "importers")]
    Importers { module: String },
    #[serde(rename = "resolve_import")]
    ResolveImport { file: String, name: String },
    #[serde(rename = "cross_refs")]
    CrossRefs { file: String },
    #[serde(rename = "cross_ref_sources")]
    CrossRefSources { target: String },
    #[serde(rename = "all_cross_refs")]
    AllCrossRefs,
    #[serde(rename = "status")]
    Status,
    #[serde(rename = "shutdown")]
    Shutdown,
}

#[derive(Debug, Deserialize)]
pub struct Response {
    pub ok: bool,
    pub data: Option<serde_json::Value>,
    pub error: Option<String>,
}

pub struct DaemonClient {
    socket_path: String,
    root_path: std::path::PathBuf,
}

impl DaemonClient {
    pub fn new(root: &Path) -> Self {
        let moss_dir = get_moss_dir(root);
        let socket_path = moss_dir.join("daemon.sock").to_string_lossy().to_string();
        let root_path = root.to_path_buf();
        Self {
            socket_path,
            root_path,
        }
    }

    pub fn is_available(&self) -> bool {
        if !Path::new(&self.socket_path).exists() {
            return false;
        }
        // Socket exists - verify daemon is actually responding
        self.query(&Request::Status).is_ok()
    }

    /// Ensure daemon is running, starting it if necessary.
    /// Returns true if daemon is running (was running or was started).
    pub fn ensure_running(&self) -> bool {
        if self.is_available() {
            return true;
        }
        // Clean up stale socket if it exists but daemon isn't responding
        let _ = std::fs::remove_file(&self.socket_path);
        // Try to start daemon
        self.start_daemon().is_ok()
    }

    fn start_daemon(&self) -> Result<(), String> {
        use std::process::{Command, Stdio};

        // Create moss data directory if it doesn't exist
        let moss_dir = get_moss_dir(&self.root_path);
        if !moss_dir.exists() {
            std::fs::create_dir_all(&moss_dir)
                .map_err(|e| format!("Failed to create moss directory: {}", e))?;
        }

        let socket_path = moss_dir.join("daemon.sock");

        // Get current executable to spawn daemon subprocess
        let current_exe = std::env::current_exe()
            .map_err(|e| format!("Failed to get current executable: {}", e))?;

        // Spawn as background process (detached)
        let result = Command::new(&current_exe)
            .arg("daemon")
            .arg("run")
            .arg("--root")
            .arg(&self.root_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();

        match result {
            Ok(_) => {
                // Wait for socket to appear (up to 2 seconds)
                for _ in 0..20 {
                    if socket_path.exists() {
                        // Give it a moment to bind
                        std::thread::sleep(Duration::from_millis(100));
                        return Ok(());
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err("Daemon started but socket not created".to_string())
            }
            Err(e) => Err(format!("Failed to spawn daemon: {}", e)),
        }
    }

    pub fn query(&self, request: &Request) -> Result<Response, String> {
        let mut stream = UnixStream::connect(&self.socket_path)
            .map_err(|e| format!("Failed to connect to daemon: {}", e))?;

        // Use reasonable per-operation timeouts - these reset on each read/write
        // For truly long operations, chunked responses handle progress
        stream.set_read_timeout(Some(Duration::from_secs(120))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(10))).ok();

        let request_json = serde_json::to_string(request)
            .map_err(|e| format!("Failed to serialize request: {}", e))?;

        stream
            .write_all(request_json.as_bytes())
            .map_err(|e| format!("Failed to send request: {}", e))?;
        stream
            .write_all(b"\n")
            .map_err(|e| format!("Failed to send newline: {}", e))?;

        let mut reader = BufReader::new(&stream);
        let mut response_line = String::new();
        reader
            .read_line(&mut response_line)
            .map_err(|e| format!("Failed to read response: {}", e))?;

        // Check if response indicates chunked transfer
        if response_line.contains("\"chunked\":true") {
            // Need mutable access to underlying stream for timeout adjustment
            drop(reader);
            let mut stream = stream;
            return self.read_chunked_response(&mut stream, &response_line);
        }

        serde_json::from_str(&response_line).map_err(|e| format!("Failed to parse response: {}", e))
    }

    fn read_chunked_response(
        &self,
        stream: &mut UnixStream,
        header: &str,
    ) -> Result<Response, String> {
        // Parse header to get total size
        #[derive(Deserialize)]
        struct ChunkedHeader {
            chunked: bool,
            total_size: usize,
        }

        let header_info: ChunkedHeader = serde_json::from_str(header)
            .map_err(|e| format!("Failed to parse chunked header: {}", e))?;

        if !header_info.chunked {
            return Err("Invalid chunked header".to_string());
        }

        // For chunked transfer, use longer per-chunk timeout since data is streaming
        // Each chunk proves the daemon is alive, so we just need reasonable timeout
        // between chunks (5 minutes should handle even slow analysis)
        stream.set_read_timeout(Some(Duration::from_secs(300))).ok();

        // Read length-prefixed chunks until we get all data
        let mut data = Vec::with_capacity(header_info.total_size);
        let mut length_buf = [0u8; 4];

        loop {
            stream
                .read_exact(&mut length_buf)
                .map_err(|e| format!("Failed to read chunk length: {}", e))?;

            let chunk_len = u32::from_be_bytes(length_buf) as usize;
            if chunk_len == 0 {
                break; // End of chunks
            }

            let mut chunk = vec![0u8; chunk_len];
            stream
                .read_exact(&mut chunk)
                .map_err(|e| format!("Failed to read chunk data: {}", e))?;

            data.extend_from_slice(&chunk);
        }

        let response_str =
            String::from_utf8(data).map_err(|e| format!("Invalid UTF-8 in response: {}", e))?;

        serde_json::from_str(&response_str).map_err(|e| format!("Failed to parse response: {}", e))
    }

    pub fn status(&self) -> Result<DaemonStatus, String> {
        let response = self.query(&Request::Status)?;
        if !response.ok {
            return Err(response
                .error
                .unwrap_or_else(|| "Unknown error".to_string()));
        }
        let data = response.data.ok_or("No data in response")?;
        serde_json::from_value(data).map_err(|e| format!("Failed to parse status: {}", e))
    }

    pub fn shutdown(&self) -> Result<(), String> {
        let response = self.query(&Request::Shutdown)?;
        if !response.ok {
            return Err(response
                .error
                .unwrap_or_else(|| "Unknown error".to_string()));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)] // Fields used by serde deserialization
pub struct PathMatch {
    pub path: String,
    pub kind: String,
    pub score: i32,
}

#[derive(Debug, Deserialize)]
pub struct DaemonStatus {
    pub uptime_secs: u64,
    pub files_indexed: usize,
    pub symbols_indexed: usize,
    pub queries_served: usize,
    pub pid: Option<u32>,
}

// ============================================================================
// Daemon Server Implementation
// ============================================================================

#[derive(Debug, Serialize)]
struct ServerResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl ServerResponse {
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

struct DaemonServer {
    root: PathBuf,
    index: Mutex<FileIndex>,
    start_time: std::time::Instant,
    query_count: std::sync::atomic::AtomicUsize,
}

impl DaemonServer {
    fn new(root: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let index = FileIndex::open(&root)?;
        Ok(Self {
            root,
            index: Mutex::new(index),
            start_time: std::time::Instant::now(),
            query_count: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    fn handle_request(&self, req: Request) -> ServerResponse {
        use std::sync::atomic::Ordering;

        // Track query count (Status and Shutdown don't count as queries)
        let is_query = !matches!(req, Request::Status | Request::Shutdown);
        if is_query {
            self.query_count.fetch_add(1, Ordering::Relaxed);
        }

        match req {
            Request::Status => {
                let idx = self.index.lock().unwrap();
                let stats = idx.call_graph_stats().unwrap_or_default();
                ServerResponse::ok(serde_json::json!({
                    "uptime_secs": self.start_time.elapsed().as_secs(),
                    "files_indexed": idx.count().unwrap_or(0),
                    "symbols_indexed": stats.symbols,
                    "queries_served": self.query_count.load(Ordering::Relaxed),
                    "pid": std::process::id(),
                }))
            }
            Request::Path { query } => {
                let idx = self.index.lock().unwrap();
                match idx.find_like(&query) {
                    Ok(matches) => ServerResponse::ok(serde_json::json!(matches)),
                    Err(e) => ServerResponse::err(&e.to_string()),
                }
            }
            Request::FileName { name } => {
                let idx = self.index.lock().unwrap();
                match idx.find_by_name(&name) {
                    Ok(matches) => ServerResponse::ok(serde_json::json!(matches)),
                    Err(e) => ServerResponse::err(&e.to_string()),
                }
            }
            Request::FileStem { stem } => {
                let idx = self.index.lock().unwrap();
                match idx.find_by_stem(&stem) {
                    Ok(matches) => ServerResponse::ok(serde_json::json!(matches)),
                    Err(e) => ServerResponse::err(&e.to_string()),
                }
            }
            Request::Symbols { file } => {
                let idx = self.index.lock().unwrap();
                match idx.find_symbols(&file, None, false, 100) {
                    Ok(syms) => ServerResponse::ok(serde_json::json!(syms)),
                    Err(e) => ServerResponse::err(&e.to_string()),
                }
            }
            Request::Callers { symbol } => {
                let idx = self.index.lock().unwrap();
                match idx.find_callers(&symbol) {
                    Ok(callers) => ServerResponse::ok(serde_json::json!(callers)),
                    Err(e) => ServerResponse::err(&e.to_string()),
                }
            }
            Request::Callees { symbol, file } => {
                let idx = self.index.lock().unwrap();
                match idx.find_callees(&symbol, &file) {
                    Ok(callees) => ServerResponse::ok(serde_json::json!(callees)),
                    Err(e) => ServerResponse::err(&e.to_string()),
                }
            }
            Request::CalleesResolved { symbol, file } => {
                let idx = self.index.lock().unwrap();
                match idx.find_callees_resolved(&file, &symbol) {
                    Ok(callees) => {
                        // Convert to JSON-friendly format
                        let results: Vec<serde_json::Value> = callees
                            .into_iter()
                            .map(|(name, line, source)| {
                                serde_json::json!({
                                    "name": name,
                                    "line": line,
                                    "source": source.map(|(module, orig)| {
                                        serde_json::json!({"module": module, "original_name": orig})
                                    })
                                })
                            })
                            .collect();
                        ServerResponse::ok(serde_json::json!(results))
                    }
                    Err(e) => ServerResponse::err(&e.to_string()),
                }
            }
            Request::Expand { symbol, file: _ } => {
                let idx = self.index.lock().unwrap();
                match idx.find_symbol(&symbol) {
                    Ok(matches) if !matches.is_empty() => {
                        let (file_path, _, start, end) = &matches[0];
                        let abs_path = self.root.join(file_path);
                        match std::fs::read_to_string(&abs_path) {
                            Ok(content) => {
                                let lines: Vec<&str> = content.lines().collect();
                                let start_idx = (*start).saturating_sub(1);
                                let end_idx = (*end).min(lines.len());
                                let source = lines[start_idx..end_idx].join("\n");
                                ServerResponse::ok(serde_json::json!({"source": source}))
                            }
                            Err(e) => ServerResponse::err(&e.to_string()),
                        }
                    }
                    Ok(_) => ServerResponse::err("Symbol not found"),
                    Err(e) => ServerResponse::err(&e.to_string()),
                }
            }
            Request::Importers { module } => {
                let idx = self.index.lock().unwrap();
                match idx.find_importers(&module) {
                    Ok(importers) => {
                        let results: Vec<serde_json::Value> = importers
                            .into_iter()
                            .map(|(file, name, line)| {
                                serde_json::json!({"file": file, "name": name, "line": line})
                            })
                            .collect();
                        ServerResponse::ok(serde_json::json!(results))
                    }
                    Err(e) => ServerResponse::err(&e.to_string()),
                }
            }
            Request::ResolveImport { file, name } => {
                let idx = self.index.lock().unwrap();
                match idx.resolve_import(&file, &name) {
                    Ok(Some((module, orig_name))) => ServerResponse::ok(serde_json::json!({
                        "module": module,
                        "original_name": orig_name
                    })),
                    Ok(None) => ServerResponse::ok(serde_json::json!(null)),
                    Err(e) => ServerResponse::err(&e.to_string()),
                }
            }
            Request::CrossRefs { file } => {
                let idx = self.index.lock().unwrap();
                match idx.find_cross_refs(&file) {
                    Ok(refs) => ServerResponse::ok(serde_json::json!(refs)),
                    Err(e) => ServerResponse::err(&e.to_string()),
                }
            }
            Request::CrossRefSources { target } => {
                let idx = self.index.lock().unwrap();
                match idx.find_cross_ref_sources(&target) {
                    Ok(refs) => ServerResponse::ok(serde_json::json!(refs)),
                    Err(e) => ServerResponse::err(&e.to_string()),
                }
            }
            Request::AllCrossRefs => {
                let idx = self.index.lock().unwrap();
                match idx.all_cross_refs() {
                    Ok(refs) => ServerResponse::ok(serde_json::json!(refs)),
                    Err(e) => ServerResponse::err(&e.to_string()),
                }
            }
            Request::Shutdown => {
                ServerResponse::ok(serde_json::json!({"message": "shutting down"}))
            }
        }
    }

    fn trigger_incremental_refresh(&self) {
        if let Ok(mut idx) = self.index.lock() {
            // Incremental file refresh
            if let Err(e) = idx.incremental_refresh() {
                eprintln!("Error during incremental refresh: {}", e);
            }
        }
    }
}

/// Run the daemon server in the foreground
#[tokio::main]
pub async fn run_daemon(root: &Path) -> Result<i32, Box<dyn std::error::Error>> {
    let moss_dir = get_moss_dir(root);
    let socket_path = moss_dir.join("daemon.sock");

    // Ensure moss data directory exists
    std::fs::create_dir_all(&moss_dir)?;

    // Remove stale socket
    let _ = std::fs::remove_file(&socket_path);

    let server = Arc::new(DaemonServer::new(root.to_path_buf())?);

    // Initial index
    {
        let mut idx = server.index.lock().unwrap();
        let file_count = idx.refresh()?;
        let stats = idx.incremental_call_graph_refresh()?;
        let cross_ref_count = idx.refresh_cross_refs().unwrap_or(0);
        eprintln!(
            "Indexed {} files, {} symbols, {} calls, {} cross-refs",
            file_count, stats.symbols, stats.calls, cross_ref_count
        );
    }

    // Start file watcher - triggers incremental refresh on changes
    let server_watcher = server.clone();
    let root_watcher = root.to_path_buf();
    std::thread::spawn(move || {
        let (tx, rx) = channel();
        let mut watcher = match RecommendedWatcher::new(tx, Config::default()) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("Failed to create file watcher: {}", e);
                return;
            }
        };
        if let Err(e) = watcher.watch(&root_watcher, RecursiveMode::Recursive) {
            eprintln!("Failed to watch directory: {}", e);
            return;
        }

        // Batch file changes - don't reindex on every keystroke
        use std::time::Instant;
        let mut last_refresh = Instant::now();
        let debounce = Duration::from_millis(500);

        for res in rx {
            if let Ok(event) = res {
                // Skip .moss directory
                let dominated_by_moss = event
                    .paths
                    .iter()
                    .all(|p| p.to_string_lossy().contains(".moss"));
                if dominated_by_moss {
                    continue;
                }

                // Debounce: only refresh if enough time has passed
                if last_refresh.elapsed() >= debounce {
                    server_watcher.trigger_incremental_refresh();
                    last_refresh = Instant::now();
                }
            }
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
                    Err(e) => ServerResponse::err(&format!("Invalid request: {}", e)),
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

/// Ensure daemon is running based on config.
///
/// Loads config from global (~/.config/moss/config.toml) and project (.moss/config.toml),
/// then starts daemon if auto_start is enabled.
///
/// This is a no-op if daemon is disabled or already running.
pub fn maybe_start_daemon(root: &Path) {
    let config = MossConfig::load(root);
    if !config.daemon.enabled || !config.daemon.auto_start {
        return;
    }

    let client = DaemonClient::new(root);
    if !client.is_available() {
        let _ = client.ensure_running();
    }
}
