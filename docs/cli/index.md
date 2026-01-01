# moss index

Manage the file index and call graph for faster operations.

## Subcommands

| Subcommand | Description |
|------------|-------------|
| `status` | Show index status and statistics |
| `rebuild` | Rebuild the index from scratch |
| `refresh` | Update index with changed files |
| `clear` | Remove the index |

## Examples

```bash
# Check index status
moss index status

# Rebuild everything
moss index rebuild

# Incremental refresh
moss index refresh

# Clear index
moss index clear
```

## Options

**rebuild/refresh:**
- `--call-graph` - Build call graph (default: true)
- `--no-call-graph` - Skip call graph building
- `-r, --root <PATH>` - Root directory

## Index Contents

The index (`.moss/index.db`) stores:
- File metadata (paths, sizes, modification times)
- Symbols (functions, classes, types)
- Call graph (who calls what)
- Import/export relationships

## Index-Optional Design

All moss commands work without an index:
- `moss view` falls back to filesystem + parsing
- `moss analyze` parses files on demand
- `moss grep` uses ripgrep directly

The index provides:
- Faster symbol search
- Call graph queries (`analyze callers/callees`)
- Incremental updates

## Config

In `.moss/config.toml`:

```toml
[index]
# enabled = true      # Enable indexing
# auto_refresh = true # Auto-refresh on changes
```
