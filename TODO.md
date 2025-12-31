# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs.

## Next Up

- Lua test discovery: `moss script test` or similar to run `.moss/tests/*.lua`
- Edit `--item` flag: format-aware insertion for lists (checkbox/bullet/numbered)
- Analyze `--trace`: recursive cross-function tracing (trace INTO called functions)

## Remaining Work
- Unified tree: semantic entry points already work (`moss view SymbolName` finds it)
  - Consider: namespace-qualified lookups (`moss view std::vector`, `moss view com.example.Foo`)
  - Requires language-specific namespace semantics - low priority
- Shadow worktree isolation: git worktree or overlayfs for parallel validation
  - Store diffs in memory, use worktree as "materialized view"
  - Apply patch to worktree → run validator → if pass, apply to user dir
  - Zero user interruption (user can edit while agent tests in background)
- `analyze --trace <symbol>`: backward data flow / value provenance
  - [x] Trace where a value comes from (like "blame" for values, not lines)
  - [x] Same-file function signatures at call boundaries
  - [x] Conditionals: branch context shown (if/else/match)
  - [x] Stop conditions: literals marked as (terminal)
  - [x] Stop conditions: same-file calls show signature + @L<line>
  - [x] `--max-depth N` limits output
  - [ ] Recursive cross-function tracing (trace INTO called functions)
  - [ ] Cross-file function lookup via index

### Configuration System
Sections: `[daemon]`, `[index]`, `[filter.aliases]`, `[todo]`, `[view]`, `[analyze]`, `[grep]`

Adding a new section (3 places):
1. Define `XxxConfig` struct with `#[derive(Merge)]` + `XxxArgs` with `#[derive(Args)]` in command module
2. Add field to MossConfig
3. Add `run(args, json)` function that loads config and merges

Candidates: `[workflow]` (directory, auto-run), `[serve]` (port, host)


### Rust Redesign Candidates
- Rules engine: consider semgrep/ruff integration instead of custom
- Plugin system: Rust trait-based plugins or external tool orchestration
- Edit routing: workflow engine with LLM decision points
- Session/checkpoint: workflow state persistence
- PR/diff analysis: `moss analyze --pr` or similar

## Backlog

### Workflow Engine
- Consider streaming output for `auto{}` driver
- JSON Schema for complex action parameters (currently string-only)

### View Command
- Smart Header: optionally pull in referenced types as context (show type definitions used by the symbol)
- `moss grep` naming collision with shell grep: CLAUDE.md says use `|` not `\|` but `moss grep 'a|b'` looks like shell grep syntax. Consider renaming (not `search`/`find` - those should be semantic)

### Code Quality
- Doc coverage metric: "0% (18 of 6217)" seems wrong - check what's being counted (denominator too high? detection broken?)
- `is_source_file` function: hardcoded extension list duplicated in analyze modules - use `moss-languages` support detection instead
- Git hotspot allowlist: `.moss/hotspot-allow` file to filter expected hotspots (generated code, vendored deps) from `--hotspots`
- Large file analysis: run `./target/debug/moss analyze` on very large files (1000+ functions) - assess performance, output format, usefulness
- PR/diff analysis: `moss analyze --pr` or `--diff` for changed code focus (needs broader analysis workflow design)
- Validate node kinds against grammars: `validate_unused_kinds_audit()` in each language file ensures documented unused kinds stay in sync with grammar
- Directory context: attach LLM-relevant context to directories (like CLAUDE.md but hierarchical)
- Deduplicate SQL queries in moss: many ad-hoc queries could use shared prepared statements or query builders (needs design: queries use different execution contexts - Connection vs Transaction)
- Detect reinvented wheels: hand-rolled JSON/escaping when serde exists, manual string building for structured formats, reimplemented stdlib. Heuristics unclear. Full codebase scan impractical. Maybe: (1) trigger on new code matching suspicious patterns, (2) index function signatures and flag known anti-patterns, (3) check unused crate features vs hand-rolled equivalents. Research problem.
- Syntax-based linting: custom rules like ESLint's `no-restricted-syntax` but tree-sitter based
  - Example: `GrammarLoader::new` restricted except in `grammar_loader()` singleton
  - Example: `Query::new` restricted except in cached getters
  - Define patterns via tree-sitter queries, whitelist locations

