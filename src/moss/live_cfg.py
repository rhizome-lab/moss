"""Live CFG rendering with automatic updates.

This module provides real-time CFG visualization that updates automatically
when source files change.

Usage:
    # Start live CFG viewer for a Python file
    moss cfg --live src/myfile.py

    # Or programmatically
    from moss.live_cfg import LiveCFGServer
    server = LiveCFGServer(path)
    server.start()
"""

from __future__ import annotations

import json
import logging
import threading
import time
from dataclasses import dataclass, field
from http.server import HTTPServer, SimpleHTTPRequestHandler
from pathlib import Path
from typing import TYPE_CHECKING

from moss.cfg import CFGBuilder

if TYPE_CHECKING:
    pass

logger = logging.getLogger(__name__)


# =============================================================================
# Configuration
# =============================================================================


@dataclass
class LiveCFGConfig:
    """Configuration for live CFG server."""

    host: str = "127.0.0.1"
    port: int = 8765
    auto_open: bool = True
    debounce_ms: int = 500  # Debounce file changes


# =============================================================================
# CFG State Manager
# =============================================================================


@dataclass
class CFGState:
    """Current state of CFG analysis."""

    path: Path
    function_name: str | None = None
    mermaid: str = ""
    cfgs: list[dict] = field(default_factory=list)
    last_updated: float = 0
    error: str | None = None

    def update(self) -> None:
        """Update CFG from source file."""
        try:
            source = self.path.read_text()
            builder = CFGBuilder()
            cfgs = builder.build_from_source(source, self.function_name)

            self.cfgs = []
            mermaid_parts = []

            for cfg in cfgs:
                self.cfgs.append(
                    {
                        "name": cfg.name,
                        "node_count": cfg.node_count,
                        "edge_count": cfg.edge_count,
                        "complexity": cfg.cyclomatic_complexity,
                    }
                )
                mermaid_parts.append(cfg.to_mermaid())

            self.mermaid = "\n\n".join(mermaid_parts)
            self.error = None
            self.last_updated = time.time()

        except Exception as e:
            self.error = str(e)
            logger.error(f"CFG update failed: {e}")

    def to_json(self) -> dict:
        """Convert state to JSON-serializable dict."""
        return {
            "path": str(self.path),
            "function": self.function_name,
            "mermaid": self.mermaid,
            "cfgs": self.cfgs,
            "last_updated": self.last_updated,
            "error": self.error,
        }


# =============================================================================
# Live Server HTML Template
# =============================================================================


