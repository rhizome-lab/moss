use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Serialize)]
#[serde(tag = "cmd")]
pub enum Request {
    #[serde(rename = "path")]
    Path { query: String },
    #[serde(rename = "symbols")]
    Symbols { file: String },
    #[serde(rename = "callers")]
    Callers { symbol: String },
    #[serde(rename = "callees")]
    Callees { symbol: String, file: String },
    #[serde(rename = "expand")]
    Expand { symbol: String, file: Option<String> },
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
        let socket_path = root.join(".moss/daemon.sock").to_string_lossy().to_string();
        let root_path = root.to_path_buf();
        Self { socket_path, root_path }
    }

    pub fn is_available(&self) -> bool {
        Path::new(&self.socket_path).exists()
    }

    /// Ensure daemon is running, starting it if necessary
    /// Returns true if daemon is running (was running or was started)
    pub fn ensure_running(&self) -> bool {
        if self.is_available() {
            // Verify it's actually responding
            if self.query(&Request::Status).is_ok() {
                return true;
            }
            // Socket exists but daemon not responding - clean up stale socket
            let _ = std::fs::remove_file(&self.socket_path);
        }

        // Try to start daemon
        self.start_daemon().is_ok()
    }

    fn start_daemon(&self) -> Result<(), String> {
        use std::process::{Command, Stdio};

        // Create .moss directory if it doesn't exist
        let moss_dir = self.root_path.join(".moss");
        if !moss_dir.exists() {
            std::fs::create_dir_all(&moss_dir)
                .map_err(|e| format!("Failed to create .moss directory: {}", e))?;
        }

        // Try to start moss-server with Unix socket
        let socket_path = moss_dir.join("daemon.sock");

        // Spawn as background process (detached)
        let result = Command::new("moss-server")
            .arg(&self.root_path)
            .arg("--socket")
            .arg(&socket_path)
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
        stream
            .set_read_timeout(Some(Duration::from_secs(120)))
            .ok();
        stream
            .set_write_timeout(Some(Duration::from_secs(10)))
            .ok();

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

        serde_json::from_str(&response_line)
            .map_err(|e| format!("Failed to parse response: {}", e))
    }

    fn read_chunked_response(&self, stream: &mut UnixStream, header: &str) -> Result<Response, String> {
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

        let response_str = String::from_utf8(data)
            .map_err(|e| format!("Invalid UTF-8 in response: {}", e))?;

        serde_json::from_str(&response_str)
            .map_err(|e| format!("Failed to parse response: {}", e))
    }

    pub fn path_query(&self, query: &str) -> Result<Vec<PathMatch>, String> {
        let response = self.query(&Request::Path { query: query.to_string() })?;
        if !response.ok {
            return Err(response.error.unwrap_or_else(|| "Unknown error".to_string()));
        }
        let data = response.data.ok_or("No data in response")?;
        serde_json::from_value(data).map_err(|e| format!("Failed to parse path matches: {}", e))
    }

    pub fn status(&self) -> Result<DaemonStatus, String> {
        let response = self.query(&Request::Status)?;
        if !response.ok {
            return Err(response.error.unwrap_or_else(|| "Unknown error".to_string()));
        }
        let data = response.data.ok_or("No data in response")?;
        serde_json::from_value(data).map_err(|e| format!("Failed to parse status: {}", e))
    }

    pub fn shutdown(&self) -> Result<(), String> {
        let response = self.query(&Request::Shutdown)?;
        if !response.ok {
            return Err(response.error.unwrap_or_else(|| "Unknown error".to_string()));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
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
