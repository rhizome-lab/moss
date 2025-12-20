"""Line-based JSON daemon for Unix socket IPC.

This module provides a simple daemon server that:
- Listens on Unix socket at .moss/daemon.sock
- Accepts line-delimited JSON commands
- Executes via HTTPExecutor
- Returns line-delimited JSON responses
- Handles large responses with chunked streaming

Protocol:
    Request:  {"cmd": "path", "query": "..."}\n
    Response: {"ok": true, "data": {...}, "error": null}\n

For large responses (>64KB), uses length-prefixed chunking:
    4-byte big-endian length + chunk data
    Final chunk has length 0

Matches the protocol expected by crates/moss-cli/src/daemon.rs
"""

from __future__ import annotations

import asyncio
import json
import logging
import os
import signal
import struct
import time
from pathlib import Path
from typing import Any

from moss.gen.http import HTTPExecutor

logger = logging.getLogger(__name__)

CHUNK_THRESHOLD = 64 * 1024  # 64KB - switch to chunked mode above this


class DaemonProtocol(asyncio.Protocol):
    """Protocol handler for line-based JSON daemon."""

    def __init__(self, executor: HTTPExecutor, state: DaemonState):
        self.executor = executor
        self.state = state
        self.buffer = b""
        self.transport: asyncio.Transport | None = None

    def connection_made(self, transport: asyncio.Transport) -> None:
        self.transport = transport
        self.state.touch()

    def data_received(self, data: bytes) -> None:
        self.buffer += data
        self.state.touch()

        # Process complete lines
        while b"\n" in self.buffer:
            line, self.buffer = self.buffer.split(b"\n", 1)
            if line:
                self._handle_request(line.decode("utf-8", errors="replace"))

    def _handle_request(self, line: str) -> None:
        """Handle a single JSON request line."""
        try:
            request = json.loads(line)
            response = self._execute_command(request)
        except json.JSONDecodeError as e:
            response = {"ok": False, "data": None, "error": f"Invalid JSON: {e}"}
        except Exception as e:
            response = {"ok": False, "data": None, "error": str(e)}

        self._send_response(response)

    def _execute_command(self, request: dict[str, Any]) -> dict[str, Any]:
        """Execute a daemon command and return response."""
        cmd = request.get("cmd", "")
        self.state.increment_queries()

        if cmd == "status":
            return {
                "ok": True,
                "data": {
                    "uptime_secs": int(self.state.uptime_seconds()),
                    "files_indexed": self.state.files_indexed,
                    "symbols_indexed": self.state.symbols_indexed,
                    "queries_served": self.state.query_count,
                    "pid": os.getpid(),
                },
                "error": None,
            }

        if cmd == "shutdown":
            logger.info("Shutdown requested via IPC")
            # Schedule shutdown after response is sent
            asyncio.get_event_loop().call_later(0.1, self._initiate_shutdown)
            return {"ok": True, "data": None, "error": None}

        if cmd == "path":
            query = request.get("query", "")
            return self._execute_api("search.resolve_file", {"query": query})

        if cmd == "symbols":
            file_path = request.get("file", "")
            return self._execute_api("search.find_symbols", {"file_path": file_path})

        if cmd == "callers":
            symbol = request.get("symbol", "")
            return self._execute_api("explain.callers", {"symbol_name": symbol})

        if cmd == "callees":
            symbol = request.get("symbol", "")
            file_path = request.get("file", "")
            return self._execute_api(
                "explain.callees", {"symbol_name": symbol, "file_path": file_path}
            )

        if cmd == "expand":
            symbol = request.get("symbol", "")
            file_path = request.get("file")
            args: dict[str, Any] = {"symbol_name": symbol}
            if file_path:
                args["file_path"] = file_path
            return self._execute_api("skeleton.expand", args)

        return {"ok": False, "data": None, "error": f"Unknown command: {cmd}"}

    def _execute_api(self, api_path: str, args: dict[str, Any]) -> dict[str, Any]:
        """Execute an API method and return response dict."""
        try:
            result = self.executor.execute(api_path, args)
            return {"ok": True, "data": result, "error": None}
        except FileNotFoundError as e:
            return {"ok": False, "data": None, "error": f"File not found: {e}"}
        except ValueError as e:
            return {"ok": False, "data": None, "error": str(e)}
        except Exception as e:
            logger.exception("Error executing %s", api_path)
            return {"ok": False, "data": None, "error": str(e)}

    def _send_response(self, response: dict[str, Any]) -> None:
        """Send response, using chunking for large responses."""
        if self.transport is None:
            return

        response_json = json.dumps(response, separators=(",", ":"))
        response_bytes = response_json.encode("utf-8") + b"\n"

        if len(response_bytes) <= CHUNK_THRESHOLD:
            # Small response - send as single line
            self.transport.write(response_bytes)
        else:
            # Large response - use length-prefixed chunking
            self._send_chunked(response_bytes)

    def _send_chunked(self, data: bytes) -> None:
        """Send data in length-prefixed chunks."""
        if self.transport is None:
            return

        # Send signal that chunked mode is starting
        # First line indicates chunked transfer
        header = json.dumps({"chunked": True, "total_size": len(data)})
        self.transport.write(header.encode("utf-8") + b"\n")

        # Send chunks with 4-byte big-endian length prefix
        chunk_size = 32 * 1024  # 32KB chunks
        offset = 0

        while offset < len(data):
            chunk = data[offset : offset + chunk_size]
            length_prefix = struct.pack(">I", len(chunk))
            self.transport.write(length_prefix + chunk)
            offset += len(chunk)

        # Send zero-length terminator
        self.transport.write(struct.pack(">I", 0))

    def _initiate_shutdown(self) -> None:
        """Initiate graceful shutdown."""
        os.kill(os.getpid(), signal.SIGTERM)

    def connection_lost(self, exc: Exception | None) -> None:
        self.transport = None


