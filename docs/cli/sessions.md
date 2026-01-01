# moss sessions

Analyze Claude Code and other agent session logs.

## Subcommands

| Subcommand | Description |
|------------|-------------|
| `list` | List sessions |
| `show <ID>` | Show session details |
| `stats` | Session statistics |

## Examples

```bash
# List recent sessions
moss sessions list

# Show specific session
moss sessions show abc123

# Statistics
moss sessions stats
```

## Session Data

Analyzes session logs from `~/.claude/` including:
- Tool usage patterns
- Token consumption
- Success/failure rates
- Duration metrics
