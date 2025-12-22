# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs.

## Current Focus: CLI & Agent Experience

Dogfooding and CLI improvement are the same work stream. The goal is to make `moss agent` and the 3 primitives (view, edit, analyze) work reliably.

**Iterate:**
1. Run `moss agent "task description"` on real tasks
2. Log issues to `logs/dogfood-YYYY-MM-DD.md`
3. Fix issues, improve prompts/tooling
4. Repeat

**Known Issues:**
- [x] Agent path resolution - FIXED: now routes through Rust CLI with fuzzy resolution
- [x] Agent stuck in retry loop - FIXED: fallback strategy (retry_threshold, FallbackStrategy)
- [x] LLM hallucinating symbol names - FIXED: input validation + "Never Extract Manually" principle

**Docs Alignment:**
- [x] `docs/dwim-architecture.md` - rewritten for 3 primitives
- [x] `docs/primitives-spec.md` - added `analyze` section
- [x] `docs/agentic-loop.md` - updated examples for view/edit/analyze
- [x] `docs/codebase-tree.md` - updated example to show current dwim.py structure
- [x] `docs/tools.md` - rewritten for 3 primitives + legacy migration
- [x] `docs/cli/commands.md` - added view/edit/analyze docs, deprecated health
- [x] `CLAUDE.md` - updated dogfooding section for 3 primitives
- [ ] Remaining: prior-art.md, hybrid-loops.md, etc. (lower priority)

**Unified Plumbing for 3 Primitives:**
- [x] Path resolution unified: `path_resolve::resolve_unified` used by view, edit, analyze
- [x] Add `--kind` filter to analyze (uses `--kind` to avoid `-t` conflict with `--threshold`)
- [x] Analyze uses unified resolution for symbol targeting (`analyze cli.py/func --complexity`)
- [ ] Python edit uses separate file/symbol targeting (LLM-based, intentionally different)

**CLI Cleanup:**
- [x] `dwim` CLI - REMOVED (module kept for alias resolution)
- [x] `loop` CLI - REMOVED along with predefined loops (simple, critic, etc.)
- [x] `patterns`, `git-hotspots` - NOT slow (6s, 2.5s), keeping both
- [x] `--compact` mode on patterns (added)
- [x] Large file detection in `analyze --health` (shows top 10 files >500 lines)
- [x] Folded `health`, `summarize`, `check-docs`, `check-todos` into `analyze` flags
- [x] PTY detection for auto-compact mode (non-TTY defaults to compact)

**Keys:** see `.env.example` for ANTHROPIC_API_KEY, OPENAI_API_KEY, GEMINI_API_KEY

## Next Up

**View Primitive Enhancements (from architecture review):**
- [x] `--types-only` flag: show only types/interfaces/signatures, no bodies (highest ROI)
- [x] `--fisheye` mode: show target at full detail, imported modules at signature level
- [ ] `--resolve-imports`: inline imported symbol signatures (lower priority, fisheye covers most cases)

**Housekeeping:**
- [x] Split cli.py into cli/ package (structure in place, gradual extraction)
- [x] Split moss_api.py into moss_api/ package (structure in place)

**Deferred:**
- [ ] Python edit separate targeting (LLM-based, intentionally different)
- [ ] Remaining docs: prior-art.md, hybrid-loops.md (lower priority)

## Backlog

**View Primitive Polish:**
- [ ] `--visibility public|all` or `--exported` filter for semantic zoom
- [ ] Barrel file hoisting: detect `export * from` and surface re-exported symbols
- [ ] Useless docstring detection: skip "Sets the user id" on `setUserId()`
- [ ] Fisheye for Rust/TypeScript (currently Python-only import resolution)
- [ ] Selective import resolution (e.g., `--fisheye=moss.config` to expand only that import)

**Dogfooding Notes (Dec 23):**
- `--types-only` works well for architectural overview
- `--fisheye` resolves Python imports and shows imported module skeletons
- Combined flags (`--types-only --fisheye`) give compact architectural map
- Updated dwim.toml system prompt to mention new flags

**Architecture Cleanup:**
- [x] Consolidate redundant layers discovered Dec 22:
  - [x] SkeletonAPI.expand → now uses rust_view() properly
  - [x] rust_shim: rust_skeleton (calls skeleton), rust_view (calls view) - both correct
  - [x] Unify under `moss workflow run`:
    - [x] `moss workflow run dwim --arg task="..."` - agentic
    - [x] `moss agent` is now alias for workflow run dwim
    - [x] Removed: AgentLoop, MossToolExecutor, old workflow loader (2742 lines)
    - [x] Nested steps (step type that runs sub-steps) - see docs/nested-execution.md
    - [x] State machine workflows (states + transitions) - see design doc
  - [x] DWIMLoop removed - replaced by composable execution primitives (src/moss/execution/)
  - [x] Python edit assessment: keep EditAPI (file ops), complexity-routed edit() is stubs - agent handles it
  - Rust edit vs Python edit → same name, different behavior (Rust=structural, Python=LLM-routed)
