# moss sessions

Analyze Claude Code, Codex, Gemini CLI, and Moss agent session logs.

## Usage

```bash
moss sessions [OPTIONS] [SESSION_ID]
moss sessions plans [NAME]
```

## Subcommands

| Subcommand | Description |
|------------|-------------|
| `plans` | List and view agent plans from `~/.claude/plans/` etc. |

## Options

| Option | Description |
|--------|-------------|
| `--format <FORMAT>` | Log format: `claude` (default), `codex`, `gemini`, `moss` |
| `--grep <PATTERN>` | Filter sessions by regex pattern (searches content) |
| `--root <PATH>` | Project root directory |
| `--limit <N>` | Maximum sessions to list (default: 20) |
| `--analyze` | Run full analysis instead of raw log dump |
| `--jq <EXPR>` | Apply jq expression to output |
| `--json` | Output as JSON |
| `--serve` | Start web UI server |
| `--port <PORT>` | Server port (default: 3939) |

## Formats

| Format | Directory | File Pattern |
|--------|-----------|--------------|
| `claude` | `~/.claude/projects/<encoded-path>/` | `*.jsonl` |
| `codex` | `~/.codex/sessions/YYYY/MM/DD/` | `*.jsonl` |
| `gemini` | `~/.gemini/tmp/<hash>/` | `logs.json` |
| `moss` | `.moss/agent/logs/` | `*.jsonl` |

## Examples

```bash
# List Claude Code sessions (default)
moss sessions

# List Moss agent sessions
moss sessions --format moss

# Filter sessions by content
moss sessions --format moss --grep "benchmark"

# Show specific session with analysis
moss sessions abc123 --analyze

# Show session with jq filtering
moss sessions abc123 --jq '.tool_stats'

# JSON output for scripting
moss sessions --json
```

## Session Analysis

When using `--analyze`, reports include:
- Tool usage patterns (calls, errors per tool)
- Token consumption (input, output, cache)
- Message type counts
- Turn counts
- File token attribution (which files consumed tokens)
