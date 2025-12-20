# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs.

## Next Up

- Config schema in `.moss/config.toml` for memory system
- Workflow self-creation from detected patterns

## Recently Completed

- **Memory plugins with LRU caching** (Dec 2025):
  - `LRUCache[K, V]` generic class with O(1) operations
  - `EpisodicStore` now uses LRU eviction (least recently accessed)
  - `SimpleVectorIndex` supports optional `max_items` with LRU
  - Full test coverage for LRU behavior

- **Agent swarm coordination patterns** (Dec 2025):
  - `SwarmCoordinator` with common patterns
  - Fork-join: parallel execution, wait for all
  - Pipeline: sequential chain with data transformation
  - MapReduce: parallel map, aggregate reduce
  - Voting: consensus from multiple workers
  - Race: first completion wins
  - Retry with exponential backoff
  - Supervised: monitor and restart failed workers

- **Persistent terminal sessions** (Dec 2025):
  - `PersistentShell` maintains state across commands (cd, export, aliases)
  - `TerminalSubagent` provides high-level interface for scripts
  - Auto-detection of shell path for cross-platform support

- **Architect/Editor split** (Dec 2025):
  - Separated reasoning (planning what to change) from execution (making changes)
  - `LLMArchitect` produces structured `EditPlan` with steps, context needs, risks
  - `StructuredEditor` executes plans using anchor-based patches
  - `ArchitectEditorLoop` orchestrates plan → execute → validate → revise cycle

- **Clone elimination round 2** (Dec 2025):
  - Reduced clones from 30→18 (40% reduction)
  - `BaseLibrary` class for shared library plugin functionality
  - `LazyAPIExecutor` base class for executor pattern
  - `extract_turns_from_entries()` shared function for parsers
  - Remaining 8 groups are trivial 3-line properties or protocol implementations

- **Clone elimination** (Dec 2025):
  - Reduced clones from 56→30 (46% reduction) using mixins and shared utilities
  - PathResolvingMixin, EventEmitterMixin, WorkspaceRunner base class
  - `moss clones` now shows full relative paths (e.g. `src/moss/foo.py:42-50`)

- **Memory plugin system** (Dec 2025):
  - `MemoryPlugin` protocol for extensible memory sources
  - Plugin discovery from `.moss/memory/` and `~/.config/moss/memory/`
  - `MemoryLayer` aggregates automatic/triggered/on-demand plugins
  - Full test coverage for plugin loading and layer operations

- **Ephemeral output caching** (Dec 2025):
  - `LLMToolExecutor` auto-caches large outputs (>4K chars)
  - Returns preview + cache ID instead of full content
  - `cache.get` tool to retrieve full content on demand
  - Prevents context blowup from large tool results

- **Pattern detection** (Dec 2025):
  - `moss patterns` CLI command to detect architectural patterns
  - Detects: plugin systems, factories, strategies, singletons, coupling
  - JSON output with `--json`, compact summary with `--compact`

- **Expand auto-select** (Dec 2025):
  - Auto-selects best match when all matches have same symbol name
  - Path specificity already handled by ranking (shorter path = higher score)
  - List shown only when symbol names differ (needs user disambiguation)

- **Smart TOML navigation** (Dec 2025):
  - `moss toml` CLI command with jq-like queries
  - TomlAPI for MCP integration (parse, query, keys, summary)
  - Supports hyphenated keys, array indexing, pipe functions

- **Custom tool semantics** (Dec 2025):
  - `.moss/dwim.toml` config for project-specific tool mappings
  - Custom aliases, keyword boosts, intent patterns (regex→tool)
  - User-level config in `~/.config/moss/dwim.toml`
  - Custom tool definitions with keywords and parameters

- **DWIM & CLI enhancements** (Dec 2025):
  - Multi-match handling: numbered list + `--select N/best` for expand/callers/callees
  - Confidence thresholds: auto-correct (>=85%), execute (60-85%), clarify (<60%)
  - MCP tool discovery: auto-register external MCP tools into DWIM registry

- **CLI UX improvements** (Dec 2025):
  - Flexible arg syntax for expand/callers/callees: `symbol`, `file:symbol`, `file symbol`, `symbol file`
  - DWIM terse parsing fix: "expand Patch" now correctly matches cli_expand, not patch tools
  - Added `moss agent --dry-run` to preview task classification and tool suggestions