### `@` Sigil
- As target prefix, expands to well-known paths:
  - `@todo` → detected TODO file(s), maybe `["TODO.md", "TASKS.md"]`
  - `@config` → `.moss/config.toml`
  - Works with any command: `moss view @todo`, `moss edit @config`
  - Need to figure out: what happens when some items don't match?
  - `moss init` can detect and configure this (prints "detected TODO file at TASKS.md")
- As command prefix, runs scripts:
  - `moss @script-name args` → runs `.moss/scripts/script-name.lua`
  - First scripts: `@todo` (todo viewer/editor), `@config` (config viewer/editor)
- Partial fix for file/section detection (explicit opt-in to heuristics)

### `moss todo` Future
- Currently: Rust implementation with file/section detection, format preservation
- Goal: port `todo.rs` to `@todo` script (Lua + `moss edit` primitives)
- `add`, `rm`, `done` stay for now (convenience), but room for improvement:
  - These are conceptually `moss edit` ops on markdown
  - `@todo` target prefix is the path toward unification
- `list` with filters, `clean`, `normalize` → port to Lua script (todo-specific semantics)
- Validates that view/edit primitives are sufficient for structural edits

### Script System
- TOML workflow format: structured definition (steps, actions) - **deferred until use cases are clearer**
  - Builtin `workflow` runner script interprets TOML files
  - Users can also write pure Lua scripts directly
- Lua test framework: test discovery for `.moss/tests/` (test + test.property modules done)
  - Command naming: must clearly indicate "moss Lua scripts" not general testing (avoid `@test`, `@spec`, `@check`)
  - Alternative: no special command, just run test files directly via `moss <file>`
- Type system uses beyond validation
  - Done: `T.describe(schema)` for introspection, `type.generate` for property testing
  - Future: extract descriptions from comments (LuaDoc-style) instead of `description` field
- Format libraries (Lua): json, yaml, toml, kdl - **very low priority, defer until concrete use case**
  - Pure Lua implementations preferred (simple, no deps)
  - Key ordering: sort alphabetically by default, `__keyorder` metatable field for explicit order

### Edit Improvements
- `--at primary`: explicit opt-in to primary section detection (discoverable via error message)
- `--item` flag: format-aware insertion (detects checkbox/bullet/numbered, wraps content)
- Fuzzy glob in paths: `moss edit "TODO.md/**/feature*" delete` for item matching

### Tooling
- `moss fetch`: web content retrieval for LLM context (needs design: chunking, streaming, headless browser?)
- Multi-file batch edit: less latency than N sequential edits. Not for identical replacements (use sed) or semantic renames (use LSP). For structured batch edits where each file needs similar-but-contextual changes (e.g., adding a trait method to 35 language files).
- Semantic refactoring: `moss edit <glob> --before 'fn extract_attributes' 'fn extract_attributes(...) { ... }'`
  - Insert method before/after another method across multiple files
  - Uses tree-sitter for semantic targeting (not regex)
  - `--batch` flag for multiple targets in one invocation
- Cross-file refactors: `moss move src/foo.rs/my_func src/bar.rs`
  - Move functions/types between files with import updates
  - Handles visibility changes (pub when crossing module boundaries)
  - Updates callers to use new path
- Structured config crate (`moss-config`): trait-based view/edit for known config formats (TOML, JSON, YAML, INI). Unified interface across formats. (xkcd 927 risk acknowledged)
  - Examples: .editorconfig, prettierrc, prettierignore, oxlintrc.json[c], oxfmtrc.json[c], eslint.config.js, pom.xml
  - Open: do build scripts belong here? (conan, bazel, package.json, cmake) - maybe separate `moss-build`
  - Open: linter vs formatter vs typechecker config - same trait or specialized?
  - Open: reconsider moss config format choice (TOML vs YAML, JSON, KDL) - rationalize decision

### Workspace/Context Management
- Persistent workspace concept (like Notion): files, tool results, context stored permanently
- Cross-session continuity without re-reading everything
- Investigate memory-mapped context, incremental updates

