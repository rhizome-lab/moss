# moss tools test

Run native test runners for detected languages.

## Usage

```bash
moss tools test [PATH] [-- ARGS]   # Run tests (default)
moss tools test run [PATH]         # Explicit run
moss tools test list               # List available runners
```

## Options

| Option | Description |
|--------|-------------|
| `--runner <NAME>` | Use specific test runner |
| `--json` | JSON output |
| `-r, --root <PATH>` | Root directory |

## Examples

```bash
# Run all tests
moss tools test

# Run tests in path
moss tools test src/

# Pass args to test runner
moss tools test -- --nocapture
moss tools test -- -v

# List available runners
moss tools test list
```

## Detected Runners

| Language | Runner |
|----------|--------|
| Rust | `cargo test` |
| Go | `go test` |
| Python | `pytest`, `unittest` |
| JavaScript | `bun test`, `vitest`, `jest` |

## See Also

- [moss tools lint](lint.md) - Run linters
