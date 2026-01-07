# moss tools lint

Run linters, formatters, and type checkers.

## Usage

```bash
moss tools lint [PATH]       # Run linters (default)
moss tools lint run [PATH]   # Explicit run
moss tools lint list         # List available linters
```

## Options

| Option | Description |
|--------|-------------|
| `--fix` | Auto-fix issues where supported |
| `--json` | JSON output |
| `--only <TOOLS>` | Run only specific tools |
| `--exclude <TOOLS>` | Skip specific tools |

## Examples

```bash
# Lint current directory
moss tools lint

# Lint specific path
moss tools lint src/

# With auto-fix
moss tools lint --fix

# List available tools
moss tools lint list
```

## Detected Tools

Moss auto-detects and runs appropriate tools:

| Language | Linters |
|----------|---------|
| Rust | `cargo clippy`, `cargo fmt --check` |
| Python | `ruff`, `mypy`, `pyright` |
| JavaScript/TypeScript | `eslint`, `oxlint`, `tsc` |
| Go | `go vet`, `staticcheck` |

## See Also

- [moss tools test](test.md) - Run test runners
