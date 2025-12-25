# Moss for LLMs

Quick reference for AI agents working with codebases using Moss.

## Quick Start

```bash
# Get project overview
moss analyze --overview

# View codebase structure
moss view

# View a specific file's symbols
moss view src/main.rs
```

## Essential Commands

| Command | Purpose | When to Use |
|---------|---------|-------------|
| `moss analyze --overview` | Project health snapshot | First thing when entering a codebase |
| `moss view src/` | Code structure (symbols, hierarchy) | Understanding architecture |
| `moss view --deps FILE` | Import/export analysis | Before modifying a file |
| `moss analyze --health` | Codebase metrics and health score | Checking project state |
| `moss grep "pattern"` | Search code | Finding usage, definitions |
| `moss package audit` | Security vulnerability scan | Checking dependencies |

## Output Modes

```bash
moss view FILE           # Human-readable tree format
moss view FILE --json    # Structured JSON output
```

JSON is useful for parsing but more verbose. Plain text is token-efficient.

## Common Workflows

**Starting work on a codebase:**
```bash
moss analyze --overview   # Quick health check
moss view                 # See structure
moss view src/            # Drill into source
```

**Before modifying a file:**
```bash
moss view --deps FILE     # What does it import/export?
moss view FILE            # What symbols are in it?
moss view FILE/ClassName  # View specific symbol
```

**Understanding a symbol:**
```bash
moss view FILE/symbol            # See signature and docstring
moss view --full FILE/symbol     # Full source code
moss analyze --calls FILE/symbol # What does it call?
moss analyze --called-by symbol  # What calls it?
```

**After making changes:**
```bash
moss lint                  # Run linters
moss analyze --lint        # Full lint analysis
moss analyze --health      # Health check
```

**Checking dependencies:**
```bash
moss package list          # Show dependencies
moss package tree          # Dependency tree
moss package audit         # Security vulnerabilities
moss package why tokio     # Why is this included?
```

**Finding code:**
```bash
moss grep "TODO"           # Search for patterns
moss grep "fn main" -i     # Case insensitive
```

## Key Commands

### view - Navigate Code

```bash
moss view                     # Project tree
moss view src/main.rs         # File symbols
moss view src/main.rs/MyClass # Specific symbol
moss view --full FILE/symbol  # Full source
moss view --deps FILE         # Dependencies
moss view -d 2                # Depth 2 (nested symbols)
```

### analyze - Analysis

```bash
moss analyze                  # Health + complexity + security
moss analyze --overview       # Comprehensive overview
moss analyze --health         # Health metrics
moss analyze --complexity     # Cyclomatic complexity
moss analyze --lint           # Run all linters
moss analyze --hotspots       # High-churn files
```

### lint - Linters

```bash
moss lint                     # Auto-detect and run
moss lint --fix               # Auto-fix issues
moss lint --list              # Available tools
```

### grep - Search

```bash
moss grep "pattern"           # Full codebase search
moss grep "TODO" --glob "*.rs"
```

## Key Insights

- `moss view` is the primary navigation command - works on dirs, files, and symbols
- `moss analyze` is the primary analysis command - health, complexity, security, lint
- `moss grep` for text search, `moss view` for structural navigation
- Use `--json` when you need to parse output programmatically
- The index (`.moss/index.sqlite`) caches symbols for fast lookups