- **DWIMLoop improvements** (Dec 2025):
  - Task classification (read-only vs write) with completion hints
  - Multi-file expand: `expand Symbol` searches codebase, `expand Symbol file1.py file2.py` searches multiple
  - Integration tests with mocked LLM responses

- **DWIMLoop evaluation and fixes** (Dec 2025):
  - Fixed parameter mapping (file_path vs path vs symbol)
  - Added stall detection (repeated command exits)
  - Added recursion guard in tool execution
  - Improved system prompt for "done" signaling
  - Integrated EphemeralCache for result caching with TTL
- **Plugin consolidation** (Dec 2025):
  - LLM providers: removed anthropic.py and openai.py, use litellm for all providers
  - Linter plugins: existing architecture already good (SARIFAdapter + per-tool plugins)
- **TaskTree implementation** (`src/moss/task_tree.py`):
  - Hierarchical task state with arbitrary depth
  - Path-based context (chain from root to current leaf)
  - Notes with TTL (on_done, manual, turns_remaining)
  - Serialization for persistence
- **DWIMLoop refactored to context-excluded model**:
  - No conversation history - each turn: system + path + notes + last result
  - ~300 tokens/turn instead of unbounded growth
  - Meta-commands: breakdown, note, fetch, done
  - Result caching with preview + ID
- **`moss agent` CLI command**: Run agent loop on tasks
- **Hierarchical context model** (see `docs/agentic-loop.md`):
  - Context-excluded by default - no conversation history accumulation
  - Path-based state: Task → Subtask → Current step (arbitrary depth)
  - Notes with expiration conditions (on_done, after:N_turns, manual)
  - Recursive breakdown as fundamental agent behavior
- **Optional dependencies cleanup**:
  - Removed `gemini` extra (use litellm instead, no direct provider)
  - Removed `grpc` extra (unused)
  - Made `rag` alias for `chroma` (backward compat)
  - Simplified `all` group
- **DWIM-driven agent loop** (`src/moss/dwim_loop.py`):
  - `parse_intent()` - extracts verb + target from terse commands
  - `DWIMLoop` class - full agent loop with LLM → DWIM → execute → result cycle
  - Terse command format: "skeleton foo.py", "expand Patch", "fix: add null check", "done"
  - No tool schemas in prompts - 90%+ token reduction vs function calling
- **DWIM heuristics improved**:
  - Keyword scoring: reward matches rather than penalize tools with many keywords
  - First-word action boost: 30% bonus when first word matches a keyword
  - Exact name boost: 40% bonus when first word is the tool name
  - Scores improved from 0.29→0.83 for direct commands

## Active Backlog

**Large:**
- [ ] Memory system - layered memory for cross-session learning (see `docs/memory-system.md`)
- [ ] Workflow loader plugin abstraction - extract protocol when Python workflows need it
  - Current: TOML loader is direct implementation
  - Future: `WorkflowLoader` protocol, entry point registration, multiple loader types

### Strict Harness (guardrails for all agents)

**Signal-Only Diagnostics:** (done - see `src/moss/diagnostics.py`, `src/moss/validators.py`)
- [x] Parse `cargo check --message-format=json` instead of raw stderr
- [x] Extract: error code, message, file/line, suggestion - discard ASCII art
- [x] Integrate with validation loop via `DiagnosticValidator`
- [x] "Syntax Repair Engine" system prompt when errors present (see `REPAIR_ENGINE_PROMPT` in `agent_loop.py`)

**Degraded Mode (AST fallback):** (done - see `src/moss/tree_sitter.py`)
- [x] Wrap tree-sitter parse in Result (`ParseResult`)
- [x] On parse failure, fallback to "Text Window" mode (`text_window()`)
- [x] Never block read access due to parse failures

**Peek-First Policy:** (done - see `LoopContext.expanded_symbols` in `agent_loop.py`)
- [x] Constraint: agent cannot edit symbol only seen as skeleton
- [x] Must `expand` before `edit` - enforced in agent loop (`MossToolExecutor.enforce_peek_first`)
- [x] Prevents hallucination of function bodies

**Hunk-Level Rollback:** (done - see `src/moss/shadow_git.py`)
- [x] `DiffHunk` dataclass and `parse_diff()` for diff parsing
- [x] `get_hunks()` - parse branch diff into hunks
- [x] `map_hunks_to_symbols()` - map hunks to AST nodes via tree-sitter
- [x] `rollback_hunks()` - selectively revert specific hunks

## Future Work

