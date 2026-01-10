//! HTTP REST API server for moss.
//!
//! Exposes moss functionality over HTTP for integration with other tools.

use crate::index::FileIndex;
use crate::skeleton::SkeletonExtractor;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use utoipa::{OpenApi, ToSchema};

/// OpenAPI documentation
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Moss API",
        version = "0.1.0",
        description = "Code intelligence API for moss"
    ),
    paths(health, list_files, get_file, list_symbols, get_symbol, search),
    components(schemas(
        HealthResponse,
        FileListResponse,
        FileInfoResponse,
        SymbolInfo,
        SymbolListResponse,
        IndexedSymbol,
        SearchResponse,
        SearchResult,
        SymbolDetailResponse
    ))
)]
pub struct ApiDoc;

/// Shared server state.
struct AppState {
    root: std::path::PathBuf,
    index: Mutex<FileIndex>,
}

/// Start the HTTP server.
pub async fn run_http_server(root: &std::path::Path, port: u16) -> i32 {
    // Initialize index
    let index = match FileIndex::open_if_enabled(root).await {
        Some(idx) => idx,
        None => {
            eprintln!("Indexing disabled or failed");
            return 1;
        }
    };

    let state = Arc::new(AppState {
        root: root.to_path_buf(),
        index: Mutex::new(index),
    });

    // Build routes
    let app = Router::new()
        .route("/openapi.json", get(openapi_spec))
        .route("/health", get(health))
        .route("/files", get(list_files))
        .route("/files/*path", get(get_file))
        .route("/symbols", get(list_symbols))
        .route("/symbols/:name", get(get_symbol))
        .route("/search", get(search))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    eprintln!("HTTP server listening on http://{}", addr);
    eprintln!("OpenAPI spec available at http://{}/openapi.json", addr);

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

/// Serve OpenAPI spec as JSON
async fn openapi_spec() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

/// Health check response.
#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    /// Server status
    status: &'static str,
    /// Number of files in the index
    files_indexed: usize,
}

/// Check server health
#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "Server is healthy", body = HealthResponse)
    ),
    tag = "health"
)]
async fn health(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    let files_indexed = state.index.lock().await.count().await.unwrap_or(0);
    Json(HealthResponse {
        status: "ok",
        files_indexed,
    })
}

/// File list query parameters.
#[derive(Deserialize, utoipa::IntoParams)]
struct FileListQuery {
    /// Glob pattern to filter files
    pattern: Option<String>,
    /// Maximum number of results
    limit: Option<usize>,
}

/// File list response.
#[derive(Serialize, ToSchema)]
pub struct FileListResponse {
    /// List of file paths
    files: Vec<String>,
}

/// List indexed files
#[utoipa::path(
    get,
    path = "/files",
    params(FileListQuery),
    responses(
        (status = 200, description = "List of files", body = FileListResponse)
    ),
    tag = "files"
)]
async fn list_files(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FileListQuery>,
) -> Json<FileListResponse> {
    let pattern = query.pattern.as_deref().unwrap_or("");
    let limit = query.limit.unwrap_or(100);

    let files = state
        .index
        .lock()
        .await
        .find_like(pattern)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|f| f.path)
        .take(limit)
        .collect();

    Json(FileListResponse { files })
}

/// File info response.
#[derive(Serialize, ToSchema)]
pub struct FileInfoResponse {
    /// File path
    path: String,
    /// Symbols defined in the file
    symbols: Vec<SymbolInfo>,
}

/// Symbol info for file response.
#[derive(Serialize, ToSchema)]
pub struct SymbolInfo {
    /// Symbol name
    name: String,
    /// Symbol kind (function, class, etc.)
    kind: String,
    /// Line number
    line: usize,
}

/// Get file information and symbols
#[utoipa::path(
    get,
    path = "/files/{path}",
    params(
        ("path" = String, Path, description = "File path relative to root")
    ),
    responses(
        (status = 200, description = "File info with symbols", body = FileInfoResponse),
        (status = 404, description = "File not found")
    ),
    tag = "files"
)]
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

    let extractor = SkeletonExtractor::new();
    let result = extractor.extract(&file_path, &content);

    let symbols: Vec<SymbolInfo> = result
        .symbols
        .iter()
        .map(|s| SymbolInfo {
            name: s.name.clone(),
            kind: s.kind.as_str().to_string(),
            line: s.start_line,
        })
        .collect();

    Ok(Json(FileInfoResponse {
        path: path.clone(),
        symbols,
    }))
}

