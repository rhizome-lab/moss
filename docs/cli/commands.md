# CLI Commands

## Command Philosophy

Moss has **three core primitives**:

| Primitive | Purpose | Think of it as |
|-----------|---------|----------------|
| `view` | See/find nodes | "What's there?" |
| `edit` | Modify nodes | "Change this" |
| `analyze` | Compute properties | "What's wrong?" |

**Why three?** Complexity grows exponentially with combinations. 3 composable primitives with filters (`--type`, `--depth`, `--calls`) are simpler than 30 specialized tools. The entire interface fits in working memory.

**Aliases**: These all route to `view`:
- `search`, `find`, `grep`, `query`, `locate`, `lookup`

Content search (`grep "pattern"`) should eventually become `view --contains "pattern"`. Currently the Rust CLI has a separate `grep` command for performance, but conceptually it's a view filter.

**Folded into analyze**: Former standalone commands are now analyze flags:
- `--health` - codebase health metrics (also in Rust CLI)
- `--summary` - generate file/directory summary
- `--check-docs` - check documentation freshness
- `--check-todos` - check TODO.md accuracy

---

## moss synthesize

Synthesize code from a specification.

```bash
moss synthesize <description> [options]
```

### Arguments

| Argument | Description |
|----------|-------------|
| `description` | Natural language description of what to synthesize |

### Options

| Option | Description |
|--------|-------------|
| `--type`, `-t` | Type signature (e.g., `"(int, int) -> int"`) |
| `--example`, `-e` | Input/output example (can be repeated) |
| `--constraint`, `-c` | Constraint to satisfy (can be repeated) |
| `--test` | Test code to validate against (can be repeated) |
| `--dry-run` | Show decomposition without synthesizing |
| `--show-decomposition` | Show subproblems during synthesis |
| `--preset` | Configuration preset (default/research/production/minimal) |
| `--json` | Output as JSON |
| `--verbose`, `-v` | Verbose output |

### Examples

```bash
# Basic synthesis
moss synthesize "Create a function that adds two numbers"

# With type signature
moss synthesize "Sort a list" --type "List[int] -> List[int]"

# With examples
moss synthesize "Reverse a string" \
    --example "hello" "olleh" \
    --example "world" "dlrow"

# Dry run to see decomposition
moss synthesize "Build a REST API for users" --dry-run

# JSON output
moss synthesize "Add numbers" --json | jq .code
```

### Presets

| Preset | Description |
|--------|-------------|
| `default` | Balanced settings |
| `research` | More iterations, deeper search |
| `production` | Strict validation, conservative |
| `minimal` | Fast, shallow search |

```bash
moss synthesize "Complex task" --preset research
```

## moss view

View codebase nodes: directories, files, or symbols.

```bash
moss view [target] [options]
```

### Options

| Option | Description |
|--------|-------------|
| `--depth`, `-d` | Expansion depth (0=names, 1=signatures, 2=children) |
| `--deps` | Show dependencies (imports/exports) |
| `--calls` | Show callers of target |
| `--called-by` | Show what target calls |
| `--type` | Filter by symbol type (class, function, method) |
| `--all` | Full depth expansion |

### Examples

```bash
# Show project tree
moss view

# View file skeleton (fuzzy paths work)
moss view dwim.py

# View specific symbol
moss view dwim.py/resolve_core_primitive

# View with dependencies
moss view src/moss/cli.py --deps

# Find callers
moss view --calls my_function
```

## moss edit

Structural code modifications.

```bash
moss edit <target> [options]
```

### Options

| Option | Description |
|--------|-------------|
| `--delete` | Remove the target node |
| `--replace` | Replace with new content |
| `--before` | Insert before target |
| `--after` | Insert after target |
| `--prepend` | Add to start of container |
| `--append` | Add to end of container |
| `--dry-run` | Preview without applying |

### Examples

```bash
# Delete a function
moss edit src/foo.py/old_func --delete

# Replace a class
moss edit src/foo.py/MyClass --replace "class MyClass: pass"

# Add import at top
moss edit src/foo.py --prepend "import logging"
```

## moss analyze

Analyze codebase health, complexity, and security.

```bash
moss analyze [target] [options]
```

### Options

| Option | Description |
|--------|-------------|
| `--health` | Codebase health metrics |
| `--complexity` | Cyclomatic complexity per function |
| `--security` | Security vulnerability scanning |
| `--summary` | Generate file/directory summary |
| `--check-docs` | Check documentation freshness |
| `--check-todos` | Check TODO.md accuracy |
| `--strict` | Exit 1 on warnings (for --check-docs/--check-todos) |

### Examples

```bash
# Full analysis
moss analyze

# Just complexity
moss analyze --complexity

# Analyze specific file
moss analyze src/moss/cli.py --security

# Summarize a file
moss analyze src/moss/cli.py --summary

# Check documentation freshness
moss analyze --check-docs

# Check TODOs (strict mode for CI)
moss analyze --check-todos --strict
```

## Output Formats

All commands support these output flags:

| Flag | Description |
|------|-------------|
| `--json` | Output as JSON |
| `--jq EXPR` | Filter JSON with jq expression |
| `--pretty` | Human-friendly output with colors |
| `--compact` | LLM-optimized output (no colors, minimal decoration) |

### JSON Lines Output

There's no `--jsonl` flag. Use `--jq` instead:

```bash
# Get one JSON object per text-search match
moss text-search "pattern" --jq '.matches[]'

# Get one JSON object per todo item
moss todo list --jq '.items[]'

# Extract just file paths from matches
moss text-search "TODO" --jq '.matches[].file'

# Postprocess each match with pipe to jq
moss text-search "pattern" --jq '.matches[]' | jq -r '.file + ":" + (.line|tostring)'

# Filter matches to specific directory
moss text-search "error" --jq '[.matches[] | select(.file | startswith("src/"))]'
```

**Why no `--jsonl`?** Adding it would require `--jql` for consistency (same reasons we have `--jq`: discoverability, convenience, performance). The tiny discoverability gain isn't worth doubling the JSON-related flags.

## Environment Variables

| Variable | Description |
|----------|-------------|
| `MOSS_INDEX_DIR` | Custom location for moss data/index. Absolute path uses that directory directly. Relative path uses `$XDG_DATA_HOME/moss/<relative>`. Default: `.moss` in project root |
| `MOSS_CONFIG` | Path to config file (default: `moss.toml`) |
| `MOSS_LOG_LEVEL` | Logging level (DEBUG, INFO, WARNING, ERROR) |
| `ANTHROPIC_API_KEY` | API key for Anthropic LLM |
| `OPENAI_API_KEY` | API key for OpenAI LLM |

## Configuration File

Create `moss.toml` in your project root:

```toml
[synthesis]
max_depth = 5
max_iterations = 50
parallel_subproblems = true

[synthesis.generators]
enabled = ["template", "llm"]

[synthesis.llm]
provider = "anthropic"
model = "claude-sonnet-4-20250514"
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Synthesis failed |
| 2 | Invalid arguments |
| 3 | Configuration error |
