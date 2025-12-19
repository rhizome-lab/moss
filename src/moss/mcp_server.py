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


# Known CLI subcommands for detecting natural language vs CLI syntax
_CLI_COMMANDS = {
    "init",
    "config",
    "distros",
    "skeleton",
    "tree",
    "anchors",
    "query",
    "cfg",
    "deps",
    "context",
    "complexity",
    "clones",
    "patterns",
    "weaknesses",
    "lint",
    "security",
    "rules",
    "health",
    "report",
    "overview",
    "check-docs",
    "check-todos",
    "check-refs",
    "coverage",
    "external-deps",
    "git-hotspots",
    "diff",
    "checkpoint",
    "synthesize",
    "edit",
    "search",
    "rag",
    "mcp-server",
    "acp-server",
    "lsp",
    "tui",
    "shell",
    "explore",
    "gen",
    "watch",
    "hooks",
    "mutate",
    "run",
    "status",
    "pr",
    "roadmap",
    "metrics",
    "summarize",
    "analyze-session",
    "extract-preferences",
    "diff-preferences",
    "eval",
    "dwim",
    "help",
    "loop",
}


def _extract_paths(text: str) -> list[str]:
    """Extract file paths from natural language text."""
    import re

    paths = []
    # Match patterns like src/foo.py, ./bar.py, foo/bar/, *.py
    for match in re.finditer(r"[\w./\-*]+\.(?:py|md|json|yaml|toml|txt|js|ts|rs|go)", text):
        paths.append(match.group())
    # Match directory patterns like src/, ./foo/bar/
    for match in re.finditer(r'(?:^|[\s"])([./]?[\w\-]+(?:/[\w\-]+)+)/?(?:[\s"]|$)', text):
        paths.append(match.group(1))
    return paths


# Map DWIM tool names to CLI commands
_TOOL_TO_CLI: dict[str, str] = {
    "skeleton": "skeleton",
    "skeleton_extract": "skeleton",
    "skeleton_expand": "skeleton",
    "search_summarize_module": "summarize",
    "health_summarize": "summarize",
    "anchors": "anchors",
    "deps": "deps",
    "query": "query",
    "cfg": "cfg",
    "context": "context",
}


def _dwim_rewrite(command: str) -> str | None:
    """Try to rewrite natural language as a CLI command using DWIM.

    Returns rewritten command or None if input looks like valid CLI syntax.
    """
    from pathlib import Path

    try:
        args = shlex.split(command)
    except ValueError:
        return None  # Malformed quotes - let _execute_command handle the error

    if not args:
        return None

    # If first word is a known command, don't rewrite
    if args[0].lower() in _CLI_COMMANDS:
        return None

    # Looks like natural language - use DWIM
    from moss import MossAPI
    from moss.dwim import resolve_tool

    # First, check if first word is an alias (e.g., "structure" → "skeleton")
    first_word = args[0].lower()
    alias_match = resolve_tool(first_word)
    if alias_match.confidence >= 0.9:
        tool = alias_match.tool
    else:
        # Fall back to semantic analysis for full natural language
        api = MossAPI.for_project(Path.cwd())
        results = api.dwim.analyze_intent(command, top_k=1)

        if not results or results[0].confidence < 0.15:
            return None

        tool = results[0].tool

    # Map tool names to CLI commands
    cli_cmd = _TOOL_TO_CLI.get(tool)
    if not cli_cmd:
        # Fallback: try direct name conversion (underscores -> hyphens)
        cli_cmd = tool.replace("_", "-").split("-")[0]
        if cli_cmd not in _CLI_COMMANDS:
            return None

    # Extract paths from the original query
    paths = _extract_paths(command)

    # Build the command
    if paths:
        return f"{cli_cmd} {' '.join(paths)}"
    return cli_cmd


def _execute_command(command: str) -> dict[str, Any]:
    """Execute a moss CLI command and return the result.

    Args:
        command: CLI command string or natural language query

    Returns:
        Dict with 'output' (stdout), 'error' (stderr if any), 'exit_code'
    """
    import sys
    from contextlib import redirect_stderr, redirect_stdout

    from moss.cli import main as cli_main
    from moss.output import reset_output

    # Try DWIM rewrite for natural language
    rewritten = _dwim_rewrite(command)
    if rewritten:
        command = rewritten

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

        # Reset global output so it picks up redirected stdout
        reset_output()

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
        reset_output()  # Reset again so future calls get fresh stdout

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
                description="Code intelligence. Guesses intent from natural language.",
                inputSchema={
                    "type": "object",
                    "properties": {"command": {"type": "string"}},
                },
            )
        ]

    @server.call_tool()
    async def call_tool(name: str, arguments: dict[str, Any]) -> list[TextContent]:
        """Handle tool calls."""
        if name != "moss":
            return [TextContent(type="text", text=f"Unknown tool: {name}")]

        command = arguments.get("command", "")
        if not command:
            return [TextContent(type="text", text="No command provided")]

        result = _execute_command(command)

        # Always return plain strings
        output = result.get("output", "")
        error = result.get("error", "")

        if error and output:
            text = f"{output}\n{error}"
        elif error:
            text = error
        elif output:
            text = output
        else:
            text = "(no output)"

        return [TextContent(type="text", text=_truncate_output(text))]

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
