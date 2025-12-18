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
- [ ] Documentation: how to add to Claude Code's MCP config
- [ ] Add missing tools: `complexity`, `check-refs`, `git-hotspots`, `external-deps`
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
- [x] MCP server now uses generated tools (28 tools from MossAPI introspection)
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
- [ ] Add drift check to CI workflow (GitHub Actions)

### Non-LLM Code Generators

Alternative synthesis approaches that don't rely on LLMs. See `docs/synthesis-generators.md` for details.

#### High Priority
- [ ] `EnumerativeGenerator` - enumerate ASTs, test against examples (Escher/Myth)
- [ ] `ComponentGenerator` - combine library functions bottom-up (SyPet/InSynth)
- [ ] `SMTGenerator` - Z3-based type-guided synthesis (Synquid)

#### Medium Priority
- [ ] `PBEGenerator` - Programming by Example (FlashFill/PROSE)
- [ ] `SketchGenerator` - fill holes in user templates (Sketch/Rosette)
- [ ] `RelationalGenerator` - miniKanren-style logic programming

#### Research/Experimental
- [ ] `GeneticGenerator` - evolutionary search (PushGP)
- [ ] `NeuralGuidedGenerator` - small model guides enumeration (DeepCoder)
- [ ] `BidirectionalStrategy` - λ²-style type+example guided search

### Multi-Language Expansion

- [ ] Full TypeScript/JavaScript synthesis support
- [ ] Go and Rust synthesis strategies

### CLI Output Enhancement

Remaining token-efficient output features:

- [ ] `--query EXPR` flag - relaxed DWIM syntax for flexible querying (needs design work)
- [ ] Format strings for custom output templates

### Codebase Analysis Gaps

Tools we have:
- Project health: `overview`, `health`, `metrics`
- Structure: `skeleton`, `summarize`, `deps`
- Dependencies: `external-deps` (vulns, licenses, weight)
- Quality: `check-docs`, `check-todos`, `check-refs`
- Coverage: `coverage` (pytest-cov stats)
- Complexity: `complexity` (cyclomatic per function)
- Git analysis: `git-hotspots` (frequently changed files)

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

See `docs/prior-art.md` for research references.

- [ ] **Investigate SWE-agent**: How does it work? What's their ACI? Performance on SWE-bench?
- [ ] **Investigate Aider**: Edit format, multi-file handling, git integration patterns
- [ ] **Investigate OpenHands**: Sandbox/runtime approach, architecture

Key question: Do they do something moss should adopt, or is moss's structural-awareness approach better?

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
