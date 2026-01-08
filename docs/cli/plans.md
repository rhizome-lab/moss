# moss sessions plans

List and view Claude Code plans from `~/.claude/plans/`.

## Usage

```bash
moss sessions plans [OPTIONS] [NAME]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `[NAME]` | Plan name to view (omit to list all plans) |

## Options

| Option | Description |
|--------|-------------|
| `-l, --limit <N>` | Maximum plans to list (default: 20) |
| `--json` | Output as JSON |
| `--pretty` | Human-friendly output with colors |

## Examples

```bash
# List recent plans
moss sessions plans

# List more plans
moss sessions plans --limit 50

# View a specific plan by name
moss sessions plans my-feature

# View plan with JSON output
moss sessions plans my-feature --json

# Fuzzy match plan names
moss sessions plans feature  # matches "my-feature", "new-feature", etc.
```

## Output Format

### List Mode

```
2025-01-08 14:30 [my-feature] Implement user authentication (1234B)
2025-01-07 10:15 [refactor] Refactor database layer (5678B)

2 plans found
```

### View Mode

Displays the full markdown content of the plan.

### JSON Output

```json
[
  {
    "name": "my-feature",
    "title": "Implement user authentication",
    "modified": "2025-01-08 14:30",
    "size": 1234
  }
]
```

## Plan File Format

Plans are markdown files stored in `~/.claude/plans/` with format:

```markdown
# Plan: Feature Title

## Steps
1. First step
2. Second step
...
```

The title is extracted from the first line (`# Plan: <title>` or `# <title>`).

## See Also

- [sessions](sessions.md) - Parent command for session analysis
