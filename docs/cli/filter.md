# moss filter

Manage filter aliases for `--exclude` and `--only` flags.

## Usage

```bash
moss filter list
moss filter show <ALIAS>
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

## Usage with Commands

```bash
moss view . --exclude @tests
moss analyze --only @config
moss grep "TODO" --exclude @generated
```
