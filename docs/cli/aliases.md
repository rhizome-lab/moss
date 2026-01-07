# moss aliases

List filter aliases for `--exclude` and `--only` flags.

## Usage

```bash
moss aliases              # List all aliases
moss aliases --json       # JSON output
moss aliases --root <DIR> # Specify project root
```

## Builtin Aliases

| Alias | Description |
|-------|-------------|
| `@tests` | Test files and directories |
| `@config` | Configuration files |
| `@build` | Build output directories |
| `@docs` | Documentation files |
| `@generated` | Generated code |

## Custom Aliases

Define in `.moss/config.toml`:

```toml
[aliases]
tests = ["*_test.go", "**/__tests__/**"]
config = ["*.toml", "*.yaml", "*.json"]
todo = ["TODO.md", "TASKS.md"]
```

Set patterns to empty array to disable a builtin alias:

```toml
[aliases]
generated = []  # Disable @generated
```

## Usage with Commands

```bash
moss view . --exclude @tests
moss analyze --only @config
moss text-search "TODO" --exclude @generated
```
