# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs.

## Next Up

1. **Performance profiling** - measure hot paths in CLI, daemon, Python components
2. **Compact tool encoding** - bypass JSON Schema overhead for moss agent

## Completed (move to CHANGELOG)

- **Index reliability** - Robust handling of index issues
  - Graceful degradation: `imports` command falls back to direct parsing when index unavailable
  - Error recovery: Automatic database rebuild on corruption detection
  - Incremental refresh: `incremental_refresh()` and `incremental_call_graph_refresh()` for faster updates
  - File watching: Daemon auto-reindexes on file changes (via inotify/watchfiles)

- **Rust CLI** - 18 commands: path, view, search-tree, symbols, expand, callers, callees, tree, skeleton, anchors, deps, cfg, complexity, health, summarize, daemon (status/shutdown/start), reindex
- **Daemon** - Unix socket IPC, idle timeout, chunked streaming
- **Call graph** - SQLite index, 29,000x faster callers lookup (0.6ms vs 17.5s)
- **Reference tracing** - Complete cross-file resolution
  - Import tracking (SQLite table: file → module, name, alias)
  - `moss imports <file>` command to query imports from index
  - `moss imports <file>:<name> --resolve` to trace name to source module
  - Cross-file resolution via import alias JOIN for callers/callees
  - Qualified names (module.func vs func) with callee_qualifier
  - Wildcard import resolution (from X import * → check X's exports)
  - Method call resolution (self.method() → Class.method)
- **Benchmark suite** - CI integration with regression detection thresholds

## Active Backlog

**Small:**
- [ ] Profiling infrastructure - measure hot paths in CLI, daemon, and Python components
- [ ] Model-agnostic naming - don't over-fit to specific LLM conventions
- [ ] Multiple agents concurrently - no requirement to join back to main stream
- [ ] Graceful failure - handle errors without crashing, provide useful feedback
- [ ] Revisit CLAUDE.md dogfooding section - tools should be self-evident, not need instructions
- [ ] MCP response ephemeral handling - large responses should stream/page instead of filling context
- [ ] Agent sandboxing - restrict bash/shell access, security-conscious CLI wrappers

**Medium:**
- [ ] Compact tool encoding for moss agent - bypass JSON Schema overhead
  - For moss loop, we control both sides - can use terse function signatures
- [ ] Study Goose's context revision (`crates/goose/src/`)
- [ ] Port `context` command to Rust (if context extraction becomes hot path)
- [ ] Port `overview` command to Rust (fast codebase overview)

**Large:**
- [ ] Sessions as first-class - resumable, observable work units

## Future Work

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

### Agent Infrastructure
- [ ] Architect/Editor split - separate reasoning from editing
- [ ] Configurable agent roles in `.moss/agents/`
- [ ] Multi-subtree parallelism for independent work
- [ ] Terminal subagent with persistent shell session

### Evaluation
- [ ] SWE-bench harness - benchmark against standard tasks
- [ ] Anchor patching comparison vs search/replace vs diff
- [ ] Skeleton value measurement - does structural context help?

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

### CLI Benchmark Baselines (Dec 2025)
Fast (3-4ms): path, expand, callers, callees, search-tree, tree --depth 2
Medium (32-37ms): symbols, skeleton, complexity, deps, anchors
Slow (58ms): summarize (tree-sitter full parse)
Slowest (503ms): health (full codebase scan)

### Dogfooding Observations (Dec 2025)
- `skeleton_format` / `skeleton_expand` - very useful, genuinely saves tokens
- `complexity_get_high_risk` - instant actionable data in one call
- `search_find_symbols` - now recursively finds methods inside classes (fixed Dec 2025)
- `explain_symbol` - shows callers/callees for a function (added Dec 2025)
- Prompting issue: CLAUDE.md dogfooding section doesn't push hard enough for moss-first approach
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
