"""Moss Server: HTTP/WebSocket server with persistent state.

This module provides a server implementation for Moss that:
- Serves MossAPI over HTTP endpoints
- Supports WebSocket for streaming results
- Supports Unix socket for local high-performance
- Maintains persistent state (parse once, query many)
- Handles multiple concurrent clients

Usage:
    from moss.server import create_app, run_server

    # Create FastAPI app
    app = create_app(root="/path/to/project")

    # Or run server directly
    run_server(root="/path/to/project", port=8000)
"""

from moss.server.app import MossServer, create_app
from moss.server.state import ServerState

__all__ = [
    "MossServer",
    "ServerState",
    "create_app",
]
