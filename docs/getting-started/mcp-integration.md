# MCP Integration Guide

This guide explains how to integrate Moss with Claude Code (or other MCP clients) using the Model Context Protocol.

## What is MCP?

The Model Context Protocol (MCP) is a standard for connecting AI assistants to external tools and data sources. Moss provides an MCP server that exposes all its code analysis capabilities as tools.

## Available Tools

The Moss MCP server exposes 38 tools organized by sub-API:

| Category | Tools |
|----------|-------|
| **Skeleton** | `skeleton_extract`, `skeleton_format` |
| **Anchors** | `anchor_find`, `anchor_resolve` |
| **Patch** | `patch_apply`, `patch_apply_with_fallback`, `patch_create` |
| **Dependencies** | `dependencies_analyze`, `dependencies_extract`, `dependencies_format` |
| **CFG** | `cfg_build` |
| **Validation** | `validation_validate`, `validation_create_chain` |
| **Git** | `git_init`, `git_create_branch`, `git_commit` |
| **Context** | `context_init`, `context_compile` |
| **Health** | `health_check`, `health_summarize`, `health_check_docs`, `health_check_todos`, `health_analyze_structure`, `health_analyze_tests` |
| **DWIM** | `dwim_analyze_intent`, `dwim_resolve_tool`, `dwim_list_tools`, `dwim_get_tool_info` |
| **Complexity** | `complexity_analyze`, `complexity_get_high_risk` |
| **RefCheck** | `ref_check_check`, `ref_check_check_code_to_docs`, `ref_check_check_docs_to_code` |
| **GitHotspots** | `git_hotspots_analyze`, `git_hotspots_get_top_hotspots` |
| **ExternalDeps** | `external_deps_analyze`, `external_deps_list_direct`, `external_deps_check_security` |

## Setup with Claude Code

### 1. Install Moss with MCP Support

```bash
pip install 'moss[mcp]'
```

Or if using uv:

```bash
uv pip install 'moss[mcp]'
```

### 2. Configure Claude Code

Add Moss to your Claude Code MCP configuration. The configuration file location depends on your setup:

**Option A: Per-project configuration (recommended)**

Create `.claude/mcp.json` in your project root:

```json
{
  "mcpServers": {
    "moss": {
      "command": "python",
      "args": ["-m", "moss.mcp_server"],
      "cwd": "${workspaceFolder}"
    }
  }
}
```

**Option B: Global configuration**

Add to your global Claude Code settings (`~/.config/claude-code/mcp.json` or similar):

```json
{
  "mcpServers": {
    "moss": {
      "command": "python",
      "args": ["-m", "moss.mcp_server"]
    }
  }
}
```

### 3. Verify the Connection

After restarting Claude Code, you should see Moss tools available. You can test by asking Claude to:

```
Use the moss skeleton_extract tool on src/main.py
```

## Running the MCP Server Manually

For testing or debugging, you can run the MCP server directly:

```bash
# Via module
python -m moss.mcp_server

# Via CLI
moss mcp-server
```

The server uses stdio transport by default, communicating via standard input/output.

## Tool Examples

### Extract Code Skeleton

```
Tool: skeleton_extract
Arguments: {"file_path": "src/moss/cli.py"}
```

Returns the structural outline of a Python file (classes, functions, signatures).

### Find Code Anchors

```
Tool: anchor_find
Arguments: {"file_path": "src/moss/skeleton.py", "name": "extract"}
```

Finds all symbols matching the name with fuzzy matching.

### Analyze Dependencies

```
Tool: dependencies_analyze
Arguments: {}
```

Runs full dependency analysis on the project, finding circular dependencies, god modules, and orphaned files.

### Apply Code Patch

```
Tool: patch_apply
Arguments: {
  "file_path": "src/example.py",
  "anchor_name": "my_function",
  "new_content": "def my_function():\n    return 42"
}
```

Replaces a function using AST-aware anchor matching.

### Get Project Health

```
Tool: health_check
Arguments: {}
```

Returns overall project health score and metrics.

## DWIM (Do What I Mean) Tools

The DWIM tools provide intelligent routing:

### Analyze Intent

```
Tool: dwim_analyze_intent
Arguments: {"query": "show me the structure of the config module"}
```

Analyzes natural language and suggests which moss tool to use.

### Resolve Tool

```
Tool: dwim_resolve_tool
Arguments: {"query": "what functions are in cli.py?"}
```

Automatically selects and executes the appropriate tool.

## Troubleshooting

### Server Not Starting

1. Ensure MCP dependencies are installed: `pip install 'moss[mcp]'`
2. Check Python is in PATH
3. Verify the server runs manually: `python -m moss.mcp_server`

### Tools Not Appearing

1. Restart Claude Code after configuration changes
2. Check MCP configuration JSON is valid
3. Look for errors in Claude Code's developer console

### Wrong Project Directory

The MCP server operates in its current working directory. Ensure `cwd` in your configuration points to your project root.

## Advanced: Custom MCP Configuration

For more complex setups, you can configure environment variables:

```json
{
  "mcpServers": {
    "moss": {
      "command": "python",
      "args": ["-m", "moss.mcp_server"],
      "env": {
        "MOSS_ROOT": "/path/to/project",
        "MOSS_LOG_LEVEL": "DEBUG"
      }
    }
  }
}
```

## See Also

- [MCP Protocol Specification](https://modelcontextprotocol.io/)
- [Moss CLI Commands](../cli/commands.md)
- [Architecture Overview](../architecture/overview.md)
