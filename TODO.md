# Moss Roadmap

See `CHANGELOG.md` for completed work. See `docs/` for design docs.

## Next Up

1. **Async task documentation** - Document how to manage background tasks
   - Terminals, shells, agents running in background
   - Waiting for completion, handling hangs, when to join

2. **Recursive self-improvement** - Loops that improve other loops
   - Critic loop reviewing loop definitions
   - Start with: optimize a docstring loop

3. **Codebase search API** - Dogfood moss search instead of raw grep/glob
   - Semantic search via RAG, structural via skeleton/anchors

4. **Guessability metrics** - Evaluate codebase structure quality
   - Can you guess module names from functionality?

## Active Backlog

**Small:**
- [ ] Module name DWIM - fuzzy matching for file/module names
- [ ] Model-agnostic naming - don't over-fit to specific LLM conventions
- [ ] Multiple agents concurrently - no requirement to join back to main stream
- [ ] Async task lifecycle - spawn, wait, timeout, cancel patterns

**Medium:**
- [ ] Complexity hotspots - 60 functions with complexity ≥15

**Large:**
- [ ] CLI from MossAPI - migrate cli.py to generated interface

## Future Work

### Context & Memory
- [ ] Study Goose's context revision (`crates/goose/src/`)
- [ ] Extend context_memory.py with active pruning
- [ ] Agent learning - record mistakes in `.moss/lessons.md`
- [ ] Sessions as first-class - resumable, observable work units

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
- [ ] `moss clones` - structural similarity via hashing
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

- CLI from MossAPI (large) - wait for API stability
- Log format adapters - after loop work validates architecture

## To Consolidate

New ideas captured here before proper categorization:
- Async task management: terminals, waiting, handling completion
- Dealing with hanging tasks: heuristics like lack of output
- Note: not 100% reliable - some tools (servers) are long-running without output
- Need timeout strategies, progress detection, graceful cancellation
- Multiple agents can be active simultaneously - don't need to join all back to main stream
- Completed tasks don't need to block; main work may have moved on

## Notes

### Key Findings
- **86.9% token reduction** using skeleton vs full file (dwim.py: 3,890 vs 29,748 chars)
- **12x output token reduction** with terse prompts (1421 → 112 tokens)
- **90.2% token savings** in composable loops E2E tests

### Dogfooding Observations (Dec 2025)
- `skeleton_format` / `skeleton_expand` - very useful
- Missed: used `ls` instead of `tree_format`, `Grep` instead of RAG
- Need: `moss search` dogfoodable API, more DWIM aliases

### Design Principles
See `docs/philosophy.md` for full tenets. Key goals:
- Minimize LLM usage (structural tools first)
- Maximize useful work per token
- Low barrier to entry, works on messy codebases
