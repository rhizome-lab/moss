# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs.

## Next Up

1. Hunk-level rollback for shadow_git
2. Expose memory.recall() as agent tool (on-demand layer)
3. Add skeleton.expand to MCP tool definitions

## Active Backlog

**Large:**
- [ ] Memory system - layered memory for cross-session learning (see `docs/memory-system.md`)

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

**Hunk-Level Rollback (shadow_git enhancement):**
- [ ] Map diff hunks to AST nodes
- [ ] On verification failure, cherry-pick passing hunks, revert failing ones
- [ ] More surgical than commit-level rollback

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
- [ ] `moss patterns` - detect architectural patterns
- [ ] `moss refactor` - detect opportunities, apply with rope/libcst
- [ ] `moss review` - PR analysis using rules + LLM

### LLM-Assisted Operations
- [ ] `moss gen-tests` - generate tests for uncovered code
- [ ] `moss document` - generate/update docstrings
- [ ] `moss explain <symbol>` - explain any code construct
- [ ] `moss localize <test>` - find buggy code from failing test

### Memory System
- [x] Wire `MemoryLayer` into `LLMToolExecutor` (automatic layer) - done
- [x] Add `check_triggers()` before risky steps (triggered layer) - done
- [ ] Expose `memory.recall()` as agent tool (on-demand layer)
- [ ] Plugin loading from `.moss/memory/`
- [ ] Config schema in `.moss/config.toml`

### Agent Infrastructure
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
