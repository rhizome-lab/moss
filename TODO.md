# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs.

## Next Up

Test Status: 113 passing, 0 failing (moss)

## Remaining Work

**Configuration System:**
Sections: `[daemon]`, `[index]`, `[filter.aliases]`, `[todo]`, `[view]`, `[analyze]`, `[grep]`

Adding a new section (3 places):
1. Define `XxxConfig` struct with `#[derive(Merge)]` + `XxxArgs` with `#[derive(Args)]` in command module
2. Add field to MossConfig
3. Add `run(args, json)` function that loads config and merges

Candidates: `[workflow]` (directory, auto-run), `[serve]` (port, host)

**Workflow Engine:**
- [x] Port LLM calling logic (streaming, tool use) as workflow component
- [x] Memory system: `store()`, `recall()`, `forget()` Lua API with SQLite persistence

**Token-Efficient Output:**
- [x] Default output optimized for LLM context (compact mode)
- [x] `--pretty` for human-friendly display with colors
- [x] Elide keywords in compact mode (`pub fn` → just signature)

**Rust Redesign Candidates:**
- Rules engine: consider semgrep/ruff integration instead of custom
- Plugin system: Rust trait-based plugins or external tool orchestration
- Edit routing: workflow engine with LLM decision points
- Session/checkpoint: workflow state persistence
- PR/diff analysis: `moss analyze --pr` or similar
## Backlog

**Language Support:** 98 languages implemented - all arborium grammars covered.
See `docs/language-support.md` for design. Run `scripts/missing-grammars.sh` to verify.

**Grammar Loading (external .so files):**
Status: Implemented. `cargo xtask build-grammars` compiles 97 grammars to .so files (~142MB total).
- Grammars load from: `MOSS_GRAMMAR_PATH` env var, `~/.config/moss/grammars/`
- See `crates/moss-languages/src/grammar_loader.rs` for loader implementation
- [x] `moss grammars install` downloads from GitHub releases
- [x] Release workflow builds and packages grammars per platform

**Workflow Engine:**
- Consider streaming output for `auto{}` driver
- JSON Schema for complex action parameters (currently string-only)

**Performance:**
- [x] View command performance: fixed by lazy symbol search + auto-build index (470ms → 14ms for file view, 47ms for symbol search)

**Code Quality:**
- Validate node kinds against grammars: `validate_unused_kinds_audit()` in each language file ensures documented unused kinds stay in sync with grammar
- Directory context: attach LLM-relevant context to directories (like CLAUDE.md but hierarchical)
- Deduplicate SQL queries in moss: many ad-hoc queries could use shared prepared statements or query builders (needs design: queries use different execution contexts - Connection vs Transaction)
- Detect reinvented wheels: hand-rolled JSON/escaping when serde exists, manual string building for structured formats, reimplemented stdlib. Heuristics unclear. Full codebase scan impractical. Maybe: (1) trigger on new code matching suspicious patterns, (2) index function signatures and flag known anti-patterns, (3) check unused crate features vs hand-rolled equivalents. Research problem.
- [x] Clone detection (`moss analyze --clones`): implemented with on-demand AST hashing, `--elide-identifiers` (default true), `--elide-literals` (default false). No index storage - computed at scan time.
- [x] Binary size optimization: LTO + strip reduced 25MB → 18MB (main contributors: moss 2.1MB, bundled C libs 2MB, moss_languages 1.6MB)
- [x] Avoid Command::new in crates/moss-packages/src/ecosystems/: replaced 14 curl calls with ureq HTTP client. bun.lockb properly ported from Bun source (inline + external strings). Remaining Command uses are legitimate CLI tools: npm/cargo/pip-audit/bundle-audit/govulncheck (security audits), nix/nix-env (local queries)

**Daemon Design:**
- [x] Multi-codebase: single global daemon at `~/.config/moss/daemon.sock`, manages multiple roots via `daemon add/remove/list`
- [x] Minimal memory footprint: SQLite file-based (~2MB per connection), no LRU eviction needed

