# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs.

## Next Up

- Complete daemon integration
- Streaming --jq support for sessions command (currently loads all files into memory)
- Tree view remaining:
  - Smart depth: boilerplate_dirs defined but not yet applied to depth calculation
  - Per-directory config (.moss/tree.toml or similar)

Test Status: 65 passing, 0 failing

## Backlog

**Language Support Refactor** (see `docs/language-support.md` for full design):

Phase 1 - Scaffold: ✅
- [x] Create `crates/moss-languages/` with Cargo.toml, feature flags
- [x] Define `Language` trait in `traits.rs`
- [x] Set up registry with `OnceLock` + `#[cfg(feature)]` gating

Phase 2 - Port existing languages: ✅
- [x] Port Python (most complex: docstrings, async, visibility)
- [x] Port Rust (impl blocks, doc comments, visibility modifiers)
- [x] Port JavaScript/TypeScript/TSX (shared extractor)
- [x] Port Go, Java, C, C++, Ruby, Scala, Vue
- [x] Port config formats: JSON, YAML, TOML, Markdown

Phase 3 - Integrate: ✅
- [x] Add trait infrastructure to `skeleton.rs` (extract_with_trait, convert_symbol)
- [x] Improve trait impls to match legacy behavior (Rust impl blocks, Go types, Java visibility)
- [x] Migrate languages to trait-based extraction:
  - Python, JavaScript, TypeScript, Rust, Go, Java, Ruby, C, C++
  - Scala, Markdown, JSON, YAML, TOML
  - Vue remains on legacy (needs script element parsing)
- [x] Add extract_imports/extract_exports to Language trait
- [x] Refactor `deps.rs` to use trait (Python, Rust, JS, Go migrated)
- [x] Refactor `complexity.rs` to use trait (complexity_nodes method)
- [x] Refactor `symbols.rs` to use trait
- [x] Refactor `anchors.rs` to use trait
- [x] Refactor `scopes.rs` to use trait (add scope_creating_kinds)
- [x] Refactor `edit.rs` to use trait (uses function_kinds/container_kinds)
- [x] Refactor `cfg.rs` to use trait (add control_flow_kinds)
- [x] Delete legacy code from symbols.rs, skeleton.rs, deps.rs (~2000 lines removed)
- [x] Refactor `index.rs` to use trait-based import extraction
- [x] Complete C++ language support (scope/control/complexity/nesting kinds)
- [x] Add `ImportResolver` trait for external package resolution (resolution.rs)
- [x] Migrate main.rs callers to use ImportResolver trait

Phase 4 - Expand:
- [ ] Kotlin, Swift, Dart (mobile)
- [ ] C#, F# (.NET)
- [ ] PHP, Elixir, Erlang (backends)
- [ ] Zig, Lua (systems/gamedev)
- [ ] SQL, GraphQL (data)
- [ ] Dockerfile, HCL (infra)
- [ ] Svelte, SCSS (frontend)

**CLI Redundancy:** See `docs/llm-code-consistency.md` for full analysis. Key actions:
- [x] Rust: extract file resolution helper (resolve_and_read in path_resolve.rs)
- [x] Python: standardize directory arg to -C (was mix of -C and -d)
- [ ] Rust: OutputFormatter trait for JSON/text output
- [ ] Python: output helpers for JSON/markdown/compact

**CLI Surface Cleanup** ✅ (align with three-primitive philosophy):
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
- CLI reduced from 29 to 9 commands (-5000+ lines)

**Bugs:**
- [x] `view --calls`/`--called-by` semantics were swapped - FIXED
- [x] Output had unnecessary trailing "(caller)" on every line - FIXED
- Call graph shows same-named method on different object as self-call (e.g., `suggest_tool` calls `router.suggest_tool`)

**Performance:**
- Investigate slow `moss analyze --health` (+500ms over baseline, not uv startup)

**Integration:**
- LSP refactor actions (rename symbol across files)
- Cross-language reference tracking (Python ↔ Rust)

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