class DaemonState:
    """Track daemon state and activity."""

    def __init__(self) -> None:
        self.start_time = time.time()
        self.last_activity = time.time()
        self.query_count = 0
        self.files_indexed = 0
        self.symbols_indexed = 0

    def touch(self) -> None:
        """Update last activity time."""
        self.last_activity = time.time()

    def increment_queries(self) -> None:
        """Increment query counter."""
        self.query_count += 1

    def uptime_seconds(self) -> float:
        """Get uptime in seconds."""
        return time.time() - self.start_time

    def idle_seconds(self) -> float:
        """Get idle time in seconds."""
        return time.time() - self.last_activity


async def run_daemon(
    root: str | Path = ".",
    socket_path: str | Path | None = None,
    idle_timeout: int = 600,
) -> None:
    """Run the daemon server.

    Args:
        root: Project root directory
        socket_path: Path to Unix socket (default: root/.moss/daemon.sock)
        idle_timeout: Shutdown after this many idle seconds (0 = never)
    """
    root_path = Path(root).resolve()

    if socket_path is None:
        socket_path = root_path / ".moss" / "daemon.sock"
    else:
        socket_path = Path(socket_path)

    # Ensure directory exists
    socket_path.parent.mkdir(parents=True, exist_ok=True)

    # Remove stale socket
    if socket_path.exists():
        socket_path.unlink()

    executor = HTTPExecutor(root_path)
    state = DaemonState()

    loop = asyncio.get_event_loop()
    server = await loop.create_unix_server(
        lambda: DaemonProtocol(executor, state),
        path=str(socket_path),
    )

    logger.info("Daemon listening on %s", socket_path)

    # Set up signal handlers
    shutdown_event = asyncio.Event()

    def handle_signal() -> None:
        logger.info("Received shutdown signal")
        shutdown_event.set()

    for sig in (signal.SIGTERM, signal.SIGINT):
        loop.add_signal_handler(sig, handle_signal)

    # Idle timeout checker
    async def check_idle() -> None:
        while not shutdown_event.is_set():
            await asyncio.sleep(30)
            if idle_timeout > 0 and state.idle_seconds() > idle_timeout:
                logger.info("Idle timeout reached (%ds), shutting down", idle_timeout)
                shutdown_event.set()
                break

    idle_task = asyncio.create_task(check_idle())

    try:
        async with server:
            await shutdown_event.wait()
    finally:
        idle_task.cancel()
        try:
            await idle_task
        except asyncio.CancelledError:
            pass

        # Clean up socket
        if socket_path.exists():
            socket_path.unlink()

        logger.info("Daemon shutdown complete")


def main() -> None:
    """CLI entry point for moss-daemon."""
    import argparse

    parser = argparse.ArgumentParser(
        prog="moss-daemon",
        description="Run the Moss daemon (line-based JSON over Unix socket)",
    )
    parser.add_argument(
        "root",
        nargs="?",
        default=".",
        help="Project root directory (default: current directory)",
    )
    parser.add_argument(
        "--socket",
        help="Unix socket path (default: root/.moss/daemon.sock)",
    )
    parser.add_argument(
        "--idle-timeout",
        type=int,
        default=600,
        help="Shutdown after N seconds of inactivity (default: 600, 0 = never)",
    )
    parser.add_argument(
        "--debug",
        action="store_true",
        help="Enable debug logging",
    )

    args = parser.parse_args()

    logging.basicConfig(
        level=logging.DEBUG if args.debug else logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )

    asyncio.run(
        run_daemon(
            root=args.root,
            socket_path=args.socket,
            idle_timeout=args.idle_timeout,
        )
    )


__all__ = [
    "DaemonProtocol",
    "DaemonState",
    "main",
    "run_daemon",
]
