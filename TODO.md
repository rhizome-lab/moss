# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs.

## Next Up

1. **Rust CLI polish** - Complete the fast path
   - [x] Create `crates/moss-cli/` with Cargo workspace
   - [x] Implement `moss path` in Rust with fuzzy matching (~4ms with LIKE pre-filter)
   - [x] Add SQLite index for file caching
   - [x] Add `view`, `search-tree`, `reindex` commands
   - [x] Add tree-sitter parsing for Python/Rust
   - [x] Add `symbols`, `expand`, `callers`, `callees` commands
   - [x] Fix `callers` to search all files (not just fuzzy matches)
   - [x] Fix SQLite TEXT→INTEGER conversions (CAST)
   - [x] Add `tree` command to Rust (directory tree view)
   - [x] Add `skeleton` command to Rust (AST-based)
   - [x] Add `anchors` command to Rust (identify code anchors)
   - [x] Add `deps` command to Rust (module dependencies)
   - [x] Add `cfg` command to Rust (control flow graph)
   - [x] Add `complexity` command to Rust
   - [x] Add `health` command to Rust (codebase health metrics)
   - [x] Add `summarize` command to Rust (module overview)

2. **Daemon improvements**
   - [x] Basic daemon scaffold with Unix socket IPC
   - [x] SQLite symbol index with file watching
   - [x] CLI auto-start daemon on first query if not running
   - [x] Add daemon `status`/`shutdown`/`start` subcommands to CLI
   - [x] Daemon auto-shutdown after idle timeout (default 10 min)
   - [x] Handle large query responses (streaming/chunking)

3. **Reference tracing** - AST-based callers/callees
   - [x] Store call graph in SQLite (caller_symbol, callee_name, file, line)
   - [x] Query call graph from daemon for fast lookups (0.6ms vs 17.5s = 29,000x faster)
   - [ ] Cross-file reference resolution (import tracking)
   - [ ] Handle method calls (obj.method() → Class.method)
   - [ ] Handle qualified names (module.func vs func)

4. **MCP improvements**
   - [ ] Add `nucleo` fuzzy plugin for SQLite (research)
   - [ ] Investigate CodeQL-style queries over moss index
   - [ ] Add data flow tracking (basic taint analysis)

5. **Performance & reliability**
   - [ ] Benchmark suite for CLI commands
   - [x] Add `--profile` flag to CLI for timing breakdown
   - [ ] Error recovery when index is corrupted
   - [ ] Graceful degradation when daemon unavailable
   - [ ] Index invalidation (inotify/file watching for auto-refresh)

## Active Backlog

**Small:**
- [ ] Profiling infrastructure - measure hot paths in CLI, daemon, and Python components
- [ ] Model-agnostic naming - don't over-fit to specific LLM conventions
- [ ] Multiple agents concurrently - no requirement to join back to main stream
- [ ] Graceful failure - handle errors without crashing, provide useful feedback
- [ ] Revisit CLAUDE.md dogfooding section - tools should be self-evident, not need instructions
- [ ] MCP response ephemeral handling - large responses should stream/page instead of filling context
- [x] MCP DWIM: route natural language to `dwim` command, only use exact CLI syntax as fallback
- [x] Tune DWIM: "structure" → `summarize` first, `skeleton` for details

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

## Deferred

- Log format adapters - after loop work validates architecture

## Notes

### Key Findings
- **86.9% token reduction** using skeleton vs full file (dwim.py: 3,890 vs 29,748 chars)
- **12x output token reduction** with terse prompts (1421 → 112 tokens)
- **90.2% token savings** in composable loops E2E tests

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
