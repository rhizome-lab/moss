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
  - **Multi-tool orchestration** (don't reinvent, aggregate):
    - semgrep: SAST pattern matching, large community rule registry
    - Snyk: SCA (dependency vulnerabilities), AI-driven
    - bandit: Python-specific security checks
    - CodeQL: deep semantic/dataflow (optional, GitHub Enterprise)
    - ast-grep: custom structural security rules
  - Plugin architecture: tools configured in `.moss/security.yaml`
  - Unified output: aggregate findings from all tools
  - OWASP Top 10 / CWE Top 25 coverage via tool combination
  - Report severity, suggest fixes, dedupe overlapping findings
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
- [x] `moss lint` - Unified linting interface (basic version implemented):
  - [x] Run all configured linters with unified output
  - [ ] Configure linters (ruff, mypy, etc.) from a single place
  - [ ] Suggest linter configurations based on project structure
  - [ ] Auto-fix where possible
  - [ ] Manage scripts/commands (list available, run, explain)
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
- [ ] `moss rules` - Custom structural analysis framework:
  - User-defined rules as Python files (LLM-writable, type-checkable)
  - Pre-filters for performance (don't run every rule on every AST node)
  - Pattern: `@rule(backend="ast-grep")` decorator + check function
  - Example rules: naming conventions, structural smells, project-specific checks
  - Key insight: "code smell detection with LLMs" is backwards - build discoverability
    tools so LLMs can *query* for smells, not scan every token
  - Rules are testable, importable Python - not a novel DSL
  - **Multi-backend architecture** (ast-grep isn't enough for everything):
    - `ast-grep` plugin: structural patterns (wrap existing tool)
    - `pyright` plugin: type-aware rules
    - `deps` plugin: cross-file analysis (uses moss deps/call graph)
    - `dataflow` plugin: taint tracking (maybe wrap Pysa?)
    - `python` plugin: escape hatch for arbitrary checks
  - **Multi-backend composition**: rules can require multiple backends
    - `@rule(backend=["ast-grep", "pyright"])` for combined analysis
    - Each backend contributes to shared context, rule combines them
  - **Context detection**: auto-classify code context (test, example, CLI, library)
    - Structural detection: imports pytest → test context
    - Path heuristics: `/tests/` → test context
    - User-extensible: define what "test context" means per-project
    - Rule scoping: `@rule(context="not:test")` skips test code
- [ ] `moss clones` - Structural similarity via hashing:
  - Normalize AST subtrees: replace variable names with positional placeholders ($1, $2, $3)
  - Hash normalized structure → same hash = same structure
  - Elision levels for different granularity:
    - Level 0: names only (exact clones)
    - Level 1: + literals (same structure, different strings/numbers)
    - Level 2: + call targets (same pattern, different functions called)
    - Level 3: + expressions (control flow skeleton only)
  - Use case: "These 5 functions share structure at level 2 → potential abstraction"
  - Feeds into `moss patterns` for abstraction suggestions
  - No LLM for detection; LLM optional for suggesting *which* abstraction fits

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

### Research Backlog (Future Sessions)

Topics to explore when time permits. See also `docs/research-low-priority.md` for
resource-intensive topics (inference optimization, fine-tuning, training).

**RAG & Retrieval:**
- [ ] RAG specifically for code (vs general RAG)
- [ ] Graph-based retrieval (call graphs, dependency graphs)
- [ ] Hybrid retrieval (embeddings + keyword + AST)
- [ ] Cross-file context assembly strategies

**Developer Experience:**
- [ ] Commit message generation
- [ ] Changelog generation from commits
- [ ] Release notes automation
- [ ] Code review comment templates

**Code Quality:**
- [ ] Technical debt estimation
- [ ] Merge conflict resolution
- [ ] Code deduplication / clone detection (see `moss clones` below)
- [ ] Side effect tracking / purity analysis:
  - Detect pure vs impure functions
  - Track IO operations (file, network, env vars)
  - Mutation tracking (what does this modify?)
  - Use cases: safe refactoring, parallelization hints, smell detection
    ("this pure function calls impure function")

**Specialized Domains:**
- [ ] Natural language to SQL/queries
- [ ] API design and generation
- [ ] Schema migration generation
- [ ] Infrastructure as Code (Terraform, Pulumi)

**Compliance & Governance:**
- [ ] License compliance checking
- [ ] API versioning and breaking change detection
- [ ] Accessibility (a11y) in generated code
- [ ] Internationalization (i18n) patterns

**Evaluation & Benchmarks:**
- [ ] CodeContests and competitive programming
- [ ] APPS benchmark deep dive
- [ ] Cross-language evaluation
- [ ] Long-context code understanding

### Distilled Learnings → Consolidated Development Phases

**The research in `docs/prior-art.md` (23+ topics) consolidates into ~6 coherent phases.**
Not 23 phases of work - many topics overlap or are extensions of each other.

---

#### Phase A: Code Quality Framework

**Core feature: `moss rules`** - see Codebase Analysis Gaps section above.

This phase unifies: code smells, refactoring detection, automated review, naming conventions,
pattern detection, clone detection, and custom linting.

| Research Topic | How It Fits |
|----------------|-------------|
| Automated Code Refactoring | Rules detect refactoring opportunities; `RefactoringMirror` pattern |
| Automated Code Review | Rules + context = review comments; integrate as `moss review` |
| Code smell detection | Structural rules, not LLM scanning (discoverability, not reading) |
| Clone detection | `moss clones` feeds into pattern detection |
| Side effect tracking | Purity analysis as a rule backend |

**Sub-features:**
- [x] `moss lint` - unified linting (basic: runs ruff; TODO: basedpyright, custom rules)
- [ ] `moss patterns` - architectural pattern detection
- [ ] `moss clones` - structural similarity via hashing
- [ ] `moss weaknesses` - gap analysis
- [ ] `moss review` - PR analysis using rules + LLM for suggestions
- [ ] `moss refactor` - detect opportunities, apply with rope/libcst
  - Extract method, rename, inline, move
  - Mine project history for anti-patterns (ECO approach)
  - Validate: tests pass before/after

**From prior-art:** MANTRA (multi-agent refactoring), ECO (Google's pattern mining),
RefactoringMiner (for validation), ast-grep/semgrep/CodeQL as backends.

---

#### Phase B: LLM-Assisted Code Operations

Operations that benefit from LLM understanding but should integrate with structural tools.

| Feature | Description | Prior Art |
|---------|-------------|-----------|
| `moss gen-tests` | Generate tests for uncovered code | TestGen-LLM (Meta), mutation-guided |
| `moss document` | Generate/update docstrings | doc-comments-ai, RepoAgent |
| `moss explain <symbol>` | Explain any code construct | Use skeleton as context |
| `moss localize <test>` | Find buggy code from failing test | MemFL, AgentFL, FaR-Loc |
| `moss migrate --to <lang>` | Cross-language translation | AST-to-AST preferred |

**Implementation notes:**
- `gen-tests`: Extend `moss coverage` with `--generate` flag
  - Use existing tests as few-shot examples
  - Mutation testing integration (mutmut) to validate quality
  - Target: functions with low coverage
- `document`: Use skeleton view for context; update docstrings in place
- `explain`: Dynamic docs in TUI/LSP (hover for explanation)
- `localize`: Integrate with validator loop (on test failure, localize first)
  - Iterative narrowing with each attempt
- `migrate`: Function-by-function, not whole-file
  - Generate type mappings between languages
  - Tests as equivalence oracles

**From prior-art:** Meta's 75% build rate for gen-tests, 66% of debug time on localization,
47.3% best-case translation pass rate (set expectations appropriately).

---

#### Phase C: Context & Memory

Managing context across sessions, enabling semantic search, learning from mistakes.

| Feature | Description | Prior Art |
|---------|-------------|-----------|
| RAG / Semantic Search | Vector search over codebase | Greptile insights, code embeddings |
| Token Budget Management | Auto-compact when context grows | Speculative context masking |
| Agent Learning | Record mistakes, avoid repeating | Lessons file, learning triggers |
| Sessions | Resumable, observable units of work | See Sessions section below |

**Implementation notes:**
- RAG: Already in TODO; use skeleton as natural language descriptions
  - Per-function chunking, not per-file
  - Code embedding models: CodeBERT, StarEncoder, Jina-code
  - Hybrid search: embeddings + keyword + AST structure
- Token budgets: `context_memory.py` exists; add auto-compact trigger
- Learning: `.moss/lessons.md` for per-repo memory
  - Triggers: validation failures, rollbacks, user corrections
- Sessions: Store in `.moss/sessions/<name>/`
  - JSONL events + markdown summaries
  - Integrate with checkpoints

**Key insight from Greptile:** Simple semantic search fails on code because meaning
depends on context (imports, types, call sites). Must translate to NL first or
use structural chunking.

---

#### Phase D: Agent Infrastructure

Multi-agent coordination, trust levels, checkpoints.

| Feature | Description | Prior Art |
|---------|-------------|-----------|
| Architect/Editor Split | Separate reasoning from editing | Aider (85% benchmark) |
| Configurable Agent Roles | User-defined micro-agents | OpenHands |
| Trust Levels | Fine-grained, composable permissions | Warp's Dispatch mode |
| Checkpoint/Rollback UX | Expose Shadow Git to users | Claude Code |
| Codebase Indexing | Proactive embedding | Cursor |

**Implementation notes:**
- Architect/Editor: Planner agent uses reasoning model, Executor uses fast model
  - Generalize to N-level hierarchies
- Agent roles: Config in `.moss/agents/`, schema-validated
  - Each agent: system prompt, tool subset, constraints
- Trust levels: Already designed in detail above
  - Pattern learning, scope-based, time-bounded, rollback-aware
- Checkpoints: `moss checkpoint create/list/diff/merge/abort` - [IMPLEMENTED]
  - [ ] Add `CheckpointAPI` to `MossAPI` (currently CLI-only)
- Indexing: Auto-index on project load (background)
  - Integrate with RAG

**From prior-art:** SWE-agent proves interface design > model scaling.
Moss's structural awareness is the differentiator.

**Parallel Agents:**
- [ ] **Multi-subtree parallelism**: Run agents concurrently on independent subtrees
  - Detect independence via `moss deps` (no shared dependencies = safe to parallelize)
  - Example: refactor `src/api/` and `src/utils/` in parallel if no cross-deps
  - Merge results, detect conflicts, resolve or escalate
- [ ] **Intra-subtree parallelism**: Parallelize work within a single subtree
  - Pipeline stages: analyze → plan → implement → validate (some stages parallelizable)
  - Speculative execution: start likely next steps before current completes
  - Fan-out: multiple agents propose solutions, pick best (tournament style)
  - Divide-and-conquer: split large file into functions, parallelize per-function work
- [ ] **Coordination primitives**:
  - Locks: prevent concurrent edits to same file/symbol
  - Barriers: sync points where agents wait for each other
  - Channels: typed message passing between agents
  - Conflict detection: structural diff to detect overlapping edits
- [ ] **Resource management**:
  - Token budget allocation across parallel agents
  - Rate limiting for API calls
  - Priority queues for agent scheduling

**Already implemented:**
- [x] `moss/autofix.py` - FixEngine with safety classification
- [ ] Integrate autofix into PatchAPI.apply() flow
- [ ] Run `ruff check --fix` on validation failures automatically

---

#### Phase E: Protocols & Integrations

External integrations and protocol support.

| Feature | Description | Priority |
|---------|-------------|----------|
| ACP Server | Zed/JetBrains integration via Agent Client Protocol | HIGH |
| MCP Improvements | Resource providers, prompt templates | MEDIUM |
| GitHub Integration | `moss review` as GitHub Action | MEDIUM |
| Browser Automation | Playwright/Selenium for UI testing | LOW |
| Remote Agent Management | Web dashboard for monitoring | LOW |

**Implementation notes:**
- ACP: `moss.acp_server` module, JSON-RPC 2.0 over stdio
  - Map moss tools to ACP capabilities
- MCP: Already works; add resource providers (file summaries, overview)
- GitHub: SARIF output for CodeQL/security findings integration
- Browser: Only if screenshot-to-code (`moss ui-to-code`) becomes priority

---

#### Phase F: Evaluation & Validation

Prove that moss's approach actually works.

| Task | Purpose |
|------|---------|
| SWE-bench harness | Standard benchmark for code agents |
| Anchor patching comparison | vs search/replace vs unified diff |
| Skeleton value measurement | Does structural context improve accuracy? |
| Security iteration tracking | Monitor vuln count across refinements |

**Implementation notes:**
- Install SWE-bench: `pip install swebench`
- Start with Lite subset (faster iteration)
- Measure: Does skeleton context improve patch accuracy?
- Measure: Does anchor-based patching reduce failed applies?

**From prior-art:** 12.47% pass@1 for SWE-agent. Moss should aim to match or beat
by leveraging structural awareness.

---

#### What's NOT a phase (config/extensions):

These are configuration options or small extensions, not separate phases:

- **Local LLM support**: Config option in existing LLM layer (Ollama integration)
- **FIM code completion**: Use in SketchGenerator, not standalone feature
- **Prompt engineering**: Improve existing prompts, not new feature
- **Multi-modal (UI-to-code)**: Nice-to-have, not core
- **Formal verification**: Long-term research, not near-term phase

---

#### Summary: 23 Topics → 6 Phases + Config

| Phase | Main Features | Est. Scope |
|-------|---------------|------------|
| A: Code Quality | `rules`, `security`, `lint`, `patterns`, `clones`, `review`, `refactor` | Large |
| B: LLM Operations | `gen-tests`, `document`, `explain`, `localize`, `migrate` | Medium |
| C: Context & Memory | RAG, token budgets, learning, sessions | Medium |
| D: Agent Infrastructure | Architect/Editor, roles, trust, checkpoints, indexing | Large |
| E: Protocols | ACP, MCP improvements, GitHub, browser | Small-Medium |
| F: Evaluation | SWE-bench, benchmarks, measurements | Small |

Many features within each phase share infrastructure. Build the phase foundation
first, then features become incremental.

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
