# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs.

## Next Up

Test Status: 110 passing, 0 failing (moss-languages)

## Remaining Work

**Workflow Engine:**
- [x] Port LLM calling logic (streaming, tool use) as workflow component

**Rust Redesign Candidates:**
- Rules engine: consider semgrep/ruff integration instead of custom
- Plugin system: Rust trait-based plugins or external tool orchestration
- Edit routing: workflow engine with LLM decision points
- Session/checkpoint: workflow state persistence
- PR/diff analysis: `moss analyze --pr` or similar
## Backlog

**Language Support:** 98 languages implemented - all arborium grammars covered.
See `docs/language-support.md` for design. Run `scripts/missing-grammars.sh` to verify.


**Workflow Engine:**
- Consider streaming output for `auto{}` driver
- JSON Schema for complex action parameters (currently string-only)

**Code Quality:**
- Validate node kinds against grammars: `validate_unused_kinds_audit()` in each language file ensures documented unused kinds stay in sync with grammar
- Directory context: attach LLM-relevant context to directories (like CLAUDE.md but hierarchical)
- Deduplicate SQL queries in moss-cli: many ad-hoc queries could use shared prepared statements or query builders (needs design: queries use different execution contexts - Connection vs Transaction)

**Daemon Design:**
- Multi-codebase: single daemon indexing multiple roots simultaneously
- Minimal memory footprint (currently loads full index per root)

**Tooling:**
- Structured TODO.md editing: first-class `moss todo` command to add/complete/move items without losing content (Opus 4.5 drops TODO items when editing markdown)
- Multi-file batch edit: less latency than N sequential edits. Not for identical replacements (use sed) or semantic renames (use LSP). For structured batch edits where each file needs similar-but-contextual changes (e.g., adding a trait method to 35 language files).

**Agent Research:**
- Conversational loop pattern (vs hierarchical)
- YOLO mode evaluation
- Diffusion-like parallel refactors
- Claude Code over-reliance on Explore agents: spawns agents for direct tool tasks. Symptom of deeper issue?
- Session analysis: detect correction patterns ("You're right", "Good point", "Fair point", "Should have", "Right -", "isn't working")
- LLM code consistency: see `docs/llm-code-consistency.md` for research notes
- Analyze long chains of uninterrupted tool calls (friction indicator)
- Claude Code lacks navigation: clicking paths/links in output doesn't open them in editor (significant UX gap)

**Distribution:**
- Wrapper packages for ecosystems: npm, PyPI, Homebrew, etc. (single binary, wrapper scripts)

## Deferred

- VS Code extension: test and publish to marketplace (after first CLI release)
- Remaining docs: prior-art.md, hybrid-loops.md
- Memory system: layered cross-session learning

## Python Features Not Yet Ported

**Orchestration:**
- Session management with checkpointing
- Driver protocol for agent decision-making
- Plugin system (partial - Rust traits exist)
- Event bus, validators, policies
- PR review, diff analysis

**LLM-Powered:**
- Edit routing
- Summarization
- Working memory with summarization

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
