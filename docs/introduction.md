# Moss

**Structural code intelligence for humans and AI agents.**

Moss provides tools for understanding, navigating, and modifying code at a structural level (AST, control flow, dependencies) rather than treating code as text.

## Quick Start

```bash
# Build from source
git clone https://github.com/pterror/moss
cd moss
cargo build --release

# Or with nix
nix develop
cargo build --release

# View a file's structure
moss view src/main.rs

# Analyze codebase health
moss analyze --health

# Search for a symbol
moss view SkeletonExtractor
```

## Three Primitives

| Command | Purpose | Example |
|---------|---------|---------|
| `view` | See structure | `moss view src/` `moss view MyClass` |
| `edit` | Modify code | `moss edit src/foo.rs/func --delete` |
| `analyze` | Compute metrics | `moss analyze --complexity` |

See [Primitives Spec](primitives-spec.md) for full documentation.

## Key Features

- **98 Languages** - Tree-sitter grammars for comprehensive language support
- **Structural Editing** - AST-based code modifications with fuzzy matching
- **Lua Workflows** - Scriptable automation with `auto{}` LLM driver
- **Background Indexing** - Daemon maintains symbol/call graph index
- **Shadow Git** - Hunk-level edit tracking in `.moss/.git`

## Architecture

```
moss view/edit/analyze     CLI commands
        │
        ▼
    FileIndex              SQLite symbol/call graph
        │
        ▼
  SkeletonExtractor        AST → structured output
        │
        ▼
   GrammarLoader           Dynamic .so grammar loading
```

## Configuration

Create `.moss/config.toml`:

```toml
[index]
enabled = true

[view]
depth = 1
line_numbers = false

[filter.aliases]
tests = ["**/test_*.py", "**/*_test.go"]
```

## Documentation

- [Philosophy](philosophy.md) - Design tenets and principles
- [Primitives Spec](primitives-spec.md) - view, edit, analyze reference
- [Language Support](language-support.md) - 98 supported languages
- [Lua Workflows](workflow-format.md) - Automation with Lua scripts
