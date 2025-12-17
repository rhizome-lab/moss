"""MCP server for Moss introspection tools.

This module implements a Model Context Protocol (MCP) server that exposes
Moss's code introspection capabilities as tools for LLM interaction.

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
from pathlib import Path
from typing import Any

# Lazy import MCP to allow module to load without mcp installed
_mcp_available = False
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


def create_server() -> Any:
    """Create and configure the MCP server."""
    _check_mcp()

    server = Server("moss")

    @server.list_tools()
    async def list_tools() -> list[Tool]:
        """List available tools."""
        return [
            Tool(
                name="skeleton",
                description="Extract code skeleton (classes, functions, methods) from Python",
                inputSchema={
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to Python file or directory",
                        },
                        "pattern": {
                            "type": "string",
                            "description": "Glob pattern for directory (default: **/*.py)",
                        },
                    },
                    "required": ["path"],
                },
            ),
            Tool(
                name="anchors",
                description="Find code anchors (functions, classes, methods) with filtering",
                inputSchema={
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to Python file or directory",
                        },
                        "type": {
                            "type": "string",
                            "enum": ["function", "class", "method", "all"],
                            "description": "Type of anchors to find",
                        },
                        "name": {
                            "type": "string",
                            "description": "Regex pattern to filter by name",
                        },
                    },
                    "required": ["path"],
                },
            ),
            Tool(
                name="cfg",
                description="Build control flow graph for functions in a Python file",
                inputSchema={
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to Python file",
                        },
                        "function": {
                            "type": "string",
                            "description": "Specific function name (optional)",
                        },
                    },
                    "required": ["path"],
                },
            ),
            Tool(
                name="deps",
                description="Extract dependencies (imports and exports) from Python files",
                inputSchema={
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to Python file or directory",
                        },
                        "pattern": {
                            "type": "string",
                            "description": "Glob pattern for directory (default: **/*.py)",
                        },
                    },
                    "required": ["path"],
                },
            ),
            Tool(
                name="context",
                description="Generate compiled context (skeleton + deps + summary) for a file",
                inputSchema={
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to Python file",
                        },
                    },
                    "required": ["path"],
                },
            ),
            Tool(
                name="apply_patch",
                description="Apply a patch to modify code at a specific anchor location",
                inputSchema={
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file to modify",
                        },
                        "anchor_type": {
                            "type": "string",
                            "enum": ["function", "class", "method"],
                            "description": "Type of anchor to target",
                        },
                        "anchor_name": {
                            "type": "string",
                            "description": "Name of the anchor (function/class/method name)",
                        },
                        "new_content": {
                            "type": "string",
                            "description": "New content to replace the anchor",
                        },
                        "context": {
                            "type": "string",
                            "description": "Parent context for methods (class name)",
                        },
                    },
                    "required": ["path", "anchor_type", "anchor_name", "new_content"],
                },
            ),
            Tool(
                name="analyze_intent",
                description=(
                    "Analyze a natural language description to find the best matching "
                    "introspection tool. Uses TF-IDF cosine similarity and semantic matching. "
                    "Call this when unsure which tool to use for a task."
                ),
                inputSchema={
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": (
                                "Natural language description of what you want to do, "
                                "e.g. 'find all classes that inherit from BaseClass' or "
                                "'show me the imports in this file'"
                            ),
                        },
                        "top_k": {
                            "type": "integer",
                            "description": "Number of suggestions to return (default: 3)",
                            "default": 3,
                        },
                    },
                    "required": ["query"],
                },
            ),
            Tool(
                name="resolve_tool",
                description=(
                    "Resolve a tool name that might be misspelled or an alias. "
                    "Handles typos (e.g., 'skelton' -> 'skeleton') and semantic aliases "
                    "(e.g., 'imports' -> 'deps', 'structure' -> 'skeleton')."
                ),
                inputSchema={
                    "type": "object",
                    "properties": {
                        "tool_name": {
                            "type": "string",
                            "description": "The tool name to resolve",
                        },
                    },
                    "required": ["tool_name"],
                },
            ),
            Tool(
                name="list_capabilities",
                description=(
                    "List all available introspection tools with their descriptions, "
                    "parameters, keywords, and aliases. Use this to understand what "
                    "tools are available."
                ),
                inputSchema={
                    "type": "object",
                    "properties": {},
                },
            ),
        ]

    @server.call_tool()
    async def call_tool(name: str, arguments: dict[str, Any]) -> list[TextContent]:
        """Handle tool calls."""
        try:
            if name == "skeleton":
                result = _tool_skeleton(arguments)
            elif name == "anchors":
                result = _tool_anchors(arguments)
            elif name == "cfg":
                result = _tool_cfg(arguments)
            elif name == "deps":
                result = _tool_deps(arguments)
            elif name == "context":
                result = _tool_context(arguments)
            elif name == "apply_patch":
                result = _tool_apply_patch(arguments)
            elif name == "analyze_intent":
                result = _tool_analyze_intent(arguments)
            elif name == "resolve_tool":
                result = _tool_resolve_tool(arguments)
            elif name == "list_capabilities":
                result = _tool_list_capabilities(arguments)
            else:
                result = {"error": f"Unknown tool: {name}"}

            return [TextContent(type="text", text=json.dumps(result, indent=2))]
        except Exception as e:
            return [TextContent(type="text", text=json.dumps({"error": str(e)}))]

    return server


def _symbol_to_dict(symbol: Any) -> dict:
    """Convert a Symbol to a dictionary."""
    result = {
        "name": symbol.name,
        "kind": symbol.kind,
        "line": symbol.lineno,
    }
    if symbol.end_lineno is not None:
        result["end_line"] = symbol.end_lineno
        result["line_count"] = symbol.line_count
    if symbol.signature:
        result["signature"] = symbol.signature
    if symbol.docstring:
        result["docstring"] = symbol.docstring
    if symbol.children:
        result["children"] = [_symbol_to_dict(c) for c in symbol.children]
    return result


def _tool_skeleton(args: dict[str, Any]) -> dict | list:
    """Extract code skeleton."""
    from moss.skeleton import extract_python_skeleton

    path = Path(args["path"]).resolve()

    if not path.exists():
        return {"error": f"Path does not exist: {path}"}

    if path.is_file():
        files = [path]
    else:
        pattern = args.get("pattern", "**/*.py")
        files = list(path.glob(pattern))

    results = []
    for file_path in files:
        try:
            source = file_path.read_text()
            symbols = extract_python_skeleton(source)
            results.append(
                {
                    "file": str(file_path),
                    "symbols": [_symbol_to_dict(s) for s in symbols],
                }
            )
        except SyntaxError as e:
            results.append(
                {
                    "file": str(file_path),
                    "error": f"Syntax error: {e}",
                }
            )

    return results[0] if len(results) == 1 else results


def _tool_anchors(args: dict[str, Any]) -> dict | list:
    """Find code anchors."""
    import re

    from moss.skeleton import extract_python_skeleton

    path = Path(args["path"]).resolve()

    if not path.exists():
        return {"error": f"Path does not exist: {path}"}

    type_filter = args.get("type", "all")
    if type_filter == "all":
        type_filter = None
    name_pattern = re.compile(args["name"]) if args.get("name") else None

    if path.is_file():
        files = [path]
    else:
        pattern = args.get("pattern", "**/*.py")
        files = list(path.glob(pattern))

    results = []

    def collect_symbols(symbols: list, file_path: Path, parent: str | None = None) -> None:
        for sym in symbols:
            kind = sym.kind
            if type_filter and kind != type_filter:
                if sym.children:
                    collect_symbols(sym.children, file_path, sym.name)
                continue

            if name_pattern and not name_pattern.search(sym.name):
                if sym.children:
                    collect_symbols(sym.children, file_path, sym.name)
                continue

            anchor_info = {
                "file": str(file_path),
                "name": sym.name,
                "type": kind,
                "line": sym.lineno,
            }
            if parent:
                anchor_info["context"] = parent
            if sym.signature:
                anchor_info["signature"] = sym.signature
            results.append(anchor_info)

            if sym.children:
                collect_symbols(sym.children, file_path, sym.name)

    for file_path in files:
        try:
            source = file_path.read_text()
            symbols = extract_python_skeleton(source)
            collect_symbols(symbols, file_path)
        except SyntaxError:
            pass

    return results


def _tool_cfg(args: dict[str, Any]) -> dict | list:
    """Build control flow graph."""
    from moss.cfg import build_cfg

    path = Path(args["path"]).resolve()

    if not path.exists():
        return {"error": f"Path does not exist: {path}"}

    if not path.is_file():
        return {"error": "cfg requires a file path, not a directory"}

    try:
        source = path.read_text()
        cfgs = build_cfg(source)
    except SyntaxError as e:
        return {"error": f"Syntax error: {e}"}

    # Filter by function name if provided
    func_name = args.get("function")
    if func_name:
        cfgs = [c for c in cfgs if c.name == func_name]

    if not cfgs:
        return {"error": "No functions found"}

    results = []
    for cfg in cfgs:
        results.append(
            {
                "name": cfg.name,
                "node_count": cfg.node_count,
                "edge_count": cfg.edge_count,
                "entry": cfg.entry_node,
                "exit": cfg.exit_node,
                "nodes": {
                    nid: {
                        "type": n.node_type.value,
                        "statements": n.statements,
                        "line_start": n.line_start,
                    }
                    for nid, n in cfg.nodes.items()
                },
                "edges": [
                    {
                        "source": e.source,
                        "target": e.target,
                        "type": e.edge_type.value,
                        "condition": e.condition,
                    }
                    for e in cfg.edges
                ],
            }
        )

    return results


def _tool_deps(args: dict[str, Any]) -> dict | list:
    """Extract dependencies."""
    from moss.dependencies import extract_dependencies

    path = Path(args["path"]).resolve()

    if not path.exists():
        return {"error": f"Path does not exist: {path}"}

    if path.is_file():
        files = [path]
    else:
        pattern = args.get("pattern", "**/*.py")
        files = list(path.glob(pattern))

    results = []
    for file_path in files:
        try:
            source = file_path.read_text()
            deps = extract_dependencies(source)
            results.append(
                {
                    "file": str(file_path),
                    "imports": [
                        {
                            "module": i.module,
                            "names": i.names,
                            "alias": i.alias,
                            "line": i.lineno,
                        }
                        for i in deps.imports
                    ],
                    "exports": [
                        {"name": e.name, "kind": e.kind, "line": e.lineno} for e in deps.exports
                    ],
                }
            )
        except SyntaxError as e:
            results.append(
                {
                    "file": str(file_path),
                    "error": f"Syntax error: {e}",
                }
            )

    return results[0] if len(results) == 1 else results


def _tool_context(args: dict[str, Any]) -> dict:
    """Generate compiled context."""
    from moss.dependencies import extract_dependencies
    from moss.skeleton import extract_python_skeleton

    path = Path(args["path"]).resolve()

    if not path.exists():
        return {"error": f"Path does not exist: {path}"}

    if not path.is_file():
        return {"error": "context requires a file path, not a directory"}

    try:
        source = path.read_text()
        symbols = extract_python_skeleton(source)
        deps = extract_dependencies(source)
    except SyntaxError as e:
        return {"error": f"Syntax error: {e}"}

    # Count symbols
    def count_symbols(syms: list) -> dict:
        counts = {"classes": 0, "functions": 0, "methods": 0}
        for s in syms:
            kind = s.kind
            if kind == "class":
                counts["classes"] += 1
            elif kind == "function":
                counts["functions"] += 1
            elif kind == "method":
                counts["methods"] += 1
            if s.children:
                child_counts = count_symbols(s.children)
                for k, v in child_counts.items():
                    counts[k] += v
        return counts

    counts = count_symbols(symbols)
    line_count = len(source.splitlines())

    return {
        "file": str(path),
        "summary": {
            "lines": line_count,
            "classes": counts["classes"],
            "functions": counts["functions"],
            "methods": counts["methods"],
            "imports": len(deps.imports),
            "exports": len(deps.exports),
        },
        "symbols": [_symbol_to_dict(s) for s in symbols],
        "imports": [
            {"module": i.module, "names": i.names, "alias": i.alias, "line": i.lineno}
            for i in deps.imports
        ],
        "exports": [{"name": e.name, "kind": e.kind, "line": e.lineno} for e in deps.exports],
    }


def _tool_apply_patch(args: dict[str, Any]) -> dict:
    """Apply a patch to code."""
    from moss.anchors import Anchor, AnchorNotFoundError, AnchorType, resolve_anchor
    from moss.patches import Patch, apply_patch

    path = Path(args["path"]).resolve()

    if not path.exists():
        return {"error": f"Path does not exist: {path}"}

    if not path.is_file():
        return {"error": "apply_patch requires a file path"}

    # Map string type to AnchorType
    type_map = {
        "function": AnchorType.FUNCTION,
        "class": AnchorType.CLASS,
        "method": AnchorType.METHOD,
    }
    anchor_type = type_map.get(args["anchor_type"])
    if not anchor_type:
        return {"error": f"Invalid anchor type: {args['anchor_type']}"}

    source = path.read_text()

    # Create anchor
    anchor = Anchor(
        type=anchor_type,
        name=args["anchor_name"],
        context=args.get("context"),
    )

    try:
        match = resolve_anchor(source, anchor)
    except AnchorNotFoundError as e:
        return {
            "error": f"Anchor not found: {anchor.name}",
            "suggestions": e.suggestions,
        }

    # Create and apply patch
    patch = Patch(
        anchor=anchor,
        match=match,
        new_content=args["new_content"],
    )

    try:
        new_source = apply_patch(source, patch)
    except Exception as e:
        return {"error": f"Failed to apply patch: {e}"}

    # Write back
    path.write_text(new_source)

    return {
        "success": True,
        "file": str(path),
        "anchor": {
            "type": args["anchor_type"],
            "name": args["anchor_name"],
            "line": match.lineno,
        },
    }


def _tool_analyze_intent(args: dict[str, Any]) -> dict:
    """Analyze intent and suggest matching tools."""
    from moss.dwim import analyze_intent

    query = args.get("query", "")
    top_k = args.get("top_k", 3)

    if not query:
        return {"error": "Query is required"}

    matches = analyze_intent(query)[:top_k]

    if not matches:
        return {
            "query": query,
            "matches": [],
            "message": "No matching tools found. Use list_capabilities to see available tools.",
        }

    return {
        "query": query,
        "matches": [
            {
                "tool": m.tool,
                "confidence": round(m.confidence, 3),
                "message": m.message,
            }
            for m in matches
        ],
        "recommended": matches[0].tool if matches else None,
    }


def _tool_resolve_tool(args: dict[str, Any]) -> dict:
    """Resolve a tool name (handle typos and aliases)."""
    from moss.dwim import resolve_tool

    tool_name = args.get("tool_name", "")

    if not tool_name:
        return {"error": "tool_name is required"}

    match = resolve_tool(tool_name)

    return {
        "input": tool_name,
        "resolved": match.tool,
        "confidence": round(match.confidence, 3),
        "message": match.message,
        "is_exact": match.confidence >= 1.0,
    }


def _tool_list_capabilities(args: dict[str, Any]) -> dict:
    """List all available tools with metadata."""
    from moss.dwim import TOOL_ALIASES, TOOL_REGISTRY

    tools = []
    for name, info in TOOL_REGISTRY.items():
        aliases = [alias for alias, target in TOOL_ALIASES.items() if target == name]
        tools.append(
            {
                "name": info.name,
                "description": info.description,
                "keywords": info.keywords,
                "parameters": info.parameters,
                "aliases": aliases,
            }
        )

    return {
        "tools": tools,
        "count": len(tools),
        "hint": (
            "Use 'analyze_intent' with a natural language query to find the best tool "
            "for your task, or 'resolve_tool' to handle typos and aliases."
        ),
    }


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