### Codebase Tree Consolidation (see `docs/codebase-tree.md`)

**Phase 1: Python CLI delegates to Rust (remove Python implementations)**
- [ ] `skeleton` → delegate to Rust `view`
- [ ] `summarize` → delegate to Rust `view`
- [ ] `anchors` → delegate to Rust `search` with type filter
- [ ] `query` → delegate to Rust `search`
- [ ] `tree` → delegate to Rust `view` (directory-level)
- [x] `context` → delegate to Rust `context` (done)

**Phase 2: Unified tree model**
- [ ] Merge filesystem + AST into single tree data structure
- [ ] Implement zoom levels (directory → file → class → method → params)
- [ ] Consistent "context + node + children" view format

**Phase 3: DWIM integration**
- [ ] Natural language → tree operation mapping
- [ ] "what's in X" → view, "show me Y" → view, "full code of Z" → expand

### DWIM Improvements
- [x] Terse command parsing - "expand Patch" → cli_expand, not patch tools (done Dec 2025)
- [x] Confidence thresholds - three zones: auto-correct (>=85%), execute (60-85%), clarify (<60%) (done Dec 2025)
- [x] MCP tool discovery - auto-register MCP server tools into DWIM registry (done Dec 2025)
- [x] Custom tool semantics - `.moss/dwim.toml` for user-defined intent→tool mappings (done Dec 2025)

### CLI UX Improvements
- [x] `moss expand` - flexible arg syntax: `symbol`, `file:symbol`, `file symbol` (done Dec 2025)
- [x] `moss agent --dry-run` - show task classification and tool suggestions (done Dec 2025)
- [x] `moss expand/callers/callees` - numbered list + `--select N/best` for multiple matches (done Dec 2025)
- [x] Smart TOML navigation - `moss toml` with jq-like filtering (done Dec 2025)

### Workflow Collaboration
- [ ] Pattern detection - heuristic (frequency, similarity, rapid re-runs) + LLM for judgment
- [ ] Workflow self-creation - agent creates workflows from detected patterns autonomously
- [ ] Workflow discovery - surface candidates from Makefile/package.json/CI, agent or user picks
- [ ] Prompt header - minimal "available: test, lint, ci" announcement in agent prompts
- [ ] Scaffold command - `moss workflow new <name>` with templates + optional LLM assist

### Skills System
- [ ] `TriggerMode` protocol for plugin-extensible triggers
- [ ] `.moss/skills/` directory for user-defined skills
- [ ] Trigger modes: constant, rag, directory, file_pattern, context

### MCP & Protocols
- [ ] Extension validation before activation
- [ ] Permission scoping for MCP servers
- [ ] A2A protocol integration

### Online Integrations
- [ ] GitHub, GitLab, Forgejo/Gitea - issues, PRs, CI
- [ ] Trello, Jira, Linear - task management
- [ ] Bidirectional sync with issue trackers

### Code Quality
- [x] `moss patterns` - detect architectural patterns - done (Dec 2025)
- [ ] `moss refactor` - detect opportunities, apply with rope/libcst
- [ ] `moss review` - PR analysis using rules + LLM

### Plugin Simplification
- [x] LLM providers: consolidated to litellm-only (Dec 2025)
- [x] Linter plugins: existing SARIFAdapter + per-tool plugins is correct design

### Dependency Introspection
- [ ] `moss external-deps` improvements - filtering, show which features include a dep
- [ ] Plugin architecture for package managers (pip, npm, cargo, go mod, etc.)
- [ ] Unified interface: list deps, check updates, find conflicts

### LLM-Assisted Operations
- [ ] `moss gen-tests` - generate tests for uncovered code
- [ ] `moss document` - generate/update docstrings
- [ ] `moss explain <symbol>` - explain any code construct
- [ ] `moss localize <test>` - find buggy code from failing test

### Memory System
- [x] Wire `MemoryLayer` into `LLMToolExecutor` (automatic layer) - done
- [x] Add `check_triggers()` before risky steps (triggered layer) - done
- [x] Expose `memory.recall()` as agent tool (on-demand layer) - done
- [x] Plugin loading from `.moss/memory/` - done (Dec 2025)
- [ ] Config schema in `.moss/config.toml`

### Agent Infrastructure
- [x] Ephemeral output caching for agent loop - done (Dec 2025)
  - Large tool outputs → EphemeralCache → preview + ID
  - `cache.get` tool to fetch full content on demand