LIVE_HTML_TEMPLATE = """<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>Live CFG: {path}</title>
    <script src="https://cdn.jsdelivr.net/npm/mermaid/dist/mermaid.min.js"></script>
    <style>
        * {{
            box-sizing: border-box;
        }}
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            margin: 0;
            padding: 20px;
            background: #1a1a2e;
            color: #eee;
            min-height: 100vh;
        }}
        .header {{
            display: flex;
            justify-content: space-between;
            align-items: center;
            margin-bottom: 20px;
            padding-bottom: 10px;
            border-bottom: 1px solid #333;
        }}
        h1 {{
            margin: 0;
            font-size: 1.5rem;
            color: #4ade80;
        }}
        .status {{
            display: flex;
            align-items: center;
            gap: 10px;
        }}
        .status-dot {{
            width: 10px;
            height: 10px;
            border-radius: 50%;
            background: #4ade80;
            animation: pulse 2s infinite;
        }}
        @keyframes pulse {{
            0%, 100% {{ opacity: 1; }}
            50% {{ opacity: 0.5; }}
        }}
        .status-dot.error {{
            background: #f87171;
            animation: none;
        }}
        .container {{
            display: grid;
            grid-template-columns: 1fr 300px;
            gap: 20px;
        }}
        .mermaid-container {{
            background: white;
            padding: 20px;
            border-radius: 8px;
            overflow: auto;
        }}
        .mermaid {{
            text-align: center;
        }}
        .sidebar {{
            display: flex;
            flex-direction: column;
            gap: 15px;
        }}
        .info-card {{
            background: #16213e;
            padding: 15px;
            border-radius: 8px;
        }}
        .info-card h3 {{
            margin: 0 0 10px 0;
            font-size: 0.9rem;
            color: #94a3b8;
        }}
        .cfg-item {{
            padding: 8px;
            background: #1a1a2e;
            border-radius: 4px;
            margin-bottom: 8px;
        }}
        .cfg-name {{
            font-weight: bold;
            color: #60a5fa;
        }}
        .cfg-stats {{
            font-size: 0.85rem;
            color: #94a3b8;
            margin-top: 4px;
        }}
        .complexity {{
            display: inline-block;
            padding: 2px 6px;
            border-radius: 4px;
            font-size: 0.75rem;
            font-weight: bold;
        }}
        .complexity-low {{ background: #166534; color: #4ade80; }}
        .complexity-medium {{ background: #854d0e; color: #fbbf24; }}
        .complexity-high {{ background: #991b1b; color: #f87171; }}
        .error-message {{
            background: #991b1b;
            color: #fff;
            padding: 15px;
            border-radius: 8px;
            margin-bottom: 20px;
        }}
        .last-updated {{
            font-size: 0.8rem;
            color: #64748b;
        }}
    </style>
</head>
<body>
    <div class="header">
        <h1>Live CFG: <span id="path">{path}</span></h1>
        <div class="status">
            <span class="last-updated" id="lastUpdated">Connecting...</span>
            <div class="status-dot" id="statusDot"></div>
        </div>
    </div>

    <div id="error" class="error-message" style="display: none;"></div>

    <div class="container">
        <div class="mermaid-container">
            <div class="mermaid" id="mermaid">
                Loading...
            </div>
        </div>
        <div class="sidebar">
            <div class="info-card">
                <h3>Functions</h3>
                <div id="cfgList">Loading...</div>
            </div>
        </div>
    </div>

    <script>
        mermaid.initialize({{
            startOnLoad: false,
            theme: 'default',
            securityLevel: 'loose'
        }});

        let lastMermaid = '';

        function getComplexityClass(complexity) {{
            if (complexity <= 5) return 'complexity-low';
            if (complexity <= 10) return 'complexity-medium';
            return 'complexity-high';
        }}

        function formatTime(timestamp) {{
            if (!timestamp) return 'Never';
            const date = new Date(timestamp * 1000);
            return date.toLocaleTimeString();
        }}

        async function updateCFG(data) {{
            const errorDiv = document.getElementById('error');
            const statusDot = document.getElementById('statusDot');
            const lastUpdated = document.getElementById('lastUpdated');

            if (data.error) {{
                errorDiv.textContent = data.error;
                errorDiv.style.display = 'block';
                statusDot.classList.add('error');
                return;
            }}

            errorDiv.style.display = 'none';
            statusDot.classList.remove('error');
            lastUpdated.textContent = 'Updated: ' + formatTime(data.last_updated);

            // Update CFG list
            const cfgList = document.getElementById('cfgList');
            cfgList.innerHTML = data.cfgs.map(cfg => `
                <div class="cfg-item">
                    <div class="cfg-name">${{cfg.name}}</div>
                    <div class="cfg-stats">
                        Nodes: ${{cfg.node_count}} | Edges: ${{cfg.edge_count}}
                        <span class="complexity ${{getComplexityClass(cfg.complexity)}}">
                            C=${{cfg.complexity}}
                        </span>
                    </div>
                </div>
            `).join('');

            // Update Mermaid diagram only if changed
            if (data.mermaid && data.mermaid !== lastMermaid) {{
                lastMermaid = data.mermaid;
                const mermaidDiv = document.getElementById('mermaid');
                mermaidDiv.innerHTML = data.mermaid;
                try {{
                    await mermaid.run({{ nodes: [mermaidDiv] }});
                }} catch (e) {{
                    console.error('Mermaid error:', e);
                }}
            }}
        }}

        // Poll for updates
        async function poll() {{
            try {{
                const response = await fetch('/api/state');
                const data = await response.json();
                await updateCFG(data);
            }} catch (e) {{
                console.error('Poll error:', e);
                document.getElementById('statusDot').classList.add('error');
            }}
            setTimeout(poll, 1000);
        }}

        poll();
    </script>
</body>
</html>
"""


