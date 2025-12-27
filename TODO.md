# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs.

## Next Up

- Finish vitepress setup: `corepack enable && pnpm install`, update pnpm version in package.json, test `pnpm docs:dev`
- Expand docs nav in `docs/.vitepress/config.ts` - many docs not yet linked
- Review docs content for vitepress compatibility (frontmatter, etc)

Test Status: 110 passing, 0 failing (moss-languages)

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

**Code Quality:**
- Validate node kinds against grammars: `validate_unused_kinds_audit()` in each language file ensures documented unused kinds stay in sync with grammar
- Directory context: attach LLM-relevant context to directories (like CLAUDE.md but hierarchical)
- Deduplicate SQL queries in moss: many ad-hoc queries could use shared prepared statements or query builders (needs design: queries use different execution contexts - Connection vs Transaction)
- [x] Binary size optimization: LTO + strip reduced 25MB → 18MB (main contributors: moss 2.1MB, bundled C libs 2MB, moss_languages 1.6MB)
- [x] Avoid Command::new in crates/moss-packages/src/ecosystems/: replaced 14 curl calls with ureq HTTP client. bun.lockb properly ported from Bun source (inline + external strings). Remaining Command uses are legitimate CLI tools: npm/cargo/pip-audit/bundle-audit/govulncheck (security audits), nix/nix-env (local queries)

**Daemon Design:**
- Multi-codebase: single daemon indexing multiple roots simultaneously
- Minimal memory footprint (currently loads full index per root)

**Tooling:**
- Multi-file batch edit: less latency than N sequential edits. Not for identical replacements (use sed) or semantic renames (use LSP). For structured batch edits where each file needs similar-but-contextual changes (e.g., adding a trait method to 35 language files).
- Interactive config editor: `moss config` TUI for editing `.moss/config.toml`

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

**Distribution:**
- Wrapper packages for ecosystems: npm, PyPI, Homebrew, etc.
  - Auto-generate and publish in sync with GitHub releases
  - Single binary + thin wrapper scripts per ecosystem

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
