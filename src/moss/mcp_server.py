"""Single-tool MCP server for token-efficient LLM integration.

This server exposes a single `moss` tool that accepts CLI-style commands,
reducing tool definition overhead from ~8K tokens to ~50 tokens.

For the full multi-tool server (better for IDEs), see mcp_server_full.py.

Usage:
    # Run the server
    python -m moss.mcp_server

    # Or via CLI
    moss mcp-server

    # For full multi-tool server
    moss mcp-server --full
"""

from __future__ import annotations

import json
import shlex
from io import StringIO
from typing import Any

# Lazy import MCP to allow module to load without mcp installed
_mcp_available = False
Server: type = type(None)
Tool: type = type(None)
TextContent: type = type(None)
stdio_server: Any = None

try:
    from mcp.server import Server
    from mcp.server.stdio import stdio_server
    from mcp.types import TextContent, Tool

    _mcp_available = True
except ImportError:
    pass


def _check_mcp() -> None:
    """Check if MCP is available."""
    if not _mcp_available:
        raise ImportError("MCP SDK not installed. Install with: pip install 'moss[mcp]'")


# Maximum output size in characters
MAX_OUTPUT_CHARS = 200_000


def _truncate_output(text: str) -> str:
    """Truncate output if too large."""
    if len(text) <= MAX_OUTPUT_CHARS:
        return text
    head_size = int(MAX_OUTPUT_CHARS * 0.8)
    tail_size = int(MAX_OUTPUT_CHARS * 0.1)
    omitted = len(text) - head_size - tail_size
    return text[:head_size] + f"\n\n... [{omitted:,} chars truncated] ...\n\n" + text[-tail_size:]


def _execute_command(command: str) -> dict[str, Any]:
    """Execute a moss CLI command and return the result.

    Args:
        command: CLI command string (e.g., "skeleton src/main.py" or "search find_symbols Query")

    Returns:
        Dict with 'output' (stdout), 'error' (stderr if any), 'exit_code'
    """
    import sys
    from contextlib import redirect_stderr, redirect_stdout

    from moss.cli import main as cli_main

    # Parse command string into args
    try:
        args = shlex.split(command)
    except ValueError as e:
        return {"error": f"Invalid command syntax: {e}", "exit_code": 1}

    if not args:
        return {"error": "Empty command", "exit_code": 1}

    # Capture stdout/stderr
    stdout_capture = StringIO()
    stderr_capture = StringIO()

    # Save original argv
    original_argv = sys.argv

    try:
        # Set up argv as if called from CLI
        sys.argv = ["moss", *args]

        with redirect_stdout(stdout_capture), redirect_stderr(stderr_capture):
            try:
                exit_code = cli_main() or 0
            except SystemExit as e:
                exit_code = e.code if isinstance(e.code, int) else 1
            except Exception as e:
                stderr_capture.write(f"Error: {e}\n")
                exit_code = 1

    finally:
        sys.argv = original_argv

    stdout_text = stdout_capture.getvalue()
    stderr_text = stderr_capture.getvalue()

    result: dict[str, Any] = {"exit_code": exit_code}

    if stdout_text:
        result["output"] = stdout_text
    if stderr_text:
        result["error"] = stderr_text

    return result


def create_server() -> Any:
    """Create the single-tool MCP server."""
    _check_mcp()

    server = Server("moss")

    @server.list_tools()
    async def list_tools() -> list[Tool]:
        """List the single moss tool."""
        return [
            Tool(
                name="moss",
                description=(
                    "Run moss code intelligence commands. "
                    "Examples: 'skeleton src/main.py', 'search find_symbols Query', "
                    "'tree src/', 'complexity src/', 'deps src/main.py'. "
                    "Run 'help' for full command list."
                ),
                inputSchema={
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The moss command to run (without 'moss' prefix)",
                        },
                    },
                    "required": ["command"],
                },
            )
        ]

    @server.call_tool()
    async def call_tool(name: str, arguments: dict[str, Any]) -> list[TextContent]:
        """Handle tool calls."""
        if name != "moss":
            return [TextContent(type="text", text=json.dumps({"error": f"Unknown tool: {name}"}))]

        command = arguments.get("command", "")
        if not command:
            return [TextContent(type="text", text=json.dumps({"error": "No command provided"}))]

        result = _execute_command(command)

        # Format output for LLM consumption
        if result.get("exit_code", 0) == 0 and "output" in result:
            # Success - return output directly (most common case)
            text = _truncate_output(result["output"])
        else:
            # Error or mixed output - return structured result
            text = json.dumps(result, separators=(",", ":"))

        return [TextContent(type="text", text=text)]

    return server


async def run_server() -> None:
    """Run the MCP server."""
    _check_mcp()
    server = create_server()
    async with stdio_server() as (read_stream, write_stream):
        await server.run(read_stream, write_stream, server.create_initialization_options())


def main() -> None:
    """Entry point for MCP server."""
    import argparse
    import asyncio

    parser = argparse.ArgumentParser(description="Moss MCP Server")
    parser.add_argument(
        "--full",
        action="store_true",
        help="Use full multi-tool server (more tokens, better for IDEs)",
    )
    args = parser.parse_args()

    if args.full:
        from moss.mcp_server_full import main as full_main

        full_main()
    else:
        asyncio.run(run_server())


if __name__ == "__main__":
    main()
