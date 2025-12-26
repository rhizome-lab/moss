# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs.

## Next Up

(empty - pick from backlog)

Test Status: 110 passing, 0 failing (moss-languages)

## Remaining Work

**Workflow Engine:**
- [ ] Port LLM calling logic (streaming, tool use) as workflow component

**Consider Porting:**
- [x] `cmd_check_refs` - bidirectional code/doc reference checking → `moss analyze --check-refs`

**Rust Redesign Candidates:**
- Rules engine: consider semgrep/ruff integration instead of custom
- Plugin system: Rust trait-based plugins or external tool orchestration
- Edit routing: workflow engine with LLM decision points
- Session/checkpoint: workflow state persistence
- PR/diff analysis: `moss analyze --pr` or similar
- Context compilation: `moss view --context` combining skeleton + deps

## Backlog

**Language Support:** 98 languages implemented - all arborium grammars covered.
See `docs/language-support.md` for design. Run `scripts/missing-grammars.sh` to verify.

**Workflow Engine Design:**
Current scaffold is TOML state machines. Needs design work:
- Interactive/agentic sessions (user-driven, not TOML-defined)
- Relationship between `moss workflow` and interactive sessions
- Where LLM decision-making hooks in (workflow plugin? separate mode?)
- Unify or separate: scripted workflows vs interactive agent loops

**Code Quality:**
- Validate node kinds against grammars: `validate_unused_kinds_audit()` in each language file ensures documented unused kinds stay in sync with grammar
- Directory context: attach LLM-relevant context to directories (like CLAUDE.md but hierarchical)
- Deduplicate SQL queries in moss-cli: many ad-hoc queries could use shared prepared statements or query builders (needs design: queries use different execution contexts - Connection vs Transaction)

**Daemon Design:**
- Multi-codebase: single daemon indexing multiple roots simultaneously
- Minimal memory footprint (currently loads full index per root)
- Per-project config: .moss/config.toml to disable daemon/indexing
- Global config: ~/.config/moss/config.toml for defaults
- Auto-start: `moss` commands start daemon if enabled and not running (~2.3ms check cost)

**HTTP Server:**
- OpenAPI spec for `moss serve http` endpoints (enables client codegen, docs)
- Codegen from OpenAPI: generate TypeScript/Python/Rust clients

**Tooling:**
- Structured TODO.md editing: first-class `moss todo` command to add/complete/move items without losing content (Opus 4.5 drops TODO items when editing markdown)
- Multi-file batch edit: less latency than N sequential edits. Not for identical replacements (use sed) or semantic renames (use LSP). For structured batch edits where each file needs similar-but-contextual changes (e.g., adding a trait method to 35 language files).

**Linting:**
- `lint list` now ~100ms (removed recursive dir scans, config-only detection)

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
- Session analysis: detect correction patterns ("You're right", "Good point", "Fair point", "Should have", "Right -", "isn't working")
- LLM code consistency: see `docs/llm-code-consistency.md` for research notes
- Analyze long chains of uninterrupted tool calls (friction indicator)

**Session Tooling:**
- End-of-session summary workflow (.moss/workflows/session-summary.toml, no LLM):
  - Test status: passing/failing count
  - `git diff --shortstat` (files changed, insertions, deletions)
  - Commits ahead of remote
  - Uncommitted changes summary
  - TODO.md delta (items added/completed)
- Introspect ~/.claude/plans/ - list/view saved plan files from Claude Code sessions

**Distribution:**
- Wrapper packages for ecosystems: npm, PyPI, Homebrew, etc. (single binary, wrapper scripts)

## Deferred

- VS Code extension: test and publish to marketplace (after first CLI release)
- Remaining docs: prior-art.md, hybrid-loops.md
- Memory system: layered cross-session learning

## Python Feature Audit

Tracking what was in Python packages, what's been reimplemented, what's intentionally dropped.

**moss-orchestration (~112 files):**
- Session management with checkpointing - not yet in Rust
- Driver protocol for agent decision-making - not yet in Rust
- Rules engine with SARIF output - consider semgrep/ruff integration
- Plugin system - Rust trait-based plugins (partial)
- Event bus, validators, policies - not yet in Rust
- PR review, diff analysis - not yet in Rust
- Watch/test runners - `moss lint --watch` exists
- Gen commands for MCP/HTTP/gRPC/LSP - MCP/HTTP/LSP in Rust

**moss-intelligence (~36 files):**
- Skeleton extraction - ✓ in Rust
- Complexity analysis - ✓ in Rust
- Dependency analysis - ✓ in Rust
- Security analysis - ✓ in Rust (shells to bandit)
- Edit routing (LLM-powered) - not yet in Rust
- Summarization (LLM-powered) - not yet in Rust

**moss-llm:**
- LLM adapters - not yet in Rust (rig crate available)
- Model abstraction - not yet in Rust

**moss-context:**
- Working memory with summarization - not yet in Rust
- Context compilation (skeleton + deps + summary) - partial via `view --deps`

**moss-mcp/acp:**
- MCP server - ✓ in Rust
- ACP server - dropped (unused)
- dwim.py tool resolution - simplified in Rust (3 primitives)

**moss-lsp:**
- LSP server - ✓ in Rust

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
