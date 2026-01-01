# moss analyze

Analyze codebase quality: health, complexity, security, duplicates, hotspots.

## Subcommands

| Subcommand | Description |
|------------|-------------|
| (none) | Default: health analysis |
| `health` | File counts, complexity stats, large file warnings |
| `overview` | Comprehensive project summary with grade |
| `complexity` | Cyclomatic complexity analysis |
| `length` | Function length analysis |
| `security` | Security vulnerability patterns |
| `docs` | Documentation coverage |
| `files` | Longest files in codebase |
| `hotspots` | Git history hotspots (frequently changed files) |
| `duplicate-functions` | Detect code clones |
| `duplicate-types` | Detect similar type definitions |
| `trace` | Trace value provenance for a symbol |
| `callers` | Show what calls a symbol |
| `callees` | Show what a symbol calls |
| `lint` | Run configured linters |
| `check-refs` | Check documentation for broken links |
| `stale-docs` | Find docs with stale code references |
| `check-examples` | Check example references in docs |
| `all` | Run all analysis passes with overall grade |

## Examples

```bash
# Quick health check
moss analyze

# Comprehensive overview
moss analyze overview

# Find complex functions
moss analyze complexity --threshold 15

# Security scan
moss analyze security

# Find code duplicates
moss analyze duplicate-functions

# Git hotspots (frequently changed files)
moss analyze hotspots

# Trace a symbol's data flow
moss analyze trace parse_config

# Call graph
moss analyze callers handle_request
moss analyze callees main
```

## Options

### Global
- `-r, --root <PATH>` - Root directory
- `--json` - Output as JSON
- `--jq <EXPR>` - Filter JSON with jq
- `--pretty` - Human-friendly output
- `--exclude <PATTERN>` - Exclude paths
- `--only <PATTERN>` - Include only paths

### Subcommand-specific

**complexity:**
- `-t, --threshold <N>` - Only show functions above threshold
- `--kind <TYPE>` - Filter by: function, method

**files / hotspots:**
- `--allow <PATTERN>` - Add pattern to allow file
- `--reason <TEXT>` - Reason for allowing (with --allow)
- `-n, --limit <N>` - Number of results to show

**duplicate-functions:**
- `--elide-identifiers` - Ignore identifier names when comparing (default: true)
- `--elide-literals` - Ignore literal values when comparing
- `--show-source` - Show source code for duplicates
- `--min-lines <N>` - Minimum function lines to consider
- `--allow <LOCATION>` - Add to allow file
- `--reason <TEXT>` - Reason for allowing

**trace:**
- `--target <FILE>` - Target file to search in
- `--max-depth <N>` - Maximum trace depth (default: 10)
- `--recursive` - Trace into called functions

## Allow Files

Patterns can be excluded via `.moss/` allow files:

| File | Purpose |
|------|---------|
| `.moss/large-files-allow` | Exclude from `analyze files` |
| `.moss/hotspots-allow` | Exclude from `analyze hotspots` |
| `.moss/duplicate-functions-allow` | Exclude from duplicate detection |
| `.moss/duplicate-types-allow` | Exclude type pairs |

Add via CLI:
```bash
moss analyze files --allow "**/generated/*.rs" --reason "generated code"
moss analyze hotspots --allow "CHANGELOG.md" --reason "expected to change often"
```

## Config

In `.moss/config.toml`:

```toml
[analyze]
threshold = 10           # Default complexity threshold
compact = false          # Compact overview output
health = true            # Run health by default
complexity = true        # Run complexity by default
security = true          # Run security by default
duplicate_functions = false
exclude_interface_impls = true  # Exclude trait impls from doc coverage
hotspots_exclude = ["*.lock", "CHANGELOG.md"]

[analyze.weights]
health = 1.0
complexity = 0.5
security = 2.0
duplicate_functions = 0.3
```

## Module Structure

```
analyze/
├── mod.rs        # Main dispatch, config
├── args.rs       # CLI argument definitions
├── report.rs     # Report formatting, grading
├── health.rs     # Health analysis
├── complexity.rs # Complexity metrics
├── security.rs   # Security patterns
├── files.rs      # File length analysis
├── hotspots.rs   # Git hotspots
├── duplicates.rs # Code clone detection
├── trace.rs      # Value provenance tracing
├── call_graph.rs # Caller/callee analysis
├── docs.rs       # Documentation coverage
├── lint.rs       # Linter integration
└── ...
```
