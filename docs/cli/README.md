# CLI Architecture

Moss provides a unified CLI for code intelligence. Three core primitives: **view**, **edit**, **analyze**.

## Command Categories

### Core Primitives
| Command | Purpose |
|---------|---------|
| [view](view.md) | View directories, files, symbols, line ranges |
| [edit](edit.md) | Structural code modification |
| [analyze](analyze.md) | Code quality analysis (health, complexity, security) |

### Search
| Command | Purpose |
|---------|---------|
| [text-search](text-search.md) | Fast ripgrep-based text search |
| [grep](text-search.md) | Alias for text-search |

### Index & Infrastructure
| Command | Purpose |
|---------|---------|
| [index](index.md) | Manage file index and call graph |
| [init](init.md) | Initialize moss in a project |
| [daemon](daemon.md) | Background daemon for faster operations |

### Code Quality
| Command | Purpose |
|---------|---------|
| [lint](lint.md) | Run linters, formatters, type checkers |
| [test](test.md) | Run native test runners |

### Package Management
| Command | Purpose |
|---------|---------|
| [package](package.md) | Package info, dependency trees, outdated checks |

### Scripting
| Command | Purpose |
|---------|---------|
| [script](script.md) | Run Lua scripts |

### Utilities
| Command | Purpose |
|---------|---------|
| [grammars](grammars.md) | Manage tree-sitter grammars |
| [sessions](sessions.md) | Analyze agent session logs |
| [plans](plans.md) | View Claude Code plans |
| [update](update.md) | Self-update moss |
| [filter](filter.md) | Manage filter aliases |
| [serve](serve.md) | Start MCP/HTTP/LSP server |
| [generate](generate.md) | Generate code from API specs |

## Global Options

All commands support:
- `--json` - Output as JSON
- `--jq <EXPR>` - Filter JSON with jq expression (implies --json)
- `--pretty` - Human-friendly output with colors
- `--compact` - Compact output without colors

## Design Principles

1. **Index-optional**: All commands work without an index (graceful degradation via filesystem)
2. **Unified interface**: `moss view` handles dirs, files, symbols, line ranges
3. **Composable output**: JSON output + jq for scripting
4. **Replace builtin tools**: moss view/grep replaces Read/Grep for code-aware operations
