# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs.

## Next Up

- view.rs: depth 2+ on directories should show symbols inside files (currently only shows file tree)
- view.rs: symbol JSON output uses old format, not ViewNode (inconsistent with directory/file output)
- index: lazy reindex on query (check file mtimes, update changed files before querying)

Test Status: 107 passing, 0 failing (moss-languages)

## Python → Rust Migration

Goal: Delete `packages/` entirely. Single Rust binary, no Python dependency.

**Phase 1: Workflow Engine** ✓
- [x] Port TOML workflow state machine to Rust (`moss workflow {list,run,show}`)
- [x] Add `rig` crate for LLM support (optional, `--features llm`)
- [ ] Port LLM calling logic (streaming, tool use) as workflow component

**Phase 2: Audit Python Commands**
Each command: port, redesign, or delete?

Core (port to Rust):
- [ ] `cmd_workflow` - TOML state machine engine, orchestrates primitives
- LLM lives in workflow engine, not primitives. Primitives stay deterministic:
  - `view` - no LLM
  - `edit` - structural/AST only, no LLM
  - `analyze` - no LLM
- `cmd_agent` = `moss workflow run dwim`, not a separate command

Servers (port to Rust):
- [x] `moss serve {mcp,http,lsp}` - command structure added
- [x] `moss serve mcp` - working with rmcp 0.12 (`--features mcp`)
- [ ] `moss serve http` - REST API (consider exposing commands as library first)
- [ ] `moss serve lsp` - LSP server

TUI (evaluate):
- [ ] `cmd_tui` / `cmd_explore` - Textual → ratatui? Or delete?

Delete (redundant with Rust CLI or external tools):
- [ ] `cmd_toml` - jaq handles TOML natively (expose `moss jq` or document jaq usage)
- [x] `cmd_complexity` - `moss analyze --complexity` (deleted from Python CLI)
- [x] `cmd_deps` - `moss view --deps` (deleted from Python CLI)
- [ ] `cmd_cfg` - control flow graph (who uses this?)
- [x] `cmd_query` - `moss view` with filters (deleted from Python CLI)
- [ ] `cmd_rag` - duplicates `cmd_search`?
- [ ] `cmd_metrics` / `cmd_report` / `cmd_overview` - consolidate to `moss analyze --overview`

Delete (questionable value):
- [ ] `cmd_mutate` - mutation testing (is it even implemented?)
- [ ] `cmd_patterns` / `cmd_weaknesses` / `cmd_clones` - used?
- [ ] `cmd_synthesize` - distinct from `cmd_edit`?

Consolidate to subcommands:
- [ ] `cmd_analyze_session` / `cmd_telemetry` / `cmd_extract_preferences` → `moss session {analyze,telemetry,prefs}`
- [x] `cmd_mcp_server` / `cmd_acp_server` / `cmd_lsp` → `moss serve {mcp,http,lsp}` (deleted from Python CLI)

**Phase 3: Delete Python**
- [ ] Remove `packages/` directory
- [ ] Remove Python-related CI/tooling
- [ ] Update installation docs

## Backlog

**Language Support:** 98 languages implemented - all arborium grammars covered.
See `docs/language-support.md` for design. Run `scripts/missing-grammars.sh` to verify.

**CLI Surface Cleanup:**
- [x] view.rs: unified ViewNode abstraction for directories, files, and symbols
- [ ] Command/subcommand/flag names should be self-documenting
- [ ] OutputFormatter trait for consistent JSON/text output

**Workflow Engine Design:**
Current scaffold is TOML state machines. Needs design work:
- Interactive/agentic sessions (user-driven, not TOML-defined)
- Relationship between `moss workflow` and interactive sessions
- Where LLM decision-making hooks in (workflow plugin? separate mode?)
- Unify or separate: scripted workflows vs interactive agent loops

**Code Quality:**
- Audit Rust codebase for tuple returns - replace with structs unconditionally
  - Already fixed: `find_symbols` → `SymbolMatch`, `call_graph_stats` → `CallGraphStats`, `get_changed_files` → `ChangedFiles`
  - Also fixed: `IndexedCounts`, `CollapsedChain`, `ParsedPackage`, `ExtractedDeps`
- Validate node kinds against grammars: `validate_unused_kinds_audit()` in each language file ensures documented unused kinds stay in sync with grammar
- Directory context: attach LLM-relevant context to directories (like CLAUDE.md but hierarchical)
- Deduplicate SQL queries in moss-cli: many ad-hoc queries could use shared prepared statements or query builders
- Cache line counts in index: `analyze --health` still reads all files for line counting, could store in files table

**Integration:**
- Complete daemon integration (FileIndex API methods currently unused)
- LSP refactor actions (rename symbol across files)
- Cross-language reference tracking (Python ↔ Rust)

**Tooling:**
- Structured TODO.md editing: first-class `moss todo` command to add/complete/move items without losing content (Opus 4.5 drops TODO items when editing markdown)
- Multi-file batch edit: less latency than N sequential edits. Not for identical replacements (use sed) or semantic renames (use LSP). For structured batch edits where each file needs similar-but-contextual changes (e.g., adding a trait method to 35 language files).

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

## Deferred

- Remaining docs: prior-art.md, hybrid-loops.md
- Memory system: layered cross-session learning

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
