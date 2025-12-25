# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs.

## Next Up

- Investigate slow `moss analyze --health` (+500ms over baseline)
- view.rs internal cleanup: consolidated but messy, needs proper unification
- Rust crate feature lookup: avoid web search for feature lists

Test Status: 74 passing, 0 failing

## Backlog

**Language Support:** Phase 1-5 complete. 35 languages supported.
See `docs/language-support.md` for design. Future languages: OCaml, Haskell, Clojure, Nim, Crystal.

**CLI Redundancy:** See `docs/llm-code-consistency.md`
- [ ] Rust: OutputFormatter trait for JSON/text output
- [ ] Python: output helpers for JSON/markdown/compact
- [ ] Command/subcommand/flag names should be self-documenting

**Code Quality:**
- Audit Rust codebase for tuple returns - replace with structs unconditionally
- Directory context: attach LLM-relevant context to directories (like CLAUDE.md but hierarchical)

**Bugs:**
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
- Filter view children by type/name (needs design: glob patterns? symbol kinds?)

**Agent Research:**
- Conversational loop pattern (vs hierarchical)
- YOLO mode evaluation
- Diffusion-like parallel refactors
- Claude Code over-reliance on Explore agents

**Session Tooling:**
- End-of-session summary workflow (.moss/workflows/session-summary.toml, no LLM)

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
