"""MCP (Model Context Protocol) tool generator from MossAPI introspection.

This module generates MCP tool definitions from the MossAPI structure.
Each API method becomes an MCP tool that can be used by AI assistants.

Example generated tools:
    skeleton_extract - Extract code skeleton from a file
    skeleton_format - Format skeleton as readable text
    anchor_find - Find code elements by name
    health_check - Run health analysis
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from moss_orchestration.gen.introspect import APIMethod, APIParameter, SubAPI, introspect_api


@dataclass
class MCPTool:
    """An MCP tool definition.

    Attributes:
        name: Tool name (e.g., "skeleton_extract")
        description: Human-readable description
        input_schema: JSON Schema for tool inputs
        api_path: Path to API method (e.g., "skeleton.extract")
    """

    name: str
    description: str = ""
    input_schema: dict[str, Any] = field(default_factory=dict)
    api_path: str = ""


def _type_to_json_schema(type_hint: str) -> dict[str, Any]:
    """Convert a Python type hint to JSON Schema type."""
    # Basic type mappings
    type_map = {
        "str": {"type": "string"},
        "int": {"type": "integer"},
        "float": {"type": "number"},
        "bool": {"type": "boolean"},
        "Path": {"type": "string", "description": "File or directory path"},
        "Any": {"type": "string"},
    }

    # Check for union types (str | Path)
    if "|" in type_hint:
        return {"type": "string"}

    # Check for list types (list[str])
    if type_hint.startswith("list["):
        inner = type_hint[5:-1]
        inner_schema = _type_to_json_schema(inner)
        return {"type": "array", "items": inner_schema}

    # Check for dict types
    if type_hint.startswith("dict["):
        return {"type": "object"}

    # Default to string if unknown
    return type_map.get(type_hint, {"type": "string"})


def _param_to_schema_property(param: APIParameter) -> tuple[str, dict[str, Any]]:
    """Convert an API parameter to a JSON Schema property.

    Returns:
        Tuple of (property_name, property_schema)
    """
    schema = _type_to_json_schema(param.type_hint)

    if param.description:
        schema["description"] = param.description

    if param.default is not None:
        schema["default"] = param.default

    return param.name, schema


def method_to_tool(method: APIMethod, api_name: str) -> MCPTool:
    """Convert an API method to an MCP tool definition.

    Args:
        method: The API method to convert
        api_name: Name of the parent API (e.g., "skeleton")

    Returns:
        MCPTool representing the method
    """
    # Build input schema
    properties: dict[str, Any] = {}
    required: list[str] = []

    for param in method.parameters:
        prop_name, prop_schema = _param_to_schema_property(param)
        properties[prop_name] = prop_schema
        if param.required:
            required.append(prop_name)

    input_schema: dict[str, Any] = {
        "type": "object",
        "properties": properties,
    }
    if required:
        input_schema["required"] = required

    # Generate tool name: subapi_method
    tool_name = f"{api_name}_{method.name}"

    # Enhance description with example from help module if available
    description = method.description
    try:
        from moss_cli.help import get_mcp_tool_description

        enhanced = get_mcp_tool_description(api_name, method.name)
        if enhanced and enhanced != description:
            # Use enhanced description if it provides more info
            description = enhanced
    except ImportError:
        pass  # help module not available, use original

    return MCPTool(
        name=tool_name,
        description=description,
        input_schema=input_schema,
        api_path=f"{api_name}.{method.name}",
    )


def subapi_to_tools(subapi: SubAPI) -> list[MCPTool]:
    """Convert a sub-API to MCP tool definitions.

    Args:
        subapi: The sub-API to convert

    Returns:
        List of MCPTool objects
    """
    return [method_to_tool(m, subapi.name) for m in subapi.methods]


class MCPGenerator:
    """Generator for MCP tools from MossAPI.

    Usage:
        generator = MCPGenerator()

        # Get tool definitions for custom handling
        tools = generator.generate_tools()

        # Get tools as MCP-compatible dicts
        tool_defs = generator.generate_tool_definitions()

        # Create an MCP server (requires mcp package)
        server = generator.generate_server()
    """

    def __init__(self):
        """Initialize the generator."""
        self._tools: list[MCPTool] | None = None

    def generate_tools(self) -> list[MCPTool]:
        """Generate MCP tools from MossAPI introspection.

        Returns:
            List of MCPTool objects
        """
        if self._tools is None:
            sub_apis = introspect_api()
            self._tools = []
            for api in sub_apis:
                self._tools.extend(subapi_to_tools(api))
        return self._tools

    def generate_tool_definitions(self) -> list[dict[str, Any]]:
        """Generate MCP-compatible tool definitions.

        Returns:
            List of dicts suitable for MCP tool registration
        """
        tools = self.generate_tools()
        return [
            {
                "name": tool.name,
                "description": tool.description,
                "inputSchema": tool.input_schema,
            }
            for tool in tools
        ]

    def generate_server(self) -> Any:
        """Generate an MCP server with all tools registered.

        Returns:
            MCP Server instance

        Raises:
            ImportError: If mcp package is not installed
        """
        try:
            from mcp.server import Server
            from mcp.types import Tool
        except ImportError as e:
            raise ImportError("MCP is required. Install with: pip install mcp") from e

        from pathlib import Path

        from moss import MossAPI

        server = Server("moss")

        # Register tools
        tools = self.generate_tools()

        @server.list_tools()
        async def list_tools() -> list[Tool]:
            return [
                Tool(
                    name=tool.name,
                    description=tool.description,
                    inputSchema=tool.input_schema,
                )
                for tool in tools
            ]

        @server.call_tool()
        async def call_tool(name: str, arguments: dict[str, Any]) -> Any:
            # Find the tool
            tool = next((t for t in tools if t.name == name), None)
            if tool is None:
                raise ValueError(f"Unknown tool: {name}")

            # Parse API path
            parts = tool.api_path.split(".")
            if len(parts) != 2:
                raise ValueError(f"Invalid API path: {tool.api_path}")

            subapi_name, method_name = parts

            # Get root from arguments or use current directory
            root = Path(arguments.pop("root", ".")).resolve()

            # Create API and call method
            api = MossAPI.for_project(root)
            subapi = getattr(api, subapi_name)
            method = getattr(subapi, method_name)

            return method(**arguments)

        return server


class MCPToolExecutor:
    """Executor for MCP tools.

    Runs MossAPI methods based on tool calls.
    """

    def __init__(self, tools: list[MCPTool] | None = None):
        """Initialize the executor.

        Args:
            tools: List of tools (generated if not provided)
        """
        if tools is None:
            generator = MCPGenerator()
            tools = generator.generate_tools()
        self._tools = {t.name: t for t in tools}

    def execute(self, tool_name: str, arguments: dict[str, Any]) -> Any:
        """Execute a tool with the given arguments.

        Args:
            tool_name: Name of the tool to execute
            arguments: Tool arguments

        Returns:
            Result from the API call
        """
        from pathlib import Path

        from moss import MossAPI

        tool = self._tools.get(tool_name)
        if tool is None:
            raise ValueError(f"Unknown tool: {tool_name}")

        # Parse API path
        parts = tool.api_path.split(".")
        if len(parts) != 2:
            raise ValueError(f"Invalid API path: {tool.api_path}")

        subapi_name, method_name = parts

        # Get root from arguments or use current directory
        args = dict(arguments)
        root = Path(args.pop("root", ".")).resolve()

        # Create API and call method
        api = MossAPI.for_project(root)
        subapi = getattr(api, subapi_name)
        method = getattr(subapi, method_name)

        return method(**args)

    def list_tools(self) -> list[str]:
        """List all available tool names."""
        return list(self._tools.keys())


def generate_mcp() -> list[MCPTool]:
    """Generate MCP tools from MossAPI.

    Convenience function that creates an MCPGenerator and returns tools.

    Returns:
        List of MCPTool objects
    """
    generator = MCPGenerator()
    return generator.generate_tools()


def generate_mcp_definitions() -> list[dict[str, Any]]:
    """Generate MCP tool definitions from MossAPI.

    Convenience function that creates an MCPGenerator and returns definitions.

    Returns:
        List of MCP-compatible tool definition dicts
    """
    generator = MCPGenerator()
    return generator.generate_tool_definitions()


__all__ = [
    "MCPGenerator",
    "MCPTool",
    "MCPToolExecutor",
    "generate_mcp",
    "generate_mcp_definitions",
    "method_to_tool",
    "subapi_to_tools",
]
