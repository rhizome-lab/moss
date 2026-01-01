# moss test

Run native test runners for detected languages.

## Usage

```bash
moss test [PATH] [-- ARGS]
```

## Examples

```bash
# Run all tests
moss test

# Run tests in path
moss test src/

# Pass args to test runner
moss test -- --nocapture
moss test -- -v
```

## Detected Runners

| Language | Runner |
|----------|--------|
| Rust | `cargo test` |
| Go | `go test` |
| Python | `pytest`, `unittest` |
| JavaScript | `bun test`, `vitest`, `jest` |

## Options

- `--json` - JSON output
- `-r, --root <PATH>` - Root directory
