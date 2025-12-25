# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs.

## Next Up

1. OutputFormatter trait for consistent JSON/text output
2. Daemon integration: complete FileIndex API methods
3. LSP refactor actions (rename symbol across files)
4. Cross-language reference tracking (Python ↔ Rust)

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
- [x] `cmd_workflow` - TOML state machine engine, ported to Rust (list/run/show/new)
- LLM lives in workflow engine, not primitives. Primitives stay deterministic:
  - `view` - no LLM
  - `edit` - structural/AST only, no LLM
  - `analyze` - no LLM
- `cmd_agent` = `moss workflow run dwim`, not a separate command (use Rust workflow)

Servers (port to Rust):
- [x] `moss serve {mcp,http,lsp}` - command structure added
- [x] `moss serve mcp` - working with rmcp 0.12 (`--features mcp`)
- [x] `moss serve http` - REST API with axum (health, files, symbols, search)
- [x] `moss serve lsp` - LSP server with tower-lsp (symbols, hover, definition, references)

TUI (evaluate):
- [x] `cmd_tui` / `cmd_explore` / `cmd_shell` - deleted (Textual too expensive to port, HTTP API + web UI preferred)

Delete (redundant with Rust CLI or external tools):
- [x] `cmd_toml` - jaq handles TOML natively (deleted from Python CLI)
- [x] `cmd_complexity` - `moss analyze --complexity` (deleted from Python CLI)
- [x] `cmd_deps` - `moss view --deps` (deleted from Python CLI)
- [x] `cmd_cfg` - control flow graph (deleted from Python CLI)
- [x] `cmd_query` - `moss view` with filters (deleted from Python CLI)
- [x] `cmd_rag` - duplicates `cmd_search` (deleted from Python CLI)
- [x] `cmd_metrics` / `cmd_report` / `cmd_overview` - consolidate to `moss analyze --overview` (deleted from Python CLI)

Delete (questionable value):
- [x] `cmd_mutate` - mutation testing wrapper (deleted from Python CLI)
- [x] `cmd_patterns` / `cmd_weaknesses` / `cmd_clones` (deleted from Python CLI)
- [x] `cmd_synthesize` - experimental, mock LLM (deleted from Python CLI)
- [x] `cmd_eval` - SWE-bench harness, research tool (deleted from Python CLI)

Delete (broken - used non-existent moss meta-package):
- [x] `cmd_search` - semantic search (Python broke, Rust has `moss grep`)
- [x] `cmd_checkpoint` - shadow branch management (Python broke)
- [x] `cmd_security` - multi-tool security (Rust has `moss analyze --security`)

Consider porting to Rust:
- [ ] `cmd_check_refs` - bidirectional code/doc reference checking (could be `moss analyze --check-refs`)
- [x] `moss package audit` - dependency analysis with vuln checking
- [x] `moss package why <dep>` - show why a dependency is in the tree (like `npm why`)
- [x] `moss analyze --hotspots` - git history analysis (churn + complexity)

Consolidate to subcommands:
- [x] `cmd_analyze_session` / `cmd_telemetry` / `cmd_extract_preferences` → `moss session {analyze,telemetry,prefs}` (deleted from Python CLI)
- [x] `cmd_mcp_server` / `cmd_acp_server` / `cmd_lsp` → `moss serve {mcp,http,lsp}` (deleted from Python CLI)

**Phase 3: Delete Python** ✓
- [x] Remove `packages/` directory (~75k lines)
- [x] Remove tests/, examples/, scripts/*.py (~30k lines)
- [x] Remove pyproject.toml, uv.lock, StyleGuide.py
- [x] Update flake.nix (remove python313, uv, ruff)
- [x] Update pre-commit hook (Rust-only)
- [x] Update installation docs (README rewritten for Rust CLI)

**Python Package Inventory (pre-deletion reference):**

moss-orchestration (~112 files):
- Session management with checkpointing (session.py, shadow_git.py)
- Driver protocol for agent decision-making (drivers.py, execution_adapters.py)
- Rules engine with SARIF output (rules_single.py, sarif.py) - custom pattern matching
- Plugin system (plugins/) - linters, markdown, data files, tree-sitter
- Event bus, validators, policies (events.py, validators.py, policy.py)
- PR review, diff analysis (pr_review.py, diff_analysis.py)
- Watch/test runners (watch_tests.py, watcher.py)
- Gen commands for MCP/HTTP/gRPC/LSP (gen/)

moss-intelligence (~36 files):
- Skeleton extraction (skeleton.py) - now in Rust
- Complexity analysis (complexity.py) - now in Rust
- Dependency analysis (dependencies.py, dependency_analysis.py) - now in Rust
- Security analysis (security.py) - now in Rust
- Edit routing (edit.py) - LLM-powered structural editing
- Summarization (summarize.py) - LLM-powered
- Rust shim for passthrough (rust_shim.py)

moss-llm:
- LLM adapters using litellm (LLMSummarizer, LLMDecider)
- Model abstraction for Anthropic/OpenAI/etc

moss-context:
- Working memory with summarization
- Context compilation (skeleton + deps + summary)

moss-mcp/acp:
- MCP server implementation (Python, now also in Rust)
- ACP (Agent Communication Protocol) server
- dwim.py integration for tool resolution

moss-lsp:
- LSP server implementation (planned for Rust)

moss-cli:
- Remaining commands: init, run, status, config, distros, context, gen, watch, hooks, rules, edit, pr, diff, roadmap, coverage, lint, help

**Rust redesign candidates:**
- Rules engine: consider semgrep/ruff integration instead of custom
- Plugin system: Rust trait-based plugins or external tool orchestration
- Edit routing: workflow engine with LLM decision points
- Session/checkpoint: workflow state persistence
- PR/diff analysis: `moss analyze --pr` or similar
- Context compilation: `moss view --context` combining skeleton + deps

## Backlog

**Language Support:** 98 languages implemented - all arborium grammars covered.
See `docs/language-support.md` for design. Run `scripts/missing-grammars.sh` to verify.

**CLI Surface Cleanup:**
- [x] view.rs: unified ViewNode abstraction for directories, files, and symbols
- [x] view.rs: depth 2+ shows symbols inside files, JSON uses ViewNode format
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

**Unified Linting Infrastructure (moss-tools):**
- [x] Core: Tool trait, ToolRegistry, SARIF 2.1.0 output
- [x] Adapters: ruff, oxlint, oxfmt, biome, prettier, tsc, tsgo, clippy, rustfmt, gofmt, go-vet, mypy, pyright, eslint, deno-check
- [x] CLI: `moss lint` with auto-detection, --fix, --sarif, --category filter
- [x] Custom tools: .moss/tools.toml config, SARIF/JSON consumption
- [x] Package manager cascade: pnpm exec, npx, pnpm dlx, global (JS); uv run, pipx run, global (Python)
- [x] Watch mode: run relevant linters on file changes with debounce
- [x] Integration: `moss analyze --lint` runs all detected linters

**VS Code Extension (editors/vscode/):**
- [x] Update extension to use Rust CLI instead of Python
- [x] Support multiple languages (Python, TypeScript, JavaScript, Rust, Go)
- [x] Switch to @typescript/native-preview + oxlint for development
- [ ] Test and publish to VS Code marketplace

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

- VS Code extension: test and publish to marketplace
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
