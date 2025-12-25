# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs.

## Next Up

- Investigate slow `moss analyze --health` (+500ms over baseline)
- view.rs internal cleanup (see CLI Surface Cleanup)
- Rust crate feature lookup (see Tooling)

Test Status: 74 passing, 0 failing

## Backlog

**Language Support:** Phase 1-5 complete. 35 languages supported.
See `docs/language-support.md` for design. Next: OCaml, Haskell, Clojure, Nim, Crystal.
Run `scripts/missing-grammars.sh` for all 64 remaining arborium grammars.

**CLI Redundancy:** See `docs/llm-code-consistency.md`
- [ ] Rust: OutputFormatter trait for JSON/text output
- [ ] Python: output helpers for JSON/markdown/compact

**CLI Surface Cleanup:** CLI reduced from 29 to 8 commands (-5000+ lines). Remaining:
- [ ] view.rs: consolidated but messy internally - problem shifted, not solved. Needs proper unification.
- [ ] Command/subcommand/flag names should be self-documenting (no abbreviations, clear meaning)

**Code Quality:**
- Audit Rust codebase for tuple returns - replace with structs unconditionally
  - Already fixed: `find_symbols` → `SymbolMatch`, `call_graph_stats` → `CallGraphStats`, `get_changed_files` → `ChangedFiles`
  - Also fixed: `IndexedCounts`, `CollapsedChain`, `ParsedPackage`, `ExtractedDeps`
- Validate node kinds against grammars (test that kinds like "if_statement" exist in grammar)
- Directory context: attach LLM-relevant context to directories (like CLAUDE.md but hierarchical)

**Bugs:**
- Call graph shows same-named method on different object as self-call (e.g., `suggest_tool` calls `router.suggest_tool`)

**Performance:**
- Investigate slow `moss analyze --health` (+500ms over baseline, not uv startup)

**Integration:**
- Complete daemon integration (FileIndex API methods currently unused)
- LSP refactor actions (rename symbol across files)
- Cross-language reference tracking (Python ↔ Rust)

**Tooling:**
- Avoid web search for Rust crate features: `cargo info <crate> --features` or similar offline lookup
- Structured TODO.md editing: first-class `moss todo` command to add/complete/move items without losing content (Opus 4.5 drops TODO items when editing markdown)
- Multi-file batch edit: grep to find occurrences, then apply same edit across all files in one call (less latency than N sequential edits)

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
- Session analysis: detect correction patterns ("You're right", "Good point", "isn't working")
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