### Agent Research
- Conversational loop pattern (vs hierarchical)
- YOLO mode evaluation
- Diffusion-like parallel refactors
- LLM code consistency: see `docs/llm-code-consistency.md` for research notes
- Claude Code lacks navigation: clicking paths/links in output doesn't open them in editor (significant UX gap)
- Rich links in LLM output: structured links (file:line, symbols) or cheap model postprocessing. Clickable refs in terminal/IDE.
- Large file edits: agentic tools (Claude Code) struggle with large deletions/replacements - Edit tool fails when strings don't match exactly, requiring shell workarounds

### Session Analysis
- Better `--compact` format: key:value pairs, no tables, all info preserved
- Better `--pretty` format: bar charts for tools, progress bar for success rate
- `moss sessions stats`: cross-session aggregates (session count, token hotspots, total usage)
- `moss sessions mark <id>`: mark as reviewed (store in `.moss/sessions-reviewed`)
- Friction signal detection: correction patterns, tool chains, avoidance

### Friction Signals (see `docs/research/agent-adaptation.md`)
How do we know when tools aren't working? Implicit signals from agent behavior:
- Correction patterns: "You're right", "Should have" after tool calls
- Long tool chains: 5+ calls without acting
- Tool avoidance: grep instead of moss, spawning Explore agents
- Follow-up patterns: `--types-only` → immediately view symbol
- Repeated queries: same file viewed multiple times

### Distribution
- Wrapper packages for ecosystems: npm, PyPI, Homebrew, etc.
  - Auto-generate and publish in sync with GitHub releases
  - Single binary + thin wrapper scripts per ecosystem
- Direct download: platform-detected link to latest GitHub release binary (avoid cargo install overhead)

### Vision (Aspirational)
- Shadow Git: auto-track all edits made via `moss edit` / workflow edit tools
  - `[shadow]` config section (enabled, retention policy, deletion warnings)
- Verification Loops: domain-specific validation (compiler, linter, tests) before accepting output
- Synthesis: decompose complex tasks into solvable subproblems (`moss synthesize`)
- Plugin Architecture: extensible view providers, synthesis strategies, code generators

### Agent / MCP
- `moss @agent` (crates/moss/src/commands/scripts/agent.lua): MCP support as second-class citizen
  - Our own tools take priority, MCP as fallback/extension mechanism
  - Need to design how MCP servers are discovered/configured
- Context view management: extend/edit/remove code views already in agent context
  - Agents should be able to request "add more context around this symbol" or "remove this view"
  - Incremental context refinement vs full re-fetch
  - Blocked on: agent implementation existing at all

### CI/Infrastructure
(No current issues)

## Deferred

- VS Code extension: test and publish to marketplace (after first CLI release)
- Remaining docs: prior-art.md, hybrid-loops.md

## Python Features Not Yet Ported

### Orchestration
- Session management with checkpointing
- Driver protocol for agent decision-making
- Plugin system (partial - Rust traits exist)
- Event bus, validators, policies
- PR review, diff analysis
- TUI (Textual-based explorer)
- DWIM tool routing with aliases

### LLM-Powered
- Edit routing (complexity assessment → structural vs LLM)
- Summarization with local models
- Working memory with summarization

### Memory System
See `docs/design/memory.md`. Core API: `store(content, opts)`, `recall(query)`, `forget(query)`.
SQLite-backed persistence in `.moss/memory.db`. Slots are user-space (metadata), not special-cased.

### Local NN Budget (from deleted docs)
| Model | Params | FP16 RAM |
|-------|--------|----------|
| all-MiniLM-L6-v2 | 33M | 65MB |
| distilbart-cnn | 139M | 280MB |
| T5-small | 60M | 120MB |

Pre-summarization tiers: extractive (free) → small NN → LLM (expensive)

### Usage Patterns (from dogfooding)
- Investigation flow: `view .` → `view <file> --types-only` → `analyze --complexity` → `view <symbol>`
- Token efficiency: use `--types-only` for architecture, `--depth` sparingly

## Implementation Notes

### Self-update (`moss update`)
- Now in commands/update.rs
- GITHUB_REPO constant → "pterror/moss"
- Custom SHA256 implementation (Sha256 struct)
- Expects GitHub release with SHA256SUMS.txt

## When Ready

### First Release
```bash
git tag v0.1.0
git push --tags
```
- Verify cross-platform builds in GitHub Actions
- Test `moss update` against real release
