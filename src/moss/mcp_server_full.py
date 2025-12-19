"""MCP server for Moss introspection tools.

This module implements a Model Context Protocol (MCP) server that exposes
Moss's code introspection capabilities as tools for LLM interaction.

The server is generated from MossAPI introspection - all MossAPI methods
automatically become available as MCP tools.

Features:
- **Tools**: All MossAPI methods exposed as MCP tools
- **Resources**: Codebase overview, file summaries, project structure
- **Prompts**: Templates for common tasks (understand file, refactor, review)

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
Resource: type = type(None)  # Placeholder
Prompt: type = type(None)  # Placeholder
PromptArgument: type = type(None)  # Placeholder
GetPromptResult: type = type(None)  # Placeholder
PromptMessage: type = type(None)  # Placeholder
TextResourceContents: type = type(None)  # Placeholder
stdio_server: Any = None  # Placeholder

try:
    from mcp.server import Server
    from mcp.server.stdio import stdio_server
    from mcp.types import (
        GetPromptResult,
        Prompt,
        PromptArgument,
        PromptMessage,
        Resource,
        TextContent,
        TextResourceContents,
        Tool,
    )

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
    """Recursively serialize a value to JSON-safe form.

    Prefers custom to_dict() methods over raw asdict() for controlled output.
    """
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
    # Prefer to_dict() for controlled serialization (e.g., ProjectStatus)
    if hasattr(value, "to_dict") and callable(value.to_dict):
        return _serialize_value(value.to_dict())
    if is_dataclass(value) and not isinstance(value, type):
        return _serialize_value(asdict(value))
    if hasattr(value, "__dict__"):
        return _serialize_value(vars(value))
    if hasattr(value, "name"):
        # Likely an enum
        return value.name
    return str(value)


def _format_list_compact(items: list[dict[str, Any]]) -> str:
    """Format a list of dicts as compact text.

    Tries to extract meaningful fields and format concisely.
    """
    if not items:
        return "(empty)"

    lines = []
    for item in items:
        # Try common field patterns
        if "name" in item and "complexity" in item:
            # Complexity result
            risk = item.get("risk_level", "")
            lines.append(f"- {item['name']}: {item['complexity']} ({risk})")
        elif "path" in item and "changes" in item:
            # Git hotspot
            ago = item.get("last_changed", "")
            lines.append(f"- {item['path']}: {item['changes']} changes ({ago})")
        elif "name" in item and "version" in item:
            # Dependency
            lines.append(f"- {item['name']}=={item.get('version', '?')}")
        elif "name" in item:
            # Generic named item
            lines.append(f"- {item['name']}")
        elif "path" in item:
            # Generic path item
            lines.append(f"- {item['path']}")
        else:
            # Fallback: first few key=value pairs
            parts = [f"{k}={v}" for k, v in list(item.items())[:3]]
            lines.append(f"- {', '.join(parts)}")

    return f"{len(items)} items:\n" + "\n".join(lines)


def _serialize_result(result: Any) -> str | dict[str, Any]:
    """Serialize an API result to text or JSON-safe form.

    Prefers text formats for MCP (more token-efficient):
    1. Plain strings returned directly (for format functions)
    2. to_compact() if available
    3. to_markdown() as fallback
    4. Compact list formatting for list[dict]
    5. JSON for primitives and other collections
    """
    if result is None:
        return {"result": None}

    # Return strings directly (for format functions like skeleton_format, tree_format)
    if isinstance(result, str):
        return result

    # Prefer compact text representation
    if hasattr(result, "to_compact") and callable(result.to_compact):
        return result.to_compact()

    # Fallback to markdown if available
    if hasattr(result, "to_markdown") and callable(result.to_markdown):
        return result.to_markdown()

    # Handle lists specially - format as compact text
    if isinstance(result, list):
        if not result:
            return "(empty)"
        first = result[0]
        # Prefer to_compact() on list items if available
        if hasattr(first, "to_compact") and callable(first.to_compact):
            lines = [item.to_compact() for item in result]
            return f"{len(result)} items:\n" + "\n".join(f"- {line}" for line in lines)
        # Check if items are dicts or dataclasses (which will be converted to dicts)
        if isinstance(first, dict) or (is_dataclass(first) and not isinstance(first, type)):
            # Convert dataclasses to dicts first
            items = [_serialize_value(item) for item in result]
            return _format_list_compact(items)
        # Other lists (primitives, etc.)
        serialized = _serialize_value(result)
        return f"{len(serialized)} items: {', '.join(str(x) for x in serialized[:10])}"

    # For other types, serialize to JSON-safe form
    serialized = _serialize_value(result)

    if isinstance(serialized, dict):
        return serialized
    return {"result": serialized}


# Maximum output size in characters (roughly ~50K tokens)
MAX_OUTPUT_CHARS = 200_000


def _truncate_output(text: str) -> str:
    """Truncate output if too large, preserving useful context."""
    if len(text) <= MAX_OUTPUT_CHARS:
        return text

    # Keep first 80% and last 10%, with truncation message in middle
    head_size = int(MAX_OUTPUT_CHARS * 0.8)
    tail_size = int(MAX_OUTPUT_CHARS * 0.1)
    omitted = len(text) - head_size - tail_size
    truncation_msg = f"\n\n... [TRUNCATED: {omitted:,} chars omitted] ...\n\n"

    return text[:head_size] + truncation_msg + text[-tail_size:]


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
            result = await _execute_tool(name, arguments, tools)
            serialized = _serialize_result(result)
            # Return text directly if already a string, otherwise compact JSON
            if isinstance(serialized, str):
                text = serialized
            else:
                text = json.dumps(serialized, separators=(",", ":"))
            # Truncate if output is too large for LLM consumption
            return [TextContent(type="text", text=_truncate_output(text))]
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

    # -------------------------------------------------------------------------
    # Resources
    # -------------------------------------------------------------------------

    @server.list_resources()
    async def list_resources() -> list[Resource]:
        """List available resources."""
        cwd = Path.cwd()
        resources = [
            Resource(
                uri="moss://overview",
                name="Codebase Overview",
                description="High-level overview of the codebase structure and health",
                mimeType="application/json",
            ),
            Resource(
                uri="moss://structure",
                name="Project Structure",
                description="Directory structure with file counts",
                mimeType="text/plain",
            ),
        ]

        # Add file skeletons as resources
        for py_file in list(cwd.glob("src/**/*.py"))[:20]:  # Limit to avoid too many
            rel_path = py_file.relative_to(cwd)
            resources.append(
                Resource(
                    uri=f"moss://skeleton/{rel_path}",
                    name=f"Skeleton: {py_file.name}",
                    description=f"Structural skeleton of {rel_path}",
                    mimeType="text/plain",
                )
            )

        return resources

    @server.read_resource()
    async def read_resource(uri: Any) -> list[TextResourceContents]:
        """Read a resource by URI."""
        # Convert AnyUrl to string if needed
        uri_str = str(uri)
        content = _get_resource_content(uri_str)
        return [TextResourceContents(uri=uri_str, mimeType="text/plain", text=content)]

    # -------------------------------------------------------------------------
    # Prompts
    # -------------------------------------------------------------------------

    @server.list_prompts()
    async def list_prompts() -> list[Prompt]:
        """List available prompt templates."""
        return [
            Prompt(
                name="understand-file",
                description="Get comprehensive understanding of a source file",
                arguments=[
                    PromptArgument(
                        name="path",
                        description="Path to the file to understand",
                        required=True,
                    ),
                ],
            ),
            Prompt(
                name="prepare-refactor",
                description="Prepare for refactoring a module or file",
                arguments=[
                    PromptArgument(
                        name="path",
                        description="Path to the file/module to refactor",
                        required=True,
                    ),
                    PromptArgument(
                        name="goal",
                        description="What you want to achieve with the refactor",
                        required=False,
                    ),
                ],
            ),
            Prompt(
                name="code-review",
                description="Review code changes in a file or diff",
                arguments=[
                    PromptArgument(
                        name="path",
                        description="Path to the file to review",
                        required=True,
                    ),
                ],
            ),
            Prompt(
                name="find-bugs",
                description="Analyze code for potential bugs and issues",
                arguments=[
                    PromptArgument(
                        name="path",
                        description="Path to the file to analyze",
                        required=True,
                    ),
                ],
            ),
        ]

    @server.get_prompt()
    async def get_prompt(name: str, arguments: dict[str, str] | None) -> GetPromptResult:
        """Get a prompt with filled arguments."""
        args = arguments or {}
        messages = _build_prompt_messages(name, args)
        return GetPromptResult(messages=messages)

    return server


async def _execute_tool(name: str, arguments: dict[str, Any], tools: list) -> Any:
    """Execute a tool by name.

    Args:
        name: Tool name (e.g., "skeleton_extract")
        arguments: Tool arguments
        tools: List of MCPTool objects

    Returns:
        Result from the API call
    """
    import inspect

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

    # Call the method, awaiting if async
    result = method(**args)
    if inspect.iscoroutine(result):
        result = await result
    return result


# =============================================================================
# Resource Helpers
# =============================================================================


def _get_resource_content(uri: str) -> str:
    """Get content for a resource URI.

    Supported URIs:
    - moss://overview - Codebase overview
    - moss://structure - Directory structure
    - moss://skeleton/<path> - File skeleton
    """
    from moss import MossAPI

    cwd = Path.cwd()

    if uri == "moss://overview":
        return _get_overview_content(cwd)

    if uri == "moss://structure":
        return _get_structure_content(cwd)

    if uri.startswith("moss://skeleton/"):
        rel_path = uri[len("moss://skeleton/") :]
        file_path = cwd / rel_path
        if not file_path.exists():
            return f"File not found: {rel_path}"
        api = MossAPI.for_project(cwd)
        result = api.skeleton.extract(file_path)
        return result.content if hasattr(result, "content") else str(result)

    return f"Unknown resource: {uri}"


def _get_overview_content(root: Path) -> str:
    """Generate codebase overview."""
    lines = [f"# Codebase Overview: {root.name}", ""]

    # Count files
    py_files = list(root.glob("**/*.py"))
    py_files = [f for f in py_files if ".venv" not in str(f) and "__pycache__" not in str(f)]
    total_lines = 0
    for f in py_files[:100]:  # Limit for performance
        try:
            total_lines += len(f.read_text().splitlines())
        except Exception:
            pass

    lines.extend(
        [
            f"- Python files: {len(py_files)}",
            f"- Total lines: ~{total_lines}",
            "",
        ]
    )

    # Check for common files
    markers = [
        ("pyproject.toml", "Python package"),
        ("setup.py", "Legacy setuptools"),
        ("requirements.txt", "Pip requirements"),
        ("Cargo.toml", "Rust package"),
        ("package.json", "Node.js package"),
        (".git", "Git repository"),
    ]

    detected = []
    for marker, desc in markers:
        if (root / marker).exists():
            detected.append(desc)

    if detected:
        lines.append("## Detected")
        for d in detected:
            lines.append(f"- {d}")
        lines.append("")

    # Top-level structure
    lines.append("## Structure")
    for item in sorted(root.iterdir()):
        if item.name.startswith(".") and item.name not in [".github", ".moss"]:
            continue
        if item.name in ["__pycache__", ".venv", "node_modules"]:
            continue
        suffix = "/" if item.is_dir() else ""
        lines.append(f"- {item.name}{suffix}")

    return "\n".join(lines)


def _get_structure_content(root: Path) -> str:
    """Generate directory tree structure."""
    lines = []

    def walk(path: Path, prefix: str = "") -> None:
        entries = sorted(path.iterdir(), key=lambda p: (not p.is_dir(), p.name))
        # Filter out common noise
        entries = [
            e
            for e in entries
            if e.name not in ["__pycache__", ".venv", "node_modules", ".git"]
            and not e.name.endswith(".pyc")
        ]

        for i, entry in enumerate(entries[:30]):  # Limit per directory
            is_last = i == len(entries) - 1 or i == 29
            connector = "└── " if is_last else "├── "
            lines.append(f"{prefix}{connector}{entry.name}")

            if entry.is_dir() and len(prefix) < 12:  # Limit depth
                extension = "    " if is_last else "│   "
                walk(entry, prefix + extension)

        if len(entries) > 30:
            lines.append(f"{prefix}... ({len(entries) - 30} more)")

    lines.append(root.name)
    walk(root)
    return "\n".join(lines)


# =============================================================================
# Prompt Helpers
# =============================================================================


def _build_prompt_messages(name: str, args: dict[str, str]) -> list[PromptMessage]:
    """Build prompt messages for a named prompt."""
    path = args.get("path", "")
    cwd = Path.cwd()
    file_path = cwd / path if path else cwd

    if name == "understand-file":
        return _prompt_understand_file(file_path)
    elif name == "prepare-refactor":
        goal = args.get("goal", "improve code quality")
        return _prompt_prepare_refactor(file_path, goal)
    elif name == "code-review":
        return _prompt_code_review(file_path)
    elif name == "find-bugs":
        return _prompt_find_bugs(file_path)
    else:
        return [
            PromptMessage(
                role="user",
                content=TextContent(type="text", text=f"Unknown prompt: {name}"),
            )
        ]


def _prompt_understand_file(file_path: Path) -> list[PromptMessage]:
    """Build prompt for understanding a file."""
    from moss import MossAPI

    content_parts = []

    # Get skeleton
    try:
        api = MossAPI.for_project(file_path.parent)
        skeleton = api.skeleton.extract(file_path)
        content_parts.append(f"## Structure\n```\n{skeleton.content}\n```\n")
    except Exception:
        pass

    # Get dependencies
    try:
        api = MossAPI.for_project(file_path.parent)
        deps = api.deps.extract(file_path)
        if hasattr(deps, "imports") and deps.imports:
            imports_str = "\n".join(f"- {i}" for i in deps.imports[:20])
            content_parts.append(f"## Imports\n{imports_str}\n")
    except Exception:
        pass

    context = "\n".join(content_parts) if content_parts else "Unable to analyze file."

    return [
        PromptMessage(
            role="user",
            content=TextContent(
                type="text",
                text=f"""Please help me understand this file: {file_path.name}

