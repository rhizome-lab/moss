# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs.

## Next Up

- Investigate slow `moss analyze --health` (+500ms over baseline)
- Phase 5: Continue adding languages (Dart, F#, Elixir, etc.)

Test Status: 74 passing, 0 failing

## Backlog

**Language Support Refactor** (see `docs/language-support.md` for full design):

Phase 1-4: ✅ Complete (scaffold, port, integrate, remove trait defaults)

Phase 5 - Expand (new languages):
- [x] Kotlin (mobile) - full Language trait impl, Maven/Gradle resolution
- [x] C# (.NET) - classes, structs, interfaces, records, XML docs, NuGet hints
- [x] Swift (mobile) - classes, structs, protocols, enums, actors, SPM hints
- [x] PHP (backend) - PSR-4 paths, PHPDoc, Composer integration
- [x] Dockerfile (infra) - FROM stage extraction, image/alias parsing
- [ ] Dart (mobile), F# (.NET)
- [ ] Elixir, Erlang (backends)
- [ ] Zig, Lua (systems/gamedev)
- [ ] SQL, GraphQL (data)
- [ ] HCL (infra)
- [ ] Svelte, SCSS (frontend)

**CLI Redundancy:** See `docs/llm-code-consistency.md` for full analysis. Key actions:
- [x] Rust: extract file resolution helper (resolve_and_read in path_resolve.rs)
- [x] Python: standardize directory arg to -C (was mix of -C and -d)
- [ ] Rust: OutputFormatter trait for JSON/text output
- [ ] Python: output helpers for JSON/markdown/compact

**CLI Surface Cleanup** (align with three-primitive philosophy):
- [x] Removed: callers, callees (use view --calls/--called-by)
- [x] Removed: complexity (use analyze --complexity)
- [x] Removed: cfg, scopes, health (inscrutable output, use analyze)
- [x] Removed: symbols, anchors, expand, context (use view with depth/--full/--deps)
- [x] Removed: path, search-tree, deps (use view with fuzzy matching/--deps)
- [x] Removed: summarize, imports (use view --deps)
- [x] Consolidated: reindex, index-stats, list-files, index-packages → `moss index` subcommand
- [x] Consolidated: overview → `analyze --overview`
- [x] Fixed: view --deps now shows exports, doesn't show symbols
- [x] Fixed: view lists all matches when query is ambiguous
- [x] Consolidated: find-symbols → view now searches symbols too
- CLI reduced from 29 to 8 commands (-5000+ lines)
- [ ] view.rs: consolidated but messy internally - problem shifted, not solved. Needs proper unification.
- [ ] Command/subcommand/flag names should be self-documenting (no abbreviations, clear meaning)

**Code Quality:**
- Audit Rust codebase for tuple returns - replace with structs unconditionally (unless names would be pure ceremony)
  - Already fixed: `find_symbols` → `SymbolMatch`, `call_graph_stats` → `CallGraphStats`, `get_changed_files` → `ChangedFiles`
  - Also fixed: `IndexedCounts`, `CollapsedChain`, `ParsedPackage`, `ExtractedDeps`
- [x] Clean up `crates/moss-cli/src/commands/` to mirror actual command list (8 commands = 8 files)
- Directory context: attach LLM-relevant context to directories (like CLAUDE.md but hierarchical)

**Bugs:**
- [x] `view --calls`/`--called-by` semantics were swapped - FIXED
- [x] Output had unnecessary trailing "(caller)" on every line - FIXED
- Call graph shows same-named method on different object as self-call (e.g., `suggest_tool` calls `router.suggest_tool`)

**Performance:**
- Investigate slow `moss analyze --health` (+500ms over baseline, not uv startup)

**Integration:**
- LSP refactor actions (rename symbol across files)
- Cross-language reference tracking (Python ↔ Rust)

**Tooling:**
- Avoid web search for Rust crate features: `cargo info <crate> --features` or similar offline lookup

**View Filtering:**
- Filter out tests from views (--no-tests or --exclude=tests)
- Filter by category: tests, config files, build files, etc.
- Inverse: show only specific categories (--only=tests)
- Filter view children by type/name (needs design: glob patterns? symbol kinds?)

**Agent Research:**
- Conversational loop pattern (vs hierarchical)
- YOLO mode evaluation
- Diffusion-like parallel refactors
- Claude Code over-reliance on Explore agents: spawns agents for direct tool tasks. Symptom of deeper issue?
- LLM code consistency: see `docs/llm-code-consistency.md` for research notes

**Session Tooling:**
- End-of-session summary workflow (.moss/workflows/session-summary.toml, no LLM):
  - Test status: passing/failing count
  - `git diff --shortstat` (files changed, insertions, deletions)
  - Commits ahead of remote
  - Uncommitted changes summary
  - TODO.md delta (items added/completed)

## Deferred

- Driver integration improvements
- Python edit separate targeting (LLM-based)
- Remaining docs: prior-art.md, hybrid-loops.md
- Memory system: layered cross-session learning
- Agent TUI: terminal state reset after nested commands

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
