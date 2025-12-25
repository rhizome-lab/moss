//! HTTP REST API server for moss.
//!
//! Exposes moss functionality over HTTP for integration with other tools.

use crate::index::FileIndex;
use crate::skeleton::SkeletonExtractor;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

/// Shared server state.
struct AppState {
    root: std::path::PathBuf,
    index: Mutex<FileIndex>,
}

/// Start the HTTP server.
pub async fn run_http_server(root: &std::path::Path, port: u16) -> i32 {
    // Initialize index
    let index = match FileIndex::open(root) {
        Ok(idx) => idx,
        Err(e) => {
            eprintln!("Failed to open index: {}", e);
            return 1;
        }
    };

    let state = Arc::new(AppState {
        root: root.to_path_buf(),
        index: Mutex::new(index),
    });

    // Build routes
    let app = Router::new()
        .route("/health", get(health))
        .route("/files", get(list_files))
        .route("/files/*path", get(get_file))
        .route("/symbols", get(list_symbols))
        .route("/symbols/:name", get(get_symbol))
        .route("/search", get(search))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    eprintln!("HTTP server listening on http://{}", addr);

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Failed to bind to port {}: {}", port, e);
            return 1;
        }
    };

    if let Err(e) = axum::serve(listener, app).await {
        eprintln!("Server error: {}", e);
        return 1;
    }

    0
}

/// Health check response.
#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    files_indexed: usize,
}

async fn health(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    let files_indexed = state
        .index
        .lock()
        .map(|i| i.count().unwrap_or(0))
        .unwrap_or(0);
    Json(HealthResponse {
        status: "ok",
        files_indexed,
    })
}

/// File list query parameters.
#[derive(Deserialize)]
struct FileListQuery {
    pattern: Option<String>,
    limit: Option<usize>,
}

/// File list response.
#[derive(Serialize)]
struct FileListResponse {
    files: Vec<String>,
}

async fn list_files(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FileListQuery>,
) -> Json<FileListResponse> {
    let files = state
        .index
        .lock()
        .ok()
        .and_then(|i| {
            let pattern = query.pattern.as_deref().unwrap_or("");
            i.find_like(pattern).ok()
        })
        .unwrap_or_default()
        .into_iter()
        .map(|f| f.path)
        .take(query.limit.unwrap_or(100))
        .collect();

    Json(FileListResponse { files })
}

/// File info response.
#[derive(Serialize)]
struct FileInfoResponse {
    path: String,
    symbols: Vec<SymbolInfo>,
}

/// Symbol info for file response.
#[derive(Serialize)]
struct SymbolInfo {
    name: String,
    kind: String,
    line: usize,
}

async fn get_file(
    State(state): State<Arc<AppState>>,
    Path(path): Path<String>,
) -> Result<Json<FileInfoResponse>, StatusCode> {
    let file_path = state.root.join(&path);
    if !file_path.exists() {
        return Err(StatusCode::NOT_FOUND);
    }

    let content =
        std::fs::read_to_string(&file_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut extractor = SkeletonExtractor::new();
    let result = extractor.extract(&file_path, &content);

    let symbols: Vec<SymbolInfo> = result
        .symbols
        .iter()
        .map(|s| SymbolInfo {
            name: s.name.clone(),
            kind: s.kind.to_string(),
            line: s.start_line,
        })
        .collect();

    Ok(Json(FileInfoResponse {
        path: path.clone(),
        symbols,
    }))
}

/// Symbol search query.
#[derive(Deserialize)]
struct SymbolQuery {
    name: Option<String>,
    kind: Option<String>,
    limit: Option<usize>,
}

/// Symbol search response.
#[derive(Serialize)]
struct SymbolListResponse {
    symbols: Vec<IndexedSymbol>,
}

#[derive(Serialize)]
struct IndexedSymbol {
    name: String,
    kind: String,
    file: String,
    line: usize,
}

async fn list_symbols(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SymbolQuery>,
) -> Json<SymbolListResponse> {
    let symbols = state
        .index
        .lock()
        .ok()
        .and_then(|i| {
            let name = query.name.as_deref().unwrap_or("");
            i.find_symbols(
                name,
                query.kind.as_deref(),
                false,
                query.limit.unwrap_or(100),
            )
            .ok()
        })
        .unwrap_or_default()
        .into_iter()
        .map(|s| IndexedSymbol {
            name: s.name,
            kind: s.kind,
            file: s.file,
            line: s.start_line,
        })
        .collect();

    Json(SymbolListResponse { symbols })
}

async fn get_symbol(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let matches = state
        .index
        .lock()
        .ok()
        .and_then(|i| i.find_symbol(&name).ok())
        .unwrap_or_default();

    if matches.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }

    // Return the first match with its source code
    let (file, _kind, start, end) = &matches[0];
    let abs_path = state.root.join(file);
    let content =
        std::fs::read_to_string(&abs_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let lines: Vec<&str> = content.lines().collect();
    let start_idx = (*start).saturating_sub(1);
    let end_idx = (*end).min(lines.len());
    let source = lines[start_idx..end_idx].join("\n");

    Ok(Json(serde_json::json!({
        "name": name,
        "file": file,
        "start_line": start,
        "end_line": end,
        "source": source,
    })))
}

/// Generic search query.
#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    #[serde(rename = "type")]
    search_type: Option<String>,
    limit: Option<usize>,
}

/// Search response.
#[derive(Serialize)]
struct SearchResponse {
    results: Vec<SearchResult>,
}

#[derive(Serialize)]
struct SearchResult {
    path: String,
    kind: String,
    name: Option<String>,
    line: Option<usize>,
}

async fn search(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Json<SearchResponse> {
    let search_type = query.search_type.as_deref().unwrap_or("all");
    let limit = query.limit.unwrap_or(20);
    let mut results = Vec::new();

    if search_type == "all" || search_type == "file" {
        // Search files
        if let Ok(files) = state.index.lock().unwrap().find_like(&query.q) {
            for file in files.into_iter().take(limit) {
                results.push(SearchResult {
                    path: file.path,
                    kind: "file".to_string(),
                    name: None,
                    line: None,
                });
            }
        }
    }

    if search_type == "all" || search_type == "symbol" {
        // Search symbols
        if let Ok(symbols) = state
            .index
            .lock()
            .unwrap()
            .find_symbols(&query.q, None, false, limit)
        {
            for sym in symbols {
                results.push(SearchResult {
                    path: sym.file,
                    kind: sym.kind,
                    name: Some(sym.name),
                    line: Some(sym.start_line),
                });
            }
        }
    }

    Json(SearchResponse { results })
}