{context}

Questions to address:
1. What is the main purpose of this file?
2. What are the key classes/functions and what do they do?
3. How does this file fit into the larger codebase?
4. Are there any notable patterns or design decisions?
""",
            ),
        )
    ]


def _prompt_prepare_refactor(file_path: Path, goal: str) -> list[PromptMessage]:
    """Build prompt for refactoring preparation."""
    from moss import MossAPI

    content_parts = []

    # Get skeleton
    try:
        api = MossAPI.for_project(file_path.parent)
        skeleton = api.skeleton.extract(file_path)
        content_parts.append(f"## Current Structure\n```\n{skeleton.content}\n```\n")
    except Exception:
        pass

    # Get complexity if available
    try:
        from moss.complexity import analyze_complexity

        report = analyze_complexity(file_path.parent, pattern=str(file_path.name))
        if report.functions:
            high_complexity = [f for f in report.functions if f.cyclomatic >= 10]
            if high_complexity:
                funcs = "\n".join(f"- {f.name}: {f.cyclomatic}" for f in high_complexity[:5])
                content_parts.append(f"## High Complexity Functions\n{funcs}\n")
    except Exception:
        pass

    context = "\n".join(content_parts) if content_parts else "File analysis unavailable."

    return [
        PromptMessage(
            role="user",
            content=TextContent(
                type="text",
                text=f"""I want to refactor: {file_path.name}

