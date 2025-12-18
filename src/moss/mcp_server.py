"""MCP server for Moss introspection tools.

This module implements a Model Context Protocol (MCP) server that exposes
Moss's code introspection capabilities as tools for LLM interaction.

The server is generated from MossAPI introspection - all MossAPI methods
automatically become available as MCP tools.

Usage:
    # Install MCP dependencies
    pip install 'moss[mcp]'

    # Run the server
    python -m moss.mcp_server

    # Or via CLI
    moss mcp-server
"""

from __future__ import annotations

import json
from dataclasses import asdict, is_dataclass
from pathlib import Path
from typing import Any

# Lazy import MCP to allow module to load without mcp installed
_mcp_available = False
Server: type = type(None)  # Placeholder
Tool: type = type(None)  # Placeholder
TextContent: type = type(None)  # Placeholder
stdio_server: Any = None  # Placeholder

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


# =============================================================================
# Serialization Helpers
# =============================================================================


def _serialize_value(value: Any) -> Any:
    """Recursively serialize a value to JSON-safe form."""
    if value is None:
        return None
    if isinstance(value, (str, int, float, bool)):
        return value
    if isinstance(value, Path):
        return str(value)
    if isinstance(value, dict):
        return {k: _serialize_value(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [_serialize_value(v) for v in value]
    if is_dataclass(value) and not isinstance(value, type):
        return _serialize_value(asdict(value))
    if hasattr(value, "__dict__"):
        return _serialize_value(vars(value))
    if hasattr(value, "name"):
        # Likely an enum
        return value.name
    return str(value)


def _serialize_result(result: Any) -> dict[str, Any]:
    """Serialize an API result to JSON-safe form."""
    if result is None:
        return {"result": None}

    serialized = _serialize_value(result)

    if isinstance(serialized, dict):
        return serialized
    if isinstance(serialized, list):
        return {"items": serialized, "count": len(serialized)}
    return {"result": serialized}


# =============================================================================
# Server Creation
# =============================================================================


def create_server() -> Any:
    """Create and configure the MCP server.

    Uses the MCP generator to automatically expose all MossAPI
    methods as MCP tools.
    """
    _check_mcp()

    from moss.gen.mcp import MCPGenerator

    server = Server("moss")
    generator = MCPGenerator()
    tools = generator.generate_tools()

    @server.list_tools()
    async def list_tools() -> list[Tool]:
        """List available tools."""
        return [
            Tool(
                name=tool.name,
                description=tool.description,
                inputSchema=tool.input_schema,
            )
            for tool in tools
        ]

    @server.call_tool()
    async def call_tool(name: str, arguments: dict[str, Any]) -> list[TextContent]:
        """Handle tool calls."""
        try:
            result = _execute_tool(name, arguments, tools)
            serialized = _serialize_result(result)
            return [TextContent(type="text", text=json.dumps(serialized, indent=2))]
        except FileNotFoundError as e:
            return [
                TextContent(
                    type="text",
                    text=json.dumps({"error": f"File not found: {e}", "type": "not_found"}),
                )
            ]
        except Exception as e:
            return [
                TextContent(
                    type="text",
                    text=json.dumps({"error": str(e), "type": type(e).__name__}),
                )
            ]

    return server


def _execute_tool(name: str, arguments: dict[str, Any], tools: list) -> Any:
    """Execute a tool by name.

    Args:
        name: Tool name (e.g., "skeleton_extract")
        arguments: Tool arguments
        tools: List of MCPTool objects

    Returns:
        Result from the API call
    """
    from moss import MossAPI

    # Find the tool
    tool = next((t for t in tools if t.name == name), None)
    if tool is None:
        raise ValueError(f"Unknown tool: {name}")

    # Parse API path (e.g., "skeleton.extract" -> ("skeleton", "extract"))
    parts = tool.api_path.split(".")
    if len(parts) != 2:
        raise ValueError(f"Invalid API path: {tool.api_path}")

    subapi_name, method_name = parts

    # Get root from arguments or use current directory
    args = dict(arguments)
    root = Path(args.pop("root", ".")).resolve()

    # Create API and get the sub-API
    api = MossAPI.for_project(root)
    subapi = getattr(api, subapi_name, None)
    if subapi is None:
        raise ValueError(f"Unknown API: {subapi_name}")

    # Get the method
    method = getattr(subapi, method_name, None)
    if method is None:
        raise ValueError(f"Unknown method: {method_name}")

    # Call the method
    return method(**args)


# =============================================================================
# Server Runner
# =============================================================================


async def run_server() -> None:
    """Run the MCP server."""
    _check_mcp()
    server = create_server()
    async with stdio_server() as (read_stream, write_stream):
        await server.run(read_stream, write_stream, server.create_initialization_options())


def main() -> None:
    """Entry point for MCP server."""
    import asyncio

    asyncio.run(run_server())


if __name__ == "__main__":
    main()
