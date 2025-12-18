# Moss Roadmap

See `CHANGELOG.md` for completed features (Phases 15-29).

See `~/git/prose/moss/` for full synthesis design documents.

## Future Work

### MCP Server (EXISTS - needs dogfooding)

**Already implemented** in `src/moss/mcp_server.py`:
- Tools: `skeleton`, `anchors`, `cfg`, `deps`, `context`, `apply_patch`, `analyze_intent`, `resolve_tool`, `list_capabilities`
- Entry point: `python -m moss.mcp_server` or `moss mcp-server`

**Why aren't we using it?** Need to actually configure Claude Code to use it.

Still needed:
- [x] Documentation: how to add to Claude Code's MCP config (see `docs/getting-started/mcp-integration.md`)
- [x] Add missing tools: `complexity`, `check-refs`, `git-hotspots`, `external-deps`
- [ ] Resource providers: file summaries, codebase overview
- [ ] Prompt templates: "understand this file", "prepare for refactor"
- [ ] Test it works end-to-end with Claude Code

### Interface Generators (Single Source of Truth)

**Goal**: Library is the source of truth. Run a generator → server code stays in sync. No manual maintenance of multiple server implementations.

**Generators available:**
- `moss.gen.introspect` - Introspect MossAPI structure
- `moss.gen.http` - Generate FastAPI routes from API
- `moss.gen.mcp` - Generate MCP tools from API
- `moss.gen.cli` - Generate CLI commands from API

**CLI:** `moss gen --target=<mcp|http|cli|openapi> [--output FILE] [--list]`

**Completed:**
- [x] MCP server now uses generated tools (38 tools from MossAPI introspection)
- [x] DWIMAPI added to MossAPI (analyze_intent, resolve_tool, list_tools, get_tool_info)
- [x] `moss gen` CLI command for regenerating interfaces
- [x] HTTP server (`server/app.py`) now uses generated routes via HTTPGenerator
- [x] Shared serialization module (`moss.gen.serialize`) for consistent API output
- [x] HTTPExecutor for executing API methods with proper parameter handling

**Still needed:**
- [ ] Unix socket transport option
- [ ] Documentation: how the generation pipeline works

**Completed:**
- [x] `moss.gen.tui` - Textual-based TUI (`moss tui` command)
- [x] `moss.gen.lsp` - LSP workspace commands via pygls
- [x] `moss.gen.grpc` - Protocol Buffers + servicer generation

**CI/Automation:**
- [x] Drift detection script (`scripts/check_gen_drift.py`) - compares generated OpenAPI/MCP specs to committed versions
- [x] Add drift check to CI workflow (GitHub Actions)

### Non-LLM Code Generators

Alternative synthesis approaches that don't rely on LLMs. See `docs/synthesis-generators.md` and `docs/prior-art.md` for details.

