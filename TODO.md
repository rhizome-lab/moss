# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs.

## Next Up

1. **Continue CLI Migration** - Migrate remaining CLI commands to MossAPI
   - Pattern: Replace `from moss.X import Y` with `MossAPI.for_project()`
   - 16 done (tree, complexity, health, skeleton, clones, security, check_refs, git_hotspots, external_deps, weaknesses, rag, dwim, anchors, cfg, deps, context)
   - Priority next: query, search

2. **Agent learning** - Record mistakes in `.moss/lessons.md`
   - Capture patterns from session failures
   - Surface relevant lessons during similar operations

## Active Backlog

**Small:**
- [ ] Model-agnostic naming - don't over-fit to specific LLM conventions
- [ ] Multiple agents concurrently - no requirement to join back to main stream

**Medium:**
- [ ] Study Goose's context revision (`crates/goose/src/`)
- [ ] Agent learning - record mistakes in `.moss/lessons.md`

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

### Design Principles
See `docs/philosophy.md` for full tenets. Key goals:
- Minimize LLM usage (structural tools first)
- Maximize useful work per token
- Low barrier to entry, works on messy codebases