- [x] Define clear boundaries: what's Rust, what's Python, why → docs/rust-python-boundary.md

**Workflow Enhancements:**
- [x] State machine: parallel state execution (fork/join with `parallel` and `join` fields)
- [x] State machine: nested state machines (workflow field on states)
- [x] State machine: LLM-driven state selection (llm_select field on states)

**Indexing Performance:**
- [x] Slow reindexing on large repos - FIXED (20s → 1s on ~/git/enso/)
  - Fixed redundant parsing (find_callees_for_symbol avoids re-parse)
  - Added parallel file processing with rayon
  - Added prepared statements for batch SQL inserts

**Rust Crate Consolidation:**
- [x] Extract shared code from moss-cli and moss-daemon into moss-core crate
- [x] Share: tree-sitter parsers, Language detection, SymbolKind types
- [ ] Consider: consolidate index.rs, symbols.rs (different designs for CLI vs daemon)
- [x] Refactor file extension matching: centralized SOURCE_EXTENSIONS constant + helper functions

**Code Organization:**
- [x] Synthesis plugins: aligned module paths with entry point names (Dec 22)
- See "Architecture Cleanup" above for major refactoring items

**Call Graph Improvements:**
- [x] Call extraction for Python, Rust, TypeScript, JavaScript, Java, Go
- [ ] Missing language support: Scala, Vue (no tree-sitter grammars yet)
- [ ] "(no ext)" files high count in some repos - uses gitignore, add binary detection if needed

**Skeleton Language Support:**
- [x] Added 16 tree-sitter grammars: Python, Rust, Markdown, JavaScript, TypeScript, TSX, JSON, YAML, HTML, CSS, Go, C, C++, Java, Ruby, Bash, TOML
- [x] Skeleton extraction for: Python, Rust, Markdown, JavaScript, TypeScript, Go, Java, C, C++, Ruby, JSON, YAML, TOML
- [x] Symbol parsing for call graph: Python, Rust, Java, TypeScript, TSX, JavaScript, Go, JSON, YAML, TOML
- [x] Data file key extraction: JSON/YAML/TOML keys become symbols (objects=class, values=variable)

**Explore TUI Polish:**
- [x] `.moss` index: support optional external location via MOSS_INDEX_DIR env var

**Session Analysis / Self-Improvement:**
- [ ] Correction pattern detection: extract first 2-3 words of assistant responses, flag patterns like "You're right", "Good point", "Ah yes", "My bad", etc.
- [ ] Could be a `moss analyze-session` tool or part of telemetry
- [ ] Use detected corrections to identify friction points, improve prompts/tools

**Explore TUI Keybinds:**
- [x] Modal keybinds (TUIMode.bindings, active_bindings property, KeybindBar refresh)
- [x] Jump-to-node shortcut (fuzzy search to quickly navigate tree) - 'g' key
- [ ] View/Edit/Analyze: mode indicator (bottom right, near palette) OR tabbed content view
- [ ] Remove theme keybind (T) - wasteful, only toggles light/dark

**Editor Integration:**
- [ ] LSP refactor actions (rename symbol across files via language server)

**Telemetry** (see `docs/telemetry.md`):
- [x] `moss telemetry` CLI with aggregate analysis
- [x] HTML dashboard output
- [x] Plugin architecture for log formats (LogParser protocol)
- [x] File-level token tracking (`file_tokens` in SessionAnalysis)
- [x] Gemini CLI log parser
- [x] Real-time telemetry mode (`--watch`)
- [x] Symbol-level token tracking (moss view/analyze symbol paths)

**Memory System** (see `docs/memory-system.md`):
- [ ] Layered memory for cross-session learning

## Future Work

### Agent TUI (future)
- [ ] Terminal output sanitization: reset terminal state after nested command output (escape codes leak through)

### Agent Research
- [ ] Research https://ampcode.com/ (agent coding tool)
- [ ] Conversational loop pattern (vs hierarchical)
- [ ] YOLO mode evaluation
- [ ] Diffusion-like parallel refactors
- [ ] Fine-tuned tiny models (100M RWKV)

### Codebase Tree (see `docs/codebase-tree.md`)
Phase 1-3 complete. See changelog.

### Distribution
- [ ] Auto-updates
- [ ] Portable single binary
- [ ] Pre-built binaries (GitHub Actions)

### Reference Resolution
- [ ] Full import graph with alias tracking
- [ ] Variable scoping analysis
- [ ] Type inference for method calls
- [ ] Cross-language tracking (Python ↔ Rust)

## Notes

### Design Principles
See `docs/philosophy.md`. Key goals:
- **Generalize, Don't Multiply**: One flexible solution over N specialized ones
- **Three Primitives**: view, edit, analyze (composable, not specialized)
- Minimize LLM usage (structural tools first)
- Maximize useful work per token