#### High Priority
- [ ] `EnumerativeGenerator` - enumerate ASTs, test against examples (Escher/Myth)
  - Bottom-up AST enumeration with special handling for conditionals/recursion
  - Use Python type hints as refinement constraints (from Myth's approach)
  - Combine types + examples: tests = examples, type hints = types
- [ ] `ComponentGenerator` - combine library functions bottom-up (SyPet/InSynth)
  - Build type graph from available functions (use `moss deps` + `external-deps`)
  - Petri net representation: places=types, transitions=methods, tokens=variables
  - Two-phase: sketch generation via reachability, then SAT for argument binding
- [ ] `SMTGenerator` - Z3-based type-guided synthesis (Synquid)
  - Translate Python specs to Z3 constraints (`pip install z3-solver`)
  - Use docstrings/contracts as refinement types
  - Bidirectional type propagation (top-down + bottom-up)

#### Medium Priority
- [ ] `PBEGenerator` - Programming by Example (FlashFill/PROSE)
- [ ] `SketchGenerator` - fill holes in user templates (Sketch/Rosette)
- [ ] `RelationalGenerator` - miniKanren-style logic programming

#### Research/Experimental
- [ ] `GeneticGenerator` - evolutionary search (PushGP)
- [ ] `NeuralGuidedGenerator` - hybrid LLM + enumeration (2024 research)
  - LLM proposes (possibly incorrect) solutions
  - Build probabilistic CFG from LLM proposals
  - Use pCFG to guide enumerative search in CEGIS loop
  - 2-way info exchange: LLM → enumerator → LLM (80% benchmark completion)
- [ ] `BidirectionalStrategy` - λ²-style type+example guided search

### Multi-Language Expansion

- [ ] Full TypeScript/JavaScript synthesis support
- [ ] Go and Rust synthesis strategies

### CLI Output Enhancement

Remaining token-efficient output features:

- [ ] `--query EXPR` flag - relaxed DWIM syntax for flexible querying (needs design work)
- [ ] Format strings for custom output templates

### Security Validation (CRITICAL - see docs/prior-art.md)

**Problem**: 45% of AI-generated code has security vulnerabilities (Veracode 2025).
Moss must not contribute to this problem.

- [ ] **`moss security`** - Security analysis command:
  - Run bandit, semgrep automatically
  - AST-based detection of vulnerable patterns
  - OWASP Top 10 / CWE Top 25 checks
  - Report severity, suggest fixes
- [ ] **Validator integration**: Run security checks in synthesis loop
- [ ] **Iteration tracking**: Monitor vuln count across refinements (37.6% increase after 5 iterations is alarming)
- [ ] **Security-aware prompting**: Include security requirements in synthesis specs
- [ ] **Warn on sensitive code**: Flag auth, crypto, input handling for review

### Codebase Analysis Gaps

Tools we have:
- Project health: `overview`, `health`, `metrics`
- Structure: `skeleton`, `summarize`, `deps`
- Dependencies: `external-deps` (vulns, licenses, weight)
- Quality: `check-docs`, `check-todos`, `check-refs`
- Coverage: `coverage` (pytest-cov stats)
- Complexity: `complexity` (cyclomatic per function)
- Git analysis: `git-hotspots` (frequently changed files)
- **Security**: (NEW) `security` for vulnerability detection

Potential additions:
- [ ] Architecture diagrams from dependency graph
- [ ] `moss lint` - Unified linting interface:
  - Configure linters (ruff, mypy, etc.) from a single place
  - Suggest linter configurations based on project structure
  - Run all configured linters with unified output
  - Auto-fix where possible
  - Manage scripts/commands (list available, run, explain)
- [ ] `moss patterns` - Detect and analyze architectural patterns:
  - Plugin systems (Protocol + Registry + Entry Points)
  - Factory patterns, strategy patterns, adapter patterns
  - Inconsistent patterns (e.g., some registries use entry points, others don't)
  - Hardcoded implementations that could be plugins
  - Coupling analysis (which modules know about each other)
  - Report: "X uses plugin pattern, Y could benefit from it"
- [ ] `moss weaknesses` / `moss gaps` - Identify architectural weaknesses and gaps:
  - Hardcoded assumptions (e.g., parsing only supports one format)
  - Missing abstractions (e.g., no plugin system where one would help)
  - Tight coupling between components
  - Single points of failure
  - Missing error handling patterns
  - Inconsistent patterns across similar code
  - Technical debt indicators
  - Self-analysis: moss should be able to identify its own architectural gaps
    (eating our own dogfood, providing actionable feedback during development)

### RAG / Semantic Search

Instead of reading entire files (like TODO.md) every session, use semantic search:

- [ ] `moss rag index <path>` - build vector index of documents/code
- [ ] `moss rag search <query>` - semantic search across indexed content
- [ ] Auto-index project docs (README, TODO, CLAUDE.md, docs/)
- [ ] Integration with context_memory.py summaries
- [ ] Could use local embeddings (sentence-transformers) or API (OpenAI, Voyage)
- [ ] Chunk strategy: by section/function, not fixed token windows

Use cases:
- "What did we decide about X?" → search TODO.md, docs/, past session logs
- "Where is Y implemented?" → search codebase with semantic understanding
- Agent context loading: retrieve relevant context for current task

### Agent Log Analysis

Manual analysis complete - see `docs/log-analysis.md` for methodology and insights.
Basic automation: `moss analyze-session <path>` parses Claude Code JSONL logs.
Preference extraction: `moss extract-preferences` and `moss diff-preferences` are now implemented (see Phase 31 in CHANGELOG.md).

### Self-Analysis / Dogfooding Meta

Can moss answer questions about itself? Examples:
- "Do we have a server?" → should be answerable by `moss skeleton` + `moss deps`
- "What tools exist?" → `moss overview` or a new `moss inventory` command
- "What's implemented vs TODO?" → compare code to TODO.md

This is dogfooding at the meta level - using moss to understand moss.

### Competitor Analysis

See `docs/prior-art.md` for detailed research (updated Dec 2025).

**Completed Research:**
- [x] SWE-agent: ACI design, guardrails, 12.47% pass@1 on SWE-bench
- [x] Aider: Architect/Editor split, edit formats, PageRank repo mapping
- [x] OpenHands: Event stream, multi-agent delegation, micro-agents
- [x] Claude Code: Hooks, checkpoints, MCP integration, Agent SDK
- [x] Cursor: Codebase indexing, agent mode, $9.9B validation

**Patterns to Adopt from Competitors:**
- [ ] **Architect/Editor split** (Aider) - separate reasoning from editing with two LLM calls
- [ ] **Guardrails at edit time** (SWE-agent) - integrate linting before changes commit
- [ ] **Checkpoint/rollback UX** (Claude Code) - expose Shadow Git more explicitly to users
- [ ] **Micro-agents** (OpenHands) - task-specialized agents using shared infrastructure
- [ ] **Codebase indexing** (Cursor) - enhance RAG with proactive embedding

**Benchmarking TODO:**
- [ ] Implement SWE-bench evaluation harness
- [ ] Compare anchor-based patching vs search/replace vs diff formats
- [ ] Measure structural context (skeleton) value vs raw file context

**Additional IDE/Tool Research:** ✓ Completed Dec 2025 - see `docs/prior-art.md`
- [x] Warp (AI-native terminal) - Dispatch mode, Active AI, multi-model
- [x] Zed (GPU-accelerated editor) - ACP protocol, Edit Prediction, B-tree buffers
- [x] Windsurf (Codeium's IDE) - Cascade, Supercomplete, Rules system
- [x] Google Antigravity - Agent-first IDE, Manager View, multi-agent dispatch
- [x] VS Code Copilot - Agent Mode, MCP integration (128 tool limit), LSP→MCP lineage

**Review with user (async, don't block):**
- [ ] Review IDE/tool research (Warp, Zed, Windsurf, Antigravity, VS Code Copilot)
- [ ] Review synthesis research (Escher, Myth, SyPet, Synquid, PROSE, Sketch, miniKanren, DeepCoder)
- [ ] Review trust levels design
- [ ] Review sessions-as-first-class design

**New patterns to adopt from IDE research:**
- [ ] **Smart Trust Levels** (inspired by Warp's Dispatch mode) - see design below
- [ ] **ACP Server** (HIGH PRIORITY) - implement Agent Client Protocol for Zed/JetBrains integration
  - Create `moss.acp_server` module
  - JSON-RPC 2.0 over stdio
  - Map moss tools to ACP capabilities
  - See `docs/prior-art.md` for protocol details
- [ ] **Intent Prediction** (Windsurf's Supercomplete) - predict what user wants, not just next token
- [ ] **Manager View** (Antigravity) - UI for orchestrating multiple concurrent agents
- [ ] **Browser Automation** (Antigravity) - add Playwright/Selenium tools for UI testing

**Smart Trust Levels Design:**

Most IDEs use basic "approve X command?" prompts - safe but interrupts the agentic loop.
We want *smarter* security that's still robust but doesn't ask yes/no for everything.

**Core Principle**: Fine-grained, composable trust. Not just 4 levels - users define their own.

**Built-in Presets** (as starting points):
1. **Full Trust** (dispatch mode): No confirmations, agent runs freely
2. **High Trust** (default for known codebases):
   - Auto-approve: read, search, lint, test, git status/diff/log
   - Confirm: write, delete, git commit/push, external commands
3. **Medium Trust** (default for new codebases):
   - Auto-approve: read, search
   - Confirm: write, delete, any command execution
4. **Low Trust** (sandbox mode): Confirm everything, useful for demos/untrusted

**Custom Trust Levels** (user-defined):
```yaml
# .moss/trust.yaml
levels:
  my-dev-level:
    inherit: high  # Start from a preset
    allow:
      - "bash:ruff *"      # Allow any ruff command
      - "bash:pytest *"    # Allow any pytest command
      - "write:src/**"     # Allow writes to src/
    deny:
      - "write:*.env"      # Never auto-approve .env writes
      - "bash:rm -rf *"    # Always confirm destructive deletes
    confirm:
      - "write:config/*"   # Ask for config changes
```

**Composable Trust** (combine levels with merge strategies):
```yaml
# Combine multiple levels with different merge strategies
levels:
  prod-deploy:
    compose: [high, ci-safe, team-rules]
    merge: intersection   # Options below

# Merge strategies:
# - intersection: Must be allowed by ALL levels (most restrictive)
# - union: Allowed by ANY level (most permissive)
# - first: First level that has an opinion wins
# - last: Last level that has an opinion wins (overrides)
# - max: Most permissive decision wins (allow > confirm > deny)
# - min: Least permissive decision wins (deny > confirm > allow)
```

Example use cases:
- `intersection`: Production - must pass team + security + compliance
- `union`: Development - allow anything any role permits
- `first`: Layered defaults (user → project → global)
- `last`: Override chain (base → extensions → local)
- `max`: "If anyone trusts it, trust it"
- `min`: "If anyone denies it, deny it"

Smart features beyond basic approve/deny:
- [ ] **Pattern learning**: "You approved `ruff check` 10 times, auto-approve it?"
- [ ] **Scope-based**: "Trust writes to `src/` but confirm for `config/`"
- [ ] **Time-bounded**: "Trust for this session" vs "Trust permanently"
- [ ] **Rollback-aware**: "This can be undone via Shadow Git" (lower risk = less friction)
- [ ] **Batch approval**: "Approve all 5 pending writes at once?"
- [ ] **Explain risk**: Show what command does, why it's flagged, what could go wrong
- [ ] **Glob patterns**: `write:src/**/*.py` for fine-grained path matching
- [ ] **Command patterns**: `bash:git *` to trust all git commands

Key insight: The goal isn't "maximum safety" - it's *appropriate* safety that doesn't
destroy the productivity gains of agentic coding.

Key question answered: Interface design matters more than model scaling (SWE-agent proves this). Moss's structural-awareness approach is differentiated but unproven - needs benchmark validation.

### Distilled Learnings → Implementation Plan

From the competitor analysis, refined with project-specific insights:

**1. Multi-Agent Hierarchies (inspired by Aider's Architect/Editor)**
Don't limit to two-level splits - support N-level agent hierarchies:
- Configurable agent delegation, not hardcoded Architect→Editor
- Example: Planner → Subtask Agents → Executors (3+ levels)
- Each level can use different models/prompts/tool subsets
- Implementation: Generalize `moss.agents` to support arbitrary delegation graphs

**2. Configurable Agent Roles (inspired by OpenHands' Micro-Agents)**
Task-specialized agents, but user-configurable rather than fixed:
- Define agents via config (system prompt, tool subset, constraints)
- Not hardcoded "DocAgent", "TestAgent" - user defines their own
- Share: context loading, tool execution, validation infrastructure
- Implementation: Agent config schema in `moss.agents`, load from `.moss/agents/`

**3. Non-LLM Auto-Fix Integration (from SWE-agent guardrails)**
**Already implemented**: `moss/autofix.py` has FixEngine with safety classification.
Still needed for patch flow integration:
- [ ] Run `ruff check --fix` automatically on validation failures
- [ ] Report what was auto-fixed (diff of deterministic changes)
- [ ] Only escalate to LLM if auto-fix fails or changes are NEEDS_REVIEW/UNSAFE
- [ ] Integrate `autofix.FixEngine` into `PatchAPI.apply()` flow
- Key insight: Deterministic fixes first, LLM only for what tools can't fix

**4. Checkpoint/Rollback UX (from Claude Code)**
Make Shadow Git's rollback capability user-visible:
- `moss checkpoint create <name>` - named snapshot
- `moss checkpoint list` - show available checkpoints
- `moss checkpoint restore <name>` - rollback to checkpoint
- Currently: Shadow Git exists but users don't know about it
- Implementation: Add `CheckpointAPI` to `MossAPI`, CLI commands

**5. Codebase Indexing (from Cursor)**
Proactive embedding for semantic search:
- Auto-index on project load (background task)
- Chunk by function/class, not fixed tokens
- Enable queries like "where is authentication handled?"
- Implementation: Enhance `RAGTool` with auto-indexing, integrate with `context_memory.py`

### Agent Learning / Memory

Agents should learn from their mistakes and successes. When moss makes an error, it should record what happened so it (or future sessions) can avoid repeating it.

**Local (per-repo) memory:**
- [ ] Record mistakes in `.moss/lessons.md` or similar
- [ ] On error, auto-add entry: what went wrong, what was tried, what worked
- [ ] Before making changes, check lessons for relevant warnings
- [ ] Format should be human-readable (markdown) and searchable

**Global (cross-repo) memory:**
- [ ] Optional `~/.moss/global_lessons.md` for patterns that apply everywhere
- [ ] Sync mechanism between local and global (promote useful local lessons)
- [ ] Privacy-aware: user controls what goes global

**Learning triggers:**
- Validation failures (syntax errors, test failures, lint errors)
- Rollbacks (shadow git reset)
- User corrections ("no, do it this way instead")
- Repeated attempts at the same thing

**Use cases:**
- "Last time I tried X on this codebase, Y happened"
- "This codebase prefers pattern A over pattern B"
- "Don't modify files in vendor/ directory"

### Sessions as First-Class Citizens

Sessions should be resumable and observable for both humans and LLMs.

**Core concept:**
- A "session" is a unit of work with state: task description, progress, checkpoints, context
- Both human users and LLM agents can start/pause/resume sessions
- Session state persists across process restarts

**Session lifecycle:**
- [ ] `moss session start <name>` - begin a named session
- [ ] `moss session list` - show active/paused sessions
- [ ] `moss session resume <name>` - continue a paused session
- [ ] `moss session status <name>` - show progress, last action, context size
- [ ] `moss session end <name>` - complete and archive session

**Session state includes:**
- Task description / goals
- Progress (completed steps, pending work)
- Checkpoints (Shadow Git snapshots)
- Context summaries (what was learned)
- Error history (for learning)

**Why this matters:**
- Long-running tasks can be interrupted and resumed
- Multiple agents can hand off work via sessions
- Humans can review/approve session progress before continuation
- Natural integration with Agent Learning (session = learning context)

**Implementation ideas:**
- Store in `.moss/sessions/<name>/`
- JSONL for events, markdown for summaries
- Integrate with checkpoint system

---

## Vision: Augmenting the Vibe Coding Loop

Moss should both **replace** and **augment** conventional AI coding assistants like Claude Code, Gemini CLI, and Codex CLI. The goal is not to compete on chat UX, but to provide the structural awareness layer that makes any agentic coding loop more reliable:

- **As a replacement**: `moss run` can orchestrate full tasks with verification loops, shadow git, and checkpoint approval
- **As an augmentation**: Tools like `moss skeleton`, `moss deps`, `moss check-refs`, `moss mutate` can be called by *any* agent to get architectural context before making changes
- **MCP integration**: `moss-mcp` exposes all capabilities to MCP-compatible agents

The key insight: vibe coding works better when the agent understands structure, not just text.

---

## Notes

### Programming as Recursive Abstraction

Core philosophy: programming is essentially recursive abstraction - we build abstractions on top of abstractions. This has implications for Moss's design:

- **Plugin architecture as abstraction**: Each plugin system is an abstraction layer that can be extended
- **Composable primitives**: Small tools compose into larger capabilities
- **Meta-level tooling**: Tools that analyze/generate other tools (e.g., `moss patterns` analyzing plugin usage)
- **Self-describing systems**: Code that can describe its own structure (skeleton, deps, etc.)

**Codebase views at arbitrary granularity**: Traditional tools have fixed levels (file, function, line), but codebase structure is fractal - you should be able to view it at any detail level:
- Codebase → directories → files → symbols → blocks → expressions
- But NOT as fixed discrete levels - the structural units (block, function, file, module) are emergent properties, not fundamental categories
- The right interface isn't a decimal (what does 0.3 mean?) - maybe:
  - Token budget: "show me this codebase in ~500 tokens"
  - Information-theoretic: "show me enough to answer question X"
  - Iterative refinement: start coarse, drill down on request
  - Semantic: "show me the public API" vs "show me the implementation"

**Implementation: Merkle tree structure** (partially implemented in `context_memory.py`)
- Already have: `DocumentSummary` with `merkle_hash`, `ContentHash`, `SummaryStore`
- **Vision**: entire codebase as one Merkle tree, from root down to expressions
  - Root (codebase) → directories → files → symbols → blocks → expressions
  - Each node: content hash + summary at multiple detail levels
  - Parent hash = f(children hashes) → changes propagate up
- Benefits:
  - Efficient change detection (hash changes propagate up)
  - Cacheable at any level (hash = cache key)
  - Natural for incremental updates
  - Can verify integrity (useful for distributed/cached views)
- Structure mirrors git's object model but for AST, not just files

**Storage: in-repo, git-diff friendly**
- Store the Merkle tree inside the codebase itself (e.g., `.moss/tree/`)
- Format must be git-friendly:
  - Text-based, not binary
  - Deterministic ordering (sorted keys)
  - One file per directory? Or single JSONL with stable line order?
  - Changes to one file should only affect that file's entry + ancestors
- Could be: `.moss/tree/{hash}.summary` or `.moss/tree/index.jsonl`
- Enables: `git diff` shows which summaries changed, reviewable in PRs
- Auto-regenerate on `moss index` or git hooks

**Still needed**:
- Extend from documents to full codebase (directories, not just files)
- Integrate with skeleton/CFG views
- Design git-friendly storage format
- Incremental update (only rehash changed subtrees)

**Rendering strategies**:
- **Budget allocation**: given N tokens, allocate to subtrees by importance (size, complexity, relevance to query)
- **Progressive disclosure**: render depth-1 first, expand on request
- **Diff-aware**: if comparing versions, show only changed subtrees in detail
- **Query-focused**: given a question, rank subtrees by relevance, show most relevant in detail

Research directions:
- [ ] Can Moss analyze its own abstraction layers? (`moss abstractions` command)
- [ ] Automatic abstraction discovery (find repeated patterns that could be factored out)
- [ ] Abstraction quality metrics (coupling, cohesion, depth)
- [ ] Tools for refactoring concrete code into abstract plugins
- [ ] Token-budgeted codebase views: render codebase to fit N tokens
- [ ] Question-driven views: "show me enough to understand how X works"
- [ ] Merkle tree codebase representation with multi-level summaries
- [ ] Integration with git object model for versioned views

### PyPI Naming Collision

There's an existing `moss` package on PyPI (a data science tool requiring numpy/pandas). Before publishing, we need to either:
- Rename the package (e.g., `moss-tools`, `moss-orchestrate`, `toolmoss`)
- Check if the existing package is abandoned and claim the name
- Use a different registry

### Remote Agent Management

Web interface for monitoring and controlling agents remotely:

- [ ] Agent manager web server (`moss serve --web`)
  - Real-time status dashboard
  - View running tasks and progress
  - Approve/reject checkpoints from mobile
  - Send commands to running agents
  - View session logs and metrics
- [ ] Mobile-friendly responsive UI
- [ ] WebSocket for live updates
- [ ] Authentication for remote access
