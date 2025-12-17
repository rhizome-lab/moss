# Moss Tools

**CLI**: `moss <command> [options]` — all commands support `--json`

**MCP Server**: `moss mcp-server`

## Introspection

- **skeleton** `<path>` — classes, functions, methods with signatures and docstrings
- **anchors** `<path>` — find code elements; `--type` (function/class/method), `--name` (regex)
- **query** `<path>` — pattern search; `--name`, `--signature`, `--type`, `--inherits`, `--min-lines`, `--max-lines`
- **cfg** `<file> [function]` — control flow graph; `--dot` for graphviz
- **deps** `<path>` — imports and exports
- **context** `<file>` — combined skeleton + deps + summary

## Editing

- **apply_patch** (MCP) — anchor-based code modification

## Discovery (MCP)

- **analyze_intent** `query` — natural language → tool recommendation
- **resolve_tool** `name` — typo/alias → canonical tool
- **list_capabilities** — all tools with metadata

## Aliases

skeleton: structure, outline, symbols, tree, hierarchy
anchors: functions, classes, methods, definitions, defs, locate
query: search, find, grep, filter
deps: imports, dependencies, exports, modules
cfg: flow, graph, control-flow
context: summary, overview, info