- [ ] Architect/Editor split - separate reasoning from editing
- [ ] Configurable agent roles in `.moss/agents/`
- [ ] Multi-subtree parallelism for independent work
- [ ] Terminal subagent with persistent shell session
- [ ] Dynamic prompts - external generators receive structured context object (intent, steps, files)

### Evaluation & Debugging
- [ ] SWE-bench harness - benchmark against standard tasks
- [ ] Anchor patching comparison vs search/replace vs diff
- [ ] Skeleton value measurement - does structural context help?

**Agent Log Analysis:**
- [ ] `moss analyze-session` improvements - detect failure patterns, wasted loops, recovery strategies
- [ ] Token waste detection - find where context fills with low-value content
- [ ] Tool effectiveness metrics - which tools help vs confuse the agent
- [ ] Distill conventions - extract codebase-specific patterns from successful sessions
- [ ] Auto-generate CLAUDE.md entries from repeated corrections

**Moss Architecture Insights:**
- [ ] Identify missing tools - what does the agent grep for that should be a tool?
- [ ] Prompt improvement suggestions - where do agents misunderstand instructions
- [ ] Compare loop strategies - which AgentLoop patterns work best for which tasks
- [ ] Shadow branch analysis - how often do merges conflict, what causes them

### Reference Resolution (GitHub-level)
- [ ] Full import graph with alias tracking (`from x import y as z`)
- [ ] Variable scoping analysis (what does `x` refer to in context?)
- [ ] Type inference for method calls (`foo.bar()` where `foo: Foo`)
- [ ] Cross-language reference tracking (Python ↔ Rust)

## Deferred

- Log format adapters - after loop work validates architecture

## Notes

### Key Findings
- **86.9% token reduction** using skeleton vs full file (dwim.py: 3,890 vs 29,748 chars)
- **12x output token reduction** with terse prompts (1421 → 112 tokens)
- **90.2% token savings** in composable loops E2E tests
- **93% token reduction** in tool definitions using compact encoding (8K → 558 tokens)

### Performance Profiling (Dec 2025)

**Rust CLI (indexed, warmed):**
- Fast (3-14ms): path, tree --depth 2, search-tree, callers, expand, grep
- Medium (40-46ms): symbols, skeleton, callees, complexity, deps, anchors
- Slow (66ms): summarize (tree-sitter full parse)
- Slowest (95ms): health (parallel codebase scan, 3561 files)

**Python API (with Rust CLI):**
- skeleton: 53ms (single file tree-sitter)
- find_symbols: ~1ms via Rust CLI (was 723ms with Python scan)
- grep: ~4ms with Rust CLI

**Completed Optimizations:**
1. ✅ Rust CLI grep with ripgrep - 9.7s → 4ms (2400x speedup)
2. ✅ Rust health with rayon - 500ms → 95ms (5x speedup)
3. ✅ Rust CLI find-symbols with indexed SQLite - 723ms → 1ms (720x speedup)

### Dogfooding Observations (Dec 2025)
- `skeleton_format` / `skeleton_expand` - very useful, genuinely saves tokens
- `complexity_get_high_risk` - instant actionable data in one call
- `search_find_symbols` - now recursively finds methods inside classes (fixed Dec 2025)
- `explain_symbol` - shows callers/callees for a function (added Dec 2025)
- `guessability_score` - evaluate codebase structure quality

**Missing/wanted:**
- `search_by_keyword` - semantic search across functions, not just name matching

**Recently added:**
- `search_find_related_files` - files that import/are imported by a given file
- `search_summarize_module` - "what does this module do?"

**Friction:**
- Error messages should be specific ("File not found: X" not "No plugin for .py")
- `to_compact()` output feels natural; raw data structures feel clunky

### Agent Lessons
- Don't assume files exist based on class names - `SearchAPI` is in `moss_api.py`, not `search.py`
- Tools aren't perfect - when an error seems wrong, question your inputs before assuming the tool is broken
- Tool design: return specific error messages ("File not found: X") not generic ones ("No plugin for .py")
- **Check before creating**: Always search for existing files before creating new ones (e.g., `prior_art.md` vs `prior-art.md`)
- **Don't read entire large files**: Use grep/skeleton to find relevant section first, then read only what's needed

### Design Principles
See `docs/philosophy.md` for full tenets. Key goals:
- Minimize LLM usage (structural tools first)
- Maximize useful work per token
- Low barrier to entry, works on messy codebases
