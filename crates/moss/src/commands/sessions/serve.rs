//! Web server for session viewing.

use super::{format_age, get_sessions_dir, resolve_session_paths};
use axum::{
    Router,
    extract::{Path as AxumPath, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

struct SessionsState {
    project: Option<PathBuf>,
}

// Embedded SPA assets (gzipped for smaller binary)
const SPA_HTML_GZ: &[u8] = include_bytes!("../../../../../web/sessions/dist/index.html.gz");
const SPA_JS_GZ: &[u8] = include_bytes!("../../../../../web/sessions/dist/app.js.gz");
const SPA_CSS_GZ: &[u8] = include_bytes!("../../../../../web/sessions/dist/index.css.gz");

/// Start the sessions web server.
pub async fn cmd_sessions_serve(project: Option<&Path>, port: u16) -> i32 {
    let state = Arc::new(SessionsState {
        project: project.map(|p| p.to_path_buf()),
    });

    let app = Router::new()
        // SPA static assets
        .route("/", get(spa_index))
        .route("/app.js", get(spa_js))
        .route("/index.css", get(spa_css))
        // API endpoints
        .route("/api/sessions", get(api_sessions))
        .route("/api/session/{id}", get(api_session))
        .with_state(state);

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    eprintln!("Sessions viewer at http://{}", addr);

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

async fn spa_index() -> impl IntoResponse {
    (
        [
            (axum::http::header::CONTENT_TYPE, "text/html"),
            (axum::http::header::CONTENT_ENCODING, "gzip"),
        ],
        SPA_HTML_GZ,
    )
}

async fn spa_js() -> impl IntoResponse {
    (
        [
            (axum::http::header::CONTENT_TYPE, "application/javascript"),
            (axum::http::header::CONTENT_ENCODING, "gzip"),
        ],
        SPA_JS_GZ,
    )
}

async fn spa_css() -> impl IntoResponse {
    (
        [
            (axum::http::header::CONTENT_TYPE, "text/css"),
            (axum::http::header::CONTENT_ENCODING, "gzip"),
        ],
        SPA_CSS_GZ,
    )
}

/// API: list all sessions as JSON.
async fn api_sessions(State(state): State<Arc<SessionsState>>) -> axum::response::Response {
    let sessions_dir = get_sessions_dir(state.project.as_deref());

    let mut sessions: Vec<serde_json::Value> = Vec::new();

    if let Some(dir) = &sessions_dir
        && let Ok(entries) = std::fs::read_dir(dir)
    {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("jsonl")
                && let Ok(meta) = path.metadata()
                && let Ok(mtime) = meta.modified()
            {
                let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                let format = crate::sessions::FormatRegistry::new()
                    .detect(&path)
                    .map(|f| f.name().to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                let age = format_age(mtime.elapsed().map(|d| d.as_secs()).unwrap_or(0));
                sessions.push(serde_json::json!({
                    "id": id,
                    "format": format,
                    "age": age,
                }));
            }
        }
    }

    // Sort by id (sessions are UUIDs, so this approximates time ordering)
    // TODO: sort by mtime properly
    sessions.reverse();
    sessions.truncate(100);

    let json = serde_json::to_string(&sessions).unwrap_or_else(|_| "[]".to_string());

    (
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        json,
    )
        .into_response()
}

/// API: return raw session log as JSON array.
async fn api_session(
    State(state): State<Arc<SessionsState>>,
    AxumPath(id): AxumPath<String>,
) -> Result<axum::response::Response, StatusCode> {
    let paths = resolve_session_paths(&id, state.project.as_deref(), None);
    let path = paths.first().ok_or(StatusCode::NOT_FOUND)?;

    let content = std::fs::read_to_string(path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let entries: Vec<serde_json::Value> = content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();

    let json = serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_string());

    Ok((
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        json,
    )
        .into_response())
}
