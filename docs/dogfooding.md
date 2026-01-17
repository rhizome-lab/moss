# Dogfooding Findings

Using moss on itself. Patterns and observations from CLI usage.

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
