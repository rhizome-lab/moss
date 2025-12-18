# Prior Art & Research References

Related work that influenced moss's design or represents the competitive landscape.

## Program Synthesis

### DreamCoder
- **Paper**: [DreamCoder: Bootstrapping Inductive Program Synthesis with Wake-Sleep Library Learning](https://arxiv.org/abs/2006.08381)
- **Relevance**: Moss aims to be "DreamCoder for LLMs" - using LLMs as the synthesis engine rather than enumeration, but with similar goals of discovering reusable abstractions
- **Key ideas**:
  - Compression-based abstraction discovery
  - MDL (Minimum Description Length) scoring for abstractions
  - Library learning: extract common patterns into reusable primitives
- **Moss approach**: Instead of enumerating programs, we use LLMs with structural context. The abstraction discovery could still apply to learned preferences/patterns.

### Other Synthesis Systems

**Enumerative / Search-based:**
- **Escher/Myth**: Enumerative synthesis with examples
- **SyPet/InSynth**: Component-based synthesis (combining library functions)
- **FlashFill/PROSE**: Programming by Example
- **Sketch/Rosette**: Hole-filling in user templates

**Type-directed:**
- **Synquid**: Refinement type-guided synthesis with liquid types
- **λ² (Lambda Squared)**: Bidirectional type+example guided search
- **Idris**: Dependently typed language with proof search / auto tactics
- **Agda**: Dependently typed proof assistant, Agsy auto-search

**Logic/Relational:**
- **miniKanren**: Relational programming, run programs "backwards"
- **Prolog**: Logic programming, unification-based search

**SMT-based:**
- **Z3**: SMT solver used by many synthesis tools
- **Rosette**: Solver-aided programming (uses Z3)

See `docs/synthesis-generators.md` for how these map to moss generator plugins.

## Coding Agents (2024-2025 Landscape)

### SWE-agent (Princeton)
- **Repo**: https://github.com/swe-agent/swe-agent
- **Paper**: https://arxiv.org/abs/2405.15793 (NeurIPS 2024)
- **What it is**: Autonomous agent for GitHub issue → PR resolution

**Key Architecture Insights:**
- **Agent-Computer Interface (ACI)**: Custom interface with small set of simple actions for viewing, searching, and editing files. Crucially different from raw shell access.
- **Granular Commands**: `find_file`, `search_file`, `search_dir` with context-limited outputs (max 50 hits) to prevent context window overflow
- **Guardrails**: Integrated linter detects and prevents syntax errors at edit time, forcing corrective actions
- **Abstract Navigation**: Commands like "goto", "scroll_down" enable rapid zoom-in for fault localization

**Performance**: 12.47% pass@1 on SWE-bench (18% on Lite subset) with GPT-4 Turbo - 3-5x improvement over RAG-only approaches

**Moss Observations:**
- SWE-agent proves that **interface design matters more than model scaling** for agent performance
- Their ACI approach aligns with moss's philosophy: give agents better tools, not just more context
- Moss's structural views (skeleton, CFG) could complement SWE-agent's search commands
- Consider: moss could export an "ACI" that provides skeleton-aware navigation

### Aider
- **Repo**: https://github.com/paul-gauthier/aider
- **Site**: https://aider.chat
- **What it is**: AI pair programming CLI with git integration

**Key Architecture Insights:**
- **Architect/Editor Mode** (Sept 2024): Separates "code reasoning" from "code editing" into two LLM calls. Achieved SOTA 85% on their benchmark.
  - Architect: Plans the solution (can use o1/reasoning models)
  - Editor: Applies changes in proper format (can use cheaper/faster models)
- **Edit Formats**: Multiple strategies (diff, whole-file, search/replace) adapted to model capabilities
- **Repository Mapping**: PageRank-based to fit large codebases into token limits
- **Chat Modes**: code (default), architect (planning), ask (Q&A without changes)

**2024-2025 Timeline**: Voice interface, GUI, file watching, thinking tokens support

**Moss Observations:**
- Architect/Editor split is powerful - moss could use a "Planner" + "Executor" pattern
- Their edit format problem is exactly what moss's anchor-based patching solves
- PageRank repo mapping is interesting; moss's skeleton view serves similar purpose
- Git integration patterns worth studying - aider auto-commits like moss envisions

### OpenHands (formerly OpenDevin)
- **Repo**: https://github.com/All-Hands-AI/OpenHands
- **Paper**: https://arxiv.org/abs/2407.16741 (ICLR 2025)
- **What it is**: Open platform for AI software developers as generalist agents

**Key Architecture Insights:**
- **Event Stream Architecture**: Chronological collection of actions and observations
- **Sandbox Runtime**: Docker-sandboxed OS with bash shell, web browser, IPython server
- **CodeAct**: Core interaction through `IPythonRunCellAction` and `CmdRunAction` for arbitrary code/bash execution
- **Agent Hub**: 10+ implemented agents including specialists for web browsing and code editing
- **Multi-Agent Delegation**: `AgentDelegateAction` allows generalist to delegate to specialists
- **Micro Agents**: Task-specialized agents that reuse generalist infrastructure

**Moss Observations:**
- Event stream architecture aligns well with moss's event bus design
- Their multi-agent delegation via `AgentDelegateAction` is similar to moss's ticket-based agent model
- Sandbox approach is important for safety; moss's Shadow Git serves similar purpose for git operations
- Agent Hub concept maps to moss's plugin architecture

### Claude Code (Anthropic)
- **Site**: https://www.anthropic.com/claude-code
- **Docs**: https://docs.anthropic.com/en/docs/claude-code/overview
- **What it is**: Anthropic's official CLI agent for coding

**Key Architecture Insights:**
- **Design Philosophy**: "Low-level and unopinionated" - raw model access without forced workflows
- **Core Loop**: gather context → take action → verify work → repeat
- **Shell-Native**: Inherits local shell environment, uses Unix utilities, version control, language tooling
- **MCP Integration**: Functions as both MCP server and client
- **Subagents** (2025): Parallel task delegation (e.g., backend API while building frontend)
- **Hooks**: Automatic triggers at specific points (tests after changes, lint before commits)
- **Checkpoints**: Save/rollback to previous states

**Claude Agent SDK:**
- The infrastructure powering Claude Code is now available as "Claude Agent SDK"
- Enables building custom agents with same capabilities

**Moss Observations:**
- Claude Code's design validates moss's "library is the API" approach
- Their hooks system is similar to moss's policy engine concept
- Checkpoints map to moss's Shadow Git approach
- MCP integration shows importance of protocol interoperability
- The SDK release confirms: agent infrastructure is becoming a platform play

### Cursor IDE
- **Site**: https://cursor.com
- **What it is**: VS Code fork with deep AI integration
- **Valuation**: ~$9.9B (mid-2025)

**Key Architecture Insights:**
- **Codebase Indexing**: Embedding model gives agent deep understanding and recall
- **@files and @folders**: Explicit referencing with proactive indexing
- **Agent Mode**: High-level goal → generates and edits files, runs code, iterates
- **Multi-Model**: Users choose between OpenAI, Anthropic, Gemini, xAI models
- **Bugbot** (2025): GitHub-integrated debugging assistant that watches for potential errors

**Context Evolution**: From ~4K tokens (early 2024) to 200K+ tokens (late 2024)

**Adoption**: >500M ARR, half of Fortune 500, every Coinbase engineer uses it

**Moss Observations:**
- Cursor's success proves the IDE integration path is viable
- Their codebase indexing is similar to moss's embedding/RAG goals
- @files/@folders referencing maps to moss's context management
- Bugbot shows value of continuous monitoring - moss could watch for issues during synthesis

## Competitive Analysis Summary

### What Competitors Do Better Than Moss Currently:

1. **SWE-agent**: Proven SWE-bench results, well-designed ACI interface
2. **Aider**: Mature edit format handling, architect/editor separation
3. **OpenHands**: Multi-agent coordination, sandbox runtime
4. **Claude Code**: Native Anthropic integration, checkpoint/rollback
5. **Cursor**: IDE integration, massive adoption, codebase indexing

### Moss's Unique Differentiators:

1. **Structural Awareness**: AST-based understanding vs text-based (skeleton, CFG, anchors)
2. **Verification Loops**: Type checking, tests, linting integrated into synthesis
3. **Shadow Git**: Atomic commits per tool call with easy rollback
4. **Plugin Architecture**: Everything is a plugin, not hardcoded
5. **Library-First**: Single API surface with generated interfaces (CLI, HTTP, MCP, TUI, LSP, gRPC)

### Patterns to Adopt:

- [ ] **Architect/Editor split** (Aider) - separate reasoning from editing
- [ ] **Event stream architecture** (OpenHands) - already in design, implement it
- [ ] **Guardrails/Linting at edit time** (SWE-agent) - integrate validation earlier
- [ ] **Checkpoint/rollback UX** (Claude Code) - expose Shadow Git more explicitly
- [ ] **Micro-agents** (OpenHands) - task-specialized agents using shared infrastructure
- [ ] **Codebase indexing** (Cursor) - enhance RAG capabilities

### Questions Answered:

1. **Is structural-awareness actually better?** Unknown - need SWE-bench evaluation
2. **What's moss's weakness?** Less mature, no benchmark results yet, not widely used
3. **Are they solving the same problem?** Yes and no:
   - Same: AI-assisted code modification
   - Different: Moss emphasizes synthesis (creating code from specs) over repair (fixing bugs)
   - Different: Moss's structural views vs their text-based approaches

## Benchmarking TODO

- [ ] Implement SWE-bench evaluation harness
- [ ] Compare moss's anchor-based patching vs search/replace vs diff
- [ ] Measure structural context (skeleton) value vs raw file context
- [ ] Test architect/editor pattern with moss infrastructure
