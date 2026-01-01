# moss lint

Run linters, formatters, and type checkers.

## Usage

```bash
moss lint [PATH]
```

## Examples

```bash
# Lint current directory
moss lint

# Lint specific path
moss lint src/

# With JSON output
moss lint --json
```

## Detected Tools

Moss auto-detects and runs appropriate tools:

| Language | Linters |
|----------|---------|
| Rust | `cargo clippy`, `cargo fmt --check` |
| Python | `ruff`, `mypy`, `pyright` |
| JavaScript/TypeScript | `eslint`, `oxlint`, `tsc` |
| Go | `go vet`, `staticcheck` |

## Config

Configure in `.moss/config.toml`:

```toml
[lint]
# enabled = ["clippy", "ruff"]
# disabled = ["mypy"]
```
