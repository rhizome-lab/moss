# moss text-search (grep)

Fast text search using ripgrep. Alias: `moss grep`.

## Usage

```bash
moss text-search <PATTERN> [PATH]
moss grep <PATTERN> [PATH]
```

## Examples

```bash
# Search in current directory
moss grep "fn parse"
moss grep "TODO|FIXME"

# Search in specific path
moss grep "error" src/

# With file filtering
moss grep "impl.*Config" --only "*.rs"
moss grep "async" --exclude "@tests"

# Context lines
moss grep "panic!" -C 3
moss grep "unsafe" -B 2 -A 2

# Output modes
moss grep "Config" --files        # Just file names
moss grep "Config" --count        # Match counts
moss grep "Config" --json         # JSON output
```

## Options

### Pattern
- `-i` - Case insensitive
- `-w` - Word boundaries
- `-F` - Fixed string (not regex)

### Filtering
- `--only <PATTERN>` - Include only matching paths
- `--exclude <PATTERN>` - Exclude matching paths
- `--type <TYPE>` - File type (rs, py, js, etc.)
- `--hidden` - Search hidden files

### Context
- `-A <N>` - Lines after match
- `-B <N>` - Lines before match
- `-C <N>` - Lines before and after

### Output
- `--files` - Only show file names
- `--count` - Show match counts
- `-n` - Show line numbers (default)
- `--json` - JSON output

## vs ripgrep

`moss grep` is a thin wrapper around ripgrep with:
- Integration with moss aliases (`@tests`, `@config`)
- Consistent output formatting with other moss commands
- JSON output for scripting

For advanced ripgrep features, use `rg` directly.
