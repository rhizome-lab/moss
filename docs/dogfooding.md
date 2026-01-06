# Dogfooding Findings

Using moss on itself. Patterns and observations from both CLI usage and agent runs.

## Agent Dogfooding

### What Works

- **Ephemeral context model**: 1-turn output visibility forces explicit memory curation via `$(keep)`/`$(note)`
- **State machine (explorer/evaluator)**: Prevents pre-answering; evaluator curates, explorer acts
- **Session logging**: JSONL logs in `.moss/agent/logs/` enable post-hoc analysis
- **`moss sessions --format moss`**: List/grep agent sessions for debugging
- **Working memory**: Synthesized notes (`$(note)`) survive longer than raw outputs

### Friction Points Discovered

- **View loops**: Agent can view same files repeatedly without extracting info (7× same file, 15 turns, incomplete)
  - Cause: `view` output doesn't contain needed info directly
  - Pattern: succeeds when tool output = answer, struggles when output requires interpretation
- **Text-search syntax confusion**: Agent used grep syntax (`\|`) with text-search despite tool being renamed
  - Shows agents don't understand tool semantics, just syntax
- **Large file edits**: Edit tool match failures on large deletions
- **Context compaction**: Claude Code's auto-compaction lost in-progress work (moss's dynamic reshaping avoids this)

### Key Insights

- **Role framing beats instructions**: "You are an EVALUATOR" + banned phrases + examples beats instruction-only
- **Concrete examples prevent defaults**: Example in prompt prevents LLM defaulting to XML function calls
- **Context uniqueness**: Identical context between any two LLM calls risks loops
- **Cross-project parallelization**: Running separate Claude Code sessions per project avoids within-project coordination costs

### Session Analysis Workflow

```bash
moss sessions --format moss                    # list recent agent sessions
moss sessions --format moss --grep "benchmark" # filter by content
moss sessions <id> --analyze                   # full analysis
```

## View Primitive

**What Works:**
- Fuzzy path resolution: `view skeleton.rs` finds `crates/moss/src/skeleton.rs`
- Symbol paths: `view skeleton.rs/SkeletonExtractor`
- Underscore/hyphen equivalence: `moss-api` matches `moss_api`
- `--depth` controls expansion level
- `--json` for structured output

**Gaps:**
- No `--types-only` filter (would show only structs/enums/traits)
- No `--fisheye` mode (show imported module signatures)

## Analyze Primitive

**What Works:**
- `--health`: codebase metrics (files, lines, complexity summary, grade)
- `--complexity`: per-function cyclomatic complexity with risk levels
- Threshold filter: `-t 10` shows only functions above threshold
- Works without index (filesystem fallback)

**Gaps:**
- Security scanning not integrated (bandit, semgrep, cargo-audit)

## Investigation Flow

Effective pattern for understanding unfamiliar code:

1. `moss view .` - get tree structure
2. `moss view <file>` - file skeleton with symbols
3. `moss analyze --complexity` - find complex areas
4. `moss view <file>/<symbol>` - drill into specific symbol

## Token Efficiency

- Use shallow depth (`--depth 1`) for overview
- Symbol paths (`file/Symbol`) avoid loading entire file
- `--json | jq` for extracting specific fields
- Health check before diving into specifics

## CLI Conventions

**Global flags:**
- `--json` for structured output
- `--pretty` for human-friendly display (tables, colors, alignment)
- Default is token-efficient (minimal decoration)
- All commands work with fuzzy paths

## Future Improvements

- `--types-only` filter for architectural overview
- `--fisheye` to show imported dependencies inline
- `--visibility public|all` filter
- Cross-language import resolution (Rust `use`, TypeScript `import`)
