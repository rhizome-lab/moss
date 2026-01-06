# Moss Tools

**CLI**: `moss <command> [options]` — all commands support `--json`

**MCP Server**: `moss mcp-server`

## Core Primitives

Three tools for all codebase operations:

### view

Show tree, file skeleton, or symbol source.

```
moss view [target] [options]
```

- `view` — project tree
- `view src/` — directory contents
- `view file.py` — file skeleton (fuzzy paths OK)
- `view file.py/Class` — symbol source
- `--depth N` — expansion depth
- `--deps` — show dependencies
- `--calls` — show callers
- `--called-by` — show callees

### edit

Structural code modifications.

```
moss edit <target> [options]
```

- `--delete` — remove node
- `--replace "code"` — swap content
- `--before "code"` — insert before
- `--after "code"` — insert after
- `--prepend "code"` — add to start
- `--append "code"` — add to end

### analyze

Health, complexity, and security analysis.

```
moss analyze [target] [options]
```

- `analyze` — full codebase analysis
- `--health` — file counts, line counts, avg complexity
- `--complexity` — cyclomatic complexity per function
- `--security` — vulnerability scanning

## Search & Sessions

### text-search

Regex search with structural awareness.

```
moss text-search <pattern> [options]
```

- `--only <glob>` — filter by filename/path
- `--context N` — lines of context
- `--json` — structured output

### sessions

Analyze agent session logs (Claude Code, Codex, Gemini, Moss).

```
moss sessions [session_id] [options]
```

- `--format <fmt>` — `claude` (default), `codex`, `gemini`, `moss`
- `--grep <pattern>` — filter sessions by content
- `--analyze` — full session analysis
- `--jq <expr>` — apply jq expression

## DWIM Resolution

Tool names are resolved with fuzzy matching. Fuzzy path resolution also works: `dwim.py` → `src/moss/dwim.py`