Goal: {goal}

{context}

Please help me:
1. Identify what should be refactored to achieve the goal
2. Suggest a step-by-step refactoring plan
3. Highlight any risks or things to watch out for
4. Recommend tests to add/verify before and after
""",
            ),
        )
    ]


def _prompt_code_review(file_path: Path) -> list[PromptMessage]:
    """Build prompt for code review."""
    try:
        content = file_path.read_text()
    except Exception:
        content = f"Could not read file: {file_path}"

    return [
        PromptMessage(
            role="user",
            content=TextContent(
                type="text",
                text=f"""Please review this code: {file_path.name}

```python
{content[:5000]}
```

Review checklist:
- [ ] Code correctness
- [ ] Error handling
- [ ] Security concerns
- [ ] Performance issues
- [ ] Maintainability
- [ ] Documentation
- [ ] Test coverage considerations
""",
            ),
        )
    ]


def _prompt_find_bugs(file_path: Path) -> list[PromptMessage]:
    """Build prompt for bug finding."""
    try:
        content = file_path.read_text()
    except Exception:
        content = f"Could not read file: {file_path}"

    return [
        PromptMessage(
            role="user",
            content=TextContent(
                type="text",
                text=f"""Analyze this code for bugs and issues: {file_path.name}

```python
{content[:5000]}
```

