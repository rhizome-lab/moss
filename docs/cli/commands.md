# CLI Commands

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

## moss edit

Apply structural edits to code (coming soon).

```bash
moss edit <file> <instruction>
```

## moss view

Extract structural views from code (coming soon).

```bash
moss view <file> [--skeleton|--cfg|--deps]
```

## Environment Variables

| Variable | Description |
|----------|-------------|
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
