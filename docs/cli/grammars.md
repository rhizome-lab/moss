# moss grammars

Manage tree-sitter grammars for parsing.

## Subcommands

| Subcommand | Description |
|------------|-------------|
| `list` | List available grammars |
| `info <LANG>` | Show grammar info |
| `check` | Verify grammars are working |

## Examples

```bash
# List all grammars
moss grammars list

# Grammar info
moss grammars info rust
moss grammars info typescript

# Verify
moss grammars check
```

## Supported Languages

Moss includes grammars for 90+ languages via arborium.
See `moss grammars list` for the full list.