Look for:
1. Logic errors and off-by-one mistakes
2. Null/None handling issues
3. Resource leaks (files, connections)
4. Race conditions or threading issues
5. Security vulnerabilities (injection, auth)
6. Error handling gaps
7. Edge cases not handled
""",
            ),
        )
    ]


# =============================================================================
# Server Runner
# =============================================================================


async def run_server(socket_path: str | None = None) -> None:
    """Run the MCP server.

    Args:
        socket_path: Optional Unix socket path. If provided, listens on socket
                     instead of stdio. Useful for local MCP connections without
                     subprocess overhead.
    """
    _check_mcp()
    server = create_server()

    if socket_path:
        await _run_unix_socket_server(server, socket_path)
    else:
        async with stdio_server() as (read_stream, write_stream):
            await server.run(read_stream, write_stream, server.create_initialization_options())


async def _run_unix_socket_server(server: Any, socket_path: str) -> None:
    """Run MCP server over Unix socket.

    Args:
        server: The MCP server instance
        socket_path: Path to the Unix socket file
    """
    import os

    from anyio import create_unix_listener
    from mcp.types import JSONRPCMessage

    # Import SessionMessage from the right place
    try:
        from mcp.server.stdio import SessionMessage
    except ImportError:
        from mcp.shared.session import SessionMessage  # type: ignore

    socket_path_obj = Path(socket_path)

    # Remove existing socket file if present
    if socket_path_obj.exists():
        os.unlink(socket_path)

    listener = await create_unix_listener(socket_path)
    print(f"MCP server listening on Unix socket: {socket_path}")

    try:
        async with listener:
            async for client in listener:
                # Handle each client connection
                async with client:
                    await _handle_unix_client(server, client, JSONRPCMessage, SessionMessage)
    finally:
        # Clean up socket file
        if socket_path_obj.exists():
            os.unlink(socket_path)


async def _handle_unix_client(
    server: Any,
    client: Any,
    JSONRPCMessage: type,
    SessionMessage: type,
) -> None:
    """Handle a single Unix socket client connection."""
    import anyio
    from anyio import create_memory_object_stream, create_task_group

    read_stream_writer, read_stream = create_memory_object_stream(0)
    write_stream, write_stream_reader = create_memory_object_stream(0)

    async def socket_reader() -> None:
        """Read JSON-RPC messages from socket."""
        buffer = b""
        try:
            async with read_stream_writer:
                while True:
                    chunk = await client.receive(4096)
                    if not chunk:
                        break
                    buffer += chunk
                    # Process complete lines
                    while b"\n" in buffer:
                        line, buffer = buffer.split(b"\n", 1)
                        if line:
                            try:
                                msg = JSONRPCMessage.model_validate_json(line.decode("utf-8"))
                                await read_stream_writer.send(SessionMessage(msg))
                            except Exception as exc:
                                await read_stream_writer.send(exc)
        except anyio.ClosedResourceError:
            pass

    async def socket_writer() -> None:
        """Write JSON-RPC messages to socket."""
        try:
            async with write_stream_reader:
                async for session_message in write_stream_reader:
                    json_str = session_message.message.model_dump_json(
                        by_alias=True, exclude_none=True
                    )
                    await client.send((json_str + "\n").encode("utf-8"))
        except anyio.ClosedResourceError:
            pass

    async with create_task_group() as tg:
        tg.start_soon(socket_reader)
        tg.start_soon(socket_writer)
        tg.start_soon(
            server.run,
            read_stream,
            write_stream,
            server.create_initialization_options(),
        )


def main() -> None:
    """Entry point for MCP server."""
    import argparse
    import asyncio

    parser = argparse.ArgumentParser(description="Moss MCP Server")
    parser.add_argument(
        "--socket",
        "-s",
        type=str,
        help="Unix socket path (default: use stdio)",
    )
    args = parser.parse_args()

    asyncio.run(run_server(socket_path=args.socket))


if __name__ == "__main__":
    main()
