# moss serve

Start a moss server (MCP, HTTP, or LSP).

## Modes

| Mode | Description |
|------|-------------|
| `mcp` | Model Context Protocol server |
| `http` | HTTP REST API |
| `lsp` | Language Server Protocol |

## Examples

```bash
# MCP server (for Claude, etc.)
moss serve mcp

# HTTP API
moss serve http --port 8080

# LSP
moss serve lsp
```

## MCP Tools

When running as MCP server, exposes:
- `view` - View files, symbols, directories
- `edit` - Structural edits
- `grep` - Text search
- `analyze` - Code analysis

## Config

```toml
[serve]
# port = 8080
# host = "127.0.0.1"
```
