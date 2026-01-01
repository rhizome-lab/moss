# moss view

View directories, files, symbols, or line ranges. The primary way to explore code.

## Target Syntax

| Syntax | Description |
|--------|-------------|
| `.` | Current directory tree |
| `path/to/dir` | Directory tree |
| `path/to/file` | File skeleton (symbols) |
| `file/Symbol` | Symbol in file |
| `file/Parent/method` | Nested symbol |
| `Parent/method` | Symbol search (when Parent isn't a path) |
| `SymbolName` | Symbol search across codebase |
| `file:123` | Symbol containing line 123 |
| `file:10-20` | Lines 10-20 (raw content) |

## Examples

```bash
# Directory tree
moss view .
moss view src/

# File skeleton (signatures only)
moss view src/main.rs
moss view src/main.rs --types-only

# Symbol within file
moss view src/main.rs/main
moss view src/config.rs/Config/new

# Symbol search (finds across codebase)
moss view Config
moss view Config/new

# Line-based
moss view src/main.rs:42        # Symbol at line 42
moss view src/main.rs:10-50     # Lines 10-50
```

## Options

### Display Control
- `-d, --depth <N>` - Expansion depth (0=names, 1=signatures, 2=children, -1=all)
- `-n, --line-numbers` - Show line numbers
- `--full` - Show full source code
- `--docs` - Show full docstrings (default: summary only)
- `--raw` - Disable smart display (no collapsing single-child dirs)

### Filtering
- `-t, --type <KIND>` - Filter by symbol type: class, function, method
- `--types-only` - Show only type definitions (class, struct, enum, interface)
- `--tests` - Include test functions (hidden by default)
- `--exclude <PATTERN>` - Exclude paths matching pattern or @alias
- `--only <PATTERN>` - Include only paths matching pattern or @alias

### Context
- `--deps` - Show imports/exports
- `--focus[=MODULE]` - Show skeletons of imported modules
- `--resolve-imports` - Inline signatures of imported symbols
- `--context` - Skeleton + imports combined
- `--no-parent` - Hide ancestor context for nested symbols

### Output
- `--json` - Output as JSON
- `--pretty` - Syntax highlighting and colors
- `-r, --root <PATH>` - Root directory (default: current)

## Module Structure

```
view/
├── mod.rs      # Args, config, main routing
├── search.rs   # Symbol search (index + filesystem fallback)
├── tree.rs     # Directory tree viewing
├── file.rs     # File skeleton viewing
├── symbol.rs   # Symbol lookup and rendering
└── lines.rs    # Line range viewing
```

## Config

In `.moss/config.toml`:

```toml
[view]
depth = 1              # Default depth
line_numbers = true    # Show line numbers by default
show_docs = false      # Show full docstrings by default
```

## Use Cases

### Replace Read tool
```bash
# Instead of Read tool for specific lines
moss view src/main.rs:10-50

# Instead of Read for file overview
moss view src/main.rs --types-only
```

### Explore codebase structure
```bash
moss view .                    # Project tree
moss view src/ -d 2            # Deeper tree with symbols
moss view . --type function    # All functions
```

### Navigate to symbol
```bash
moss view Config               # Find Config anywhere
moss view config.rs/Config     # Config in specific file
moss view Config/new           # Method within class
```