# =============================================================================
# HTTP Request Handler
# =============================================================================


class LiveCFGHandler(SimpleHTTPRequestHandler):
    """HTTP handler for live CFG server."""

    state: CFGState  # Class variable set by server

    def do_GET(self) -> None:
        """Handle GET requests."""
        if self.path == "/":
            self._serve_html()
        elif self.path == "/api/state":
            self._serve_state()
        else:
            self.send_error(404)

    def _serve_html(self) -> None:
        """Serve the live CFG HTML page."""
        html = LIVE_HTML_TEMPLATE.format(path=self.state.path.name)
        self.send_response(200)
        self.send_header("Content-Type", "text/html")
        self.send_header("Content-Length", str(len(html)))
        self.end_headers()
        self.wfile.write(html.encode())

    def _serve_state(self) -> None:
        """Serve the current CFG state as JSON."""
        data = json.dumps(self.state.to_json())
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data.encode())

    def log_message(self, format: str, *args) -> None:
        """Suppress default logging."""
        pass


# =============================================================================
# Live CFG Server
# =============================================================================


class LiveCFGServer:
    """Server for live CFG visualization."""

    def __init__(
        self,
        path: Path | str,
        function_name: str | None = None,
        config: LiveCFGConfig | None = None,
    ) -> None:
        self.path = Path(path).resolve()
        self.function_name = function_name
        self.config = config or LiveCFGConfig()
        self.state = CFGState(path=self.path, function_name=function_name)
        self._server: HTTPServer | None = None
        self._watcher_stop = threading.Event()
        self._debounce_timer: threading.Timer | None = None

    def start(self) -> None:
        """Start the live CFG server."""
        # Initial CFG build
        self.state.update()

        # Set up handler with state reference
        LiveCFGHandler.state = self.state

        # Start HTTP server
        self._server = HTTPServer((self.config.host, self.config.port), LiveCFGHandler)

        # Start file watcher in background
        watcher_thread = threading.Thread(target=self._watch_file, daemon=True)
        watcher_thread.start()

        url = f"http://{self.config.host}:{self.config.port}"
        print(f"Live CFG server running at {url}")
        print(f"Watching: {self.path}")
        print("Press Ctrl+C to stop")

        # Open browser if configured
        if self.config.auto_open:
            import webbrowser

            webbrowser.open(url)

        try:
            self._server.serve_forever()
        except KeyboardInterrupt:
            print("\nStopping...")
        finally:
            self.stop()

    def stop(self) -> None:
        """Stop the server."""
        self._watcher_stop.set()
        if self._debounce_timer:
            self._debounce_timer.cancel()
        if self._server:
            self._server.shutdown()

    def _watch_file(self) -> None:
        """Watch the file for changes."""
        last_mtime = self.path.stat().st_mtime if self.path.exists() else 0

        while not self._watcher_stop.is_set():
            try:
                if self.path.exists():
                    mtime = self.path.stat().st_mtime
                    if mtime > last_mtime:
                        last_mtime = mtime
                        self._debounced_update()
            except Exception as e:
                logger.debug(f"Watch error: {e}")

            time.sleep(0.5)

    def _debounced_update(self) -> None:
        """Update CFG with debouncing."""
        if self._debounce_timer:
            self._debounce_timer.cancel()

        self._debounce_timer = threading.Timer(self.config.debounce_ms / 1000, self.state.update)
        self._debounce_timer.start()


# =============================================================================
# CLI Integration
# =============================================================================


def start_live_cfg(
    path: Path | str,
    function_name: str | None = None,
    host: str = "127.0.0.1",
    port: int = 8765,
    auto_open: bool = True,
) -> None:
    """Start live CFG visualization server.

    Args:
        path: Path to Python file to analyze
        function_name: Optional specific function to analyze
        host: Server host
        port: Server port
        auto_open: Whether to open browser automatically
    """
    config = LiveCFGConfig(host=host, port=port, auto_open=auto_open)
    server = LiveCFGServer(path, function_name, config)
    server.start()
