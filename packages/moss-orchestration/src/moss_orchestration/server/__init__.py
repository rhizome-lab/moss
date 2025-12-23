"""Moss Server: HTTP/WebSocket server with persistent state.

This module provides a server implementation for Moss that:
- Serves MossAPI over HTTP endpoints (generated from library introspection)
- Supports WebSocket for streaming results
- Supports Unix socket for local high-performance
- Maintains persistent state (parse once, query many)
- Handles multiple concurrent clients

Two server modes:
- HTTP server (app.py): Full FastAPI server with REST/WebSocket endpoints
- Line-based daemon (daemon.py): Simple Unix socket IPC for fast Rust CLI

Usage:
    from moss_orchestration.server import create_app, run_server

    # Create FastAPI app
    app = create_app(root="/path/to/project")

    # Or run server directly
    run_server(root="/path/to/project", port=8000)

    # Or run lightweight daemon
    from moss_orchestration.server import run_daemon
    asyncio.run(run_daemon(root="/path/to/project"))
"""

from moss_orchestration.server.app import create_app, run_server
from moss_orchestration.server.daemon import run_daemon
from moss_orchestration.server.state import ServerState

__all__ = [
    "ServerState",
    "create_app",
    "run_daemon",
    "run_server",
]
