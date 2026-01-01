# moss edit

Structural code modification using tree-sitter for precise edits.

## Target Syntax

Same as `moss view`:
- `path/to/file` - Edit file
- `file/Symbol` - Edit symbol
- `file:123` - Edit at line
- `@alias` - Edit alias target

## Operations

| Operation | Description |
|-----------|-------------|
| `replace` | Replace target with new content |
| `delete` | Delete target |
| `insert-before` | Insert before target |
| `insert-after` | Insert after target |
| `rename` | Rename symbol |
| `wrap` | Wrap target in new structure |

## Examples

```bash
# Replace a function
moss edit src/main.rs/parse_config replace "fn parse_config() { ... }"

# Delete a symbol
moss edit src/old.rs/deprecated_fn delete

# Insert before
moss edit src/lib.rs/Config insert-before "/// Documentation"

# Rename
moss edit src/api.rs/old_name rename new_name
```

## Options

- `--dry-run` - Show what would change without modifying
- `--backup` - Create backup before editing
- `-r, --root <PATH>` - Root directory

## Structural vs Text Edits

`moss edit` uses tree-sitter for structural awareness:
- Understands symbol boundaries
- Preserves formatting
- Handles nested structures

For simple text replacements, use standard tools (sed, Edit tool).