**Tooling:**
- Multi-file batch edit: less latency than N sequential edits. Not for identical replacements (use sed) or semantic renames (use LSP). For structured batch edits where each file needs similar-but-contextual changes (e.g., adding a trait method to 35 language files).
- Interactive config editor: `moss config` TUI for editing `.moss/config.toml`
- Todo archive workflow: `moss todo list --done` → write to CHANGELOG → `moss todo clean`. Options:
  - `moss todo archive` command: formats done items for changelog, removes them
  - Lua script works locally but not with external agents (Claude Code, etc.)
  - MCP tool could expose archive action for agent integration
  - Consider: `--format=changelog` to output in changelog format for review before commit

**Workspace/Context Management:**
- Persistent workspace concept (like Notion): files, tool results, context stored permanently
- Cross-session continuity without re-reading everything
- Investigate memory-mapped context, incremental updates

**Agent Research:**
- Conversational loop pattern (vs hierarchical)
- YOLO mode evaluation
- Diffusion-like parallel refactors
- Claude Code over-reliance on Explore agents: spawns agents for direct tool tasks. Symptom of deeper issue?
- Session analysis: detect correction patterns ("You're right", "Good point", "Fair point", "Should have", "Right -", "isn't working")
- LLM code consistency: see `docs/llm-code-consistency.md` for research notes
- Analyze long chains of uninterrupted tool calls (friction indicator)
- Claude Code lacks navigation: clicking paths/links in output doesn't open them in editor (significant UX gap)
- Rich links in LLM output: either LLM outputs structured links (file:line, TODO items, symbols) or cheap model postprocesses response to extract them. Would enable clickable references in terminal/IDE.

**Distribution:**
- Wrapper packages for ecosystems: npm, PyPI, Homebrew, etc.
  - Auto-generate and publish in sync with GitHub releases
  - Single binary + thin wrapper scripts per ecosystem
- Direct download: platform-detected link to latest GitHub release binary (avoid cargo install overhead)

**Vision (Aspirational):**
- [x] Shadow Git: hunk-level edit tracking in `.moss/.git` (see `workflow/shadow.rs`, Lua API: `shadow.*`)
  - TODO: auto-track all edits made via `moss edit` / workflow edit tools
  - TODO: `[shadow]` config section (enabled, retention policy, deletion warnings)
- Verification Loops: domain-specific validation (compiler, linter, tests) before accepting output
- Synthesis: decompose complex tasks into solvable subproblems (`moss synthesize`)
- Plugin Architecture: extensible view providers, synthesis strategies, code generators

## Deferred

- VS Code extension: test and publish to marketplace (after first CLI release)
- Remaining docs: prior-art.md, hybrid-loops.md

## Python Features Not Yet Ported

**Orchestration:**
- Session management with checkpointing
- Driver protocol for agent decision-making
- Plugin system (partial - Rust traits exist)
- Event bus, validators, policies
- PR review, diff analysis
- TUI (Textual-based explorer)
- DWIM tool routing with aliases

**LLM-Powered:**
- Edit routing (complexity assessment → structural vs LLM)
- Summarization with local models
- Working memory with summarization

**Memory System:**
See `docs/design/memory.md`. Core API: `store(content, opts)`, `recall(query)`, `forget(query)`.
SQLite-backed persistence in `.moss/memory.db`. Slots are user-space (metadata), not special-cased.

**Local NN Budget (from deleted docs):**
| Model | Params | FP16 RAM |
|-------|--------|----------|
| all-MiniLM-L6-v2 | 33M | 65MB |
| distilbart-cnn | 139M | 280MB |
| T5-small | 60M | 120MB |

Pre-summarization tiers: extractive (free) → small NN → LLM (expensive)

**Usage Patterns (from dogfooding):**
- Investigation flow: `view .` → `view <file> --types-only` → `analyze --complexity` → `view <symbol>`
- Token efficiency: use `--types-only` for architecture, `--depth` sparingly

## Implementation Notes

**Self-update (`moss update`):**
- Now in commands/update.rs
- GITHUB_REPO constant → "pterror/moss"
- Custom SHA256 implementation (Sha256 struct)
- Expects GitHub release with SHA256SUMS.txt

## When Ready

**First Release:**
```bash
git tag v0.1.0
git push --tags
```
- Verify cross-platform builds in GitHub Actions
- Test `moss update` against real release