/// Symbol search query.
#[derive(Deserialize, utoipa::IntoParams)]
struct SymbolQuery {
    /// Symbol name pattern
    name: Option<String>,
    /// Filter by kind (function, class, etc.)
    kind: Option<String>,
    /// Maximum number of results
    limit: Option<usize>,
}

/// Symbol search response.
#[derive(Serialize, ToSchema)]
pub struct SymbolListResponse {
    /// List of matching symbols
    symbols: Vec<IndexedSymbol>,
}

/// Indexed symbol info
#[derive(Serialize, ToSchema)]
pub struct IndexedSymbol {
    /// Symbol name
    name: String,
    /// Symbol kind
    kind: String,
    /// File containing the symbol
    file: String,
    /// Line number
    line: usize,
}

/// List symbols from index
#[utoipa::path(
    get,
    path = "/symbols",
    params(SymbolQuery),
    responses(
        (status = 200, description = "List of symbols", body = SymbolListResponse)
    ),
    tag = "symbols"
)]
async fn list_symbols(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SymbolQuery>,
) -> Json<SymbolListResponse> {
    let name = query.name.as_deref().unwrap_or("");
    let limit = query.limit.unwrap_or(100);

    let symbols = state
        .index
        .lock()
        .await
        .find_symbols(name, query.kind.as_deref(), false, limit)
        .await
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

/// Symbol detail response
#[derive(Serialize, ToSchema)]
pub struct SymbolDetailResponse {
    /// Symbol name
    name: String,
    /// File containing the symbol
    file: String,
    /// Start line
    start_line: usize,
    /// End line
    end_line: usize,
    /// Source code
    source: String,
}

/// Get symbol details and source code
#[utoipa::path(
    get,
    path = "/symbols/{name}",
    params(
        ("name" = String, Path, description = "Symbol name")
    ),
    responses(
        (status = 200, description = "Symbol details with source", body = SymbolDetailResponse),
        (status = 404, description = "Symbol not found")
    ),
    tag = "symbols"
)]
async fn get_symbol(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<SymbolDetailResponse>, StatusCode> {
    let matches = state
        .index
        .lock()
        .await
        .find_symbol(&name)
        .await
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

    Ok(Json(SymbolDetailResponse {
        name,
        file: file.clone(),
        start_line: *start,
        end_line: *end,
        source,
    }))
}

/// Generic search query.
#[derive(Deserialize, utoipa::IntoParams)]
struct SearchQuery {
    /// Search query string
    q: String,
    /// Search type: "file", "symbol", or "all"
    #[serde(rename = "type")]
    #[param(rename = "type")]
    search_type: Option<String>,
    /// Maximum number of results
    limit: Option<usize>,
}

/// Search response.
#[derive(Serialize, ToSchema)]
pub struct SearchResponse {
    /// Search results
    results: Vec<SearchResult>,
}

/// Individual search result
#[derive(Serialize, ToSchema)]
pub struct SearchResult {
    /// File path
    path: String,
    /// Result kind (file or symbol kind)
    kind: String,
    /// Symbol name (if symbol result)
    name: Option<String>,
    /// Line number (if symbol result)
    line: Option<usize>,
}

/// Search files and symbols
#[utoipa::path(
    get,
    path = "/search",
    params(SearchQuery),
    responses(
        (status = 200, description = "Search results", body = SearchResponse)
    ),
    tag = "search"
)]
async fn search(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Json<SearchResponse> {
    let search_type = query.search_type.as_deref().unwrap_or("all");
    let limit = query.limit.unwrap_or(20);
    let mut results = Vec::new();

    let index = state.index.lock().await;

    if search_type == "all" || search_type == "file" {
        // Search files
        if let Ok(files) = index.find_like(&query.q).await {
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
        if let Ok(symbols) = index.find_symbols(&query.q, None, false, limit).await {
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
