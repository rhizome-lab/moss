# Moss

Fast code intelligence CLI. Provides structural awareness of codebases through AST-based analysis.

## Install

```bash
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/pterror/moss/master/install.sh | bash

# From source
cargo install --path crates/moss-cli

# Or build locally
cargo build --release
```

## Quick Start

```bash
# View project structure
moss view

# View a specific file's symbols
moss view src/main.rs

# View a specific symbol
moss view src/main.rs/main

# Analyze codebase health
moss analyze --health

# Search for text patterns
moss text-search "TODO"

# Run linters
moss lint
```

## Commands

### view - Navigate Code Structure

View directories, files, and symbols as a unified tree:

```bash
moss view                       # Current directory tree
moss view src/                  # Specific directory
moss view src/main.rs           # File with symbols
moss view src/main.rs/MyClass   # Specific symbol
moss view src/main.rs -d 2      # Depth 2 (show nested symbols)
moss view --full src/foo.rs/bar # Full source code of symbol
moss view --deps src/foo.rs     # Show imports/exports
moss view --focus src/foo.rs    # Resolve and show imported symbols
```

### analyze - Codebase Analysis

Unified analysis with multiple modes:

```bash
moss analyze                    # Health + complexity + security
moss analyze --health           # Codebase metrics and health score
moss analyze --complexity       # Cyclomatic complexity report
moss analyze --security         # Security vulnerability scan
moss analyze --overview         # Comprehensive project overview
moss analyze --lint             # Run all detected linters
moss analyze --hotspots         # Git history analysis (churn + complexity)
moss analyze --storage          # Index and cache sizes
```

### lint - Run Linters

Unified interface to linters, formatters, and type checkers:

```bash
moss lint                       # Auto-detect and run relevant tools
moss lint --fix                 # Auto-fix where possible
moss lint --watch               # Watch mode with debounce
moss lint --sarif               # Output in SARIF format
moss lint --category type       # Only type checkers
moss lint --tools ruff,clippy   # Specific tools
moss lint --list                # List available tools
```

Supported tools: ruff, clippy, rustfmt, oxlint, biome, prettier, tsc, mypy, pyright, eslint, gofmt, go-vet, deno-check, and more.

### text-search - Search Code

Fast ripgrep-based search:

```bash
moss text-search "pattern"            # Search all files
moss text-search "TODO" --only "*.rs" # Filter by extension
moss text-search "fn main" -i         # Case insensitive
moss text-search "error" --limit 50   # Limit results
```

### package - Package Management

Query package registries and analyze dependencies:

```bash
moss package info tokio         # Package info from registry
moss package list               # List project dependencies
moss package tree               # Dependency tree
moss package outdated           # Check for updates
moss package why tokio          # Why is this dependency included?
moss package audit              # Security vulnerability scan
```

Supports: Cargo, npm, pip, Go modules, Bundler, Composer, Hex, Maven, NuGet, Nix, Conan.

### serve - Server Modes

Run moss as a server for integration:

```bash
moss serve mcp                  # MCP server for LLM tools (stdio)
moss serve http --port 8080     # REST API server
moss serve lsp                  # LSP server for IDEs
```

#### HTTP API Endpoints

- `GET /health` - Server status
- `GET /files?pattern=foo` - Search files
- `GET /files/*path` - File info with symbols
- `GET /symbols?name=foo` - Search symbols
- `GET /symbols/:name` - Symbol source code
- `GET /search?q=foo` - Combined search

#### LSP Capabilities

- Document symbols
- Workspace symbol search
- Hover (signature + docstring)
- Go to definition
- Find references

### index - Manage Index

Control the file and symbol index:

```bash
moss index status               # Index stats
moss index refresh              # Refresh file index
moss index reindex              # Full reindex
moss index reindex --call-graph # Include call graph
```

### workflow - TOML Workflows

Run scripted workflows defined in `.moss/workflows/`:

```bash
moss workflow list              # List available workflows
moss workflow run build         # Run a workflow
moss workflow show build        # Show workflow definition
```

### sessions - Session Analysis

Analyze Claude Code and other agent session logs:

```bash
moss sessions                   # List recent sessions
moss sessions <id>              # Show session details
moss sessions <id> --analyze    # Full session analysis
```

## Configuration

### Custom Lint Tools

Add custom tools in `.moss/tools.toml`:

```toml
[[tools]]
name = "my-linter"
command = ["my-linter", "--format", "json"]
category = "linter"
languages = ["python"]

# Parse output as SARIF or line-based
output_format = "sarif"  # or "lines"
```

### Workflows

Define workflows in `.moss/workflows/*.toml`:

```toml
name = "build"
description = "Build and test"

[[steps]]
name = "check"
command = ["cargo", "check"]

[[steps]]
name = "test"
command = ["cargo", "test"]
depends_on = ["check"]
```

## Output Formats

Most commands support `--json` for structured output:

```bash
moss view src/main.rs --json
moss analyze --health --json
moss lint --json
```

## Language Support

Moss supports 98 languages via tree-sitter grammars including:
Python, Rust, TypeScript, JavaScript, Go, Java, C, C++, Ruby, PHP, Swift, Kotlin, Scala, and many more.

## Development

```bash
# Build
cargo build

# Test
cargo test

# Install locally
cargo install --path crates/moss-cli
```

## License

MIT
