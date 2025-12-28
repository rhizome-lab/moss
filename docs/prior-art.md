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

### Goose (Block)
- **Repo**: https://github.com/block/goose
- **Docs**: https://block.github.io/goose/
- **What it is**: Open-source AI agent for automating engineering tasks (24.7k stars, Apache 2.0)

**Key Architecture Insights:**
- **Tech Stack**: Rust (59.6%) + TypeScript (32.9%), available as desktop app + CLI
- **MCP-First Design**: Extensions are MCP servers - the same protocol moss uses
- **Modular Crates**: `goose` (core), `goose-cli`, `goose-server`, `goose-mcp`, `goose-bench`
- **Multi-Model**: Tetrate Agent Router, OpenRouter, OpenAI, Anthropic, Gemini - any LLM
- **Local Execution**: Runs on-machine for privacy and control

**Extension System:**
- Built entirely on MCP - any MCP server can integrate
- **Built-in**: Developer (default), Computer Controller (web scraping, automations), Memory, Tutorial
- **Platform**: Chat Recall (search history), Extension Manager, Skills (load from `.goose/skills`), Todo
- **Security**: Automatic malware scanning before extension activation

**Permission System (Trust Model):**
Four distinct modes matching our Smart Trust Levels design:
1. **Completely Autonomous**: No approvals (default) - like our "Full Trust"
2. **Manual Approval**: Confirm every tool call - like our "Low Trust"
3. **Smart Approval**: Risk-based auto-approve - like our "Smart Approval" with risk classification
4. **Chat Only**: No tool execution - conversational only

Configurable mid-session via `/mode` command or settings.

**Interactive Processing Loop:**
1. Human submits request
2. Provider Chat sends request + available tools to LLM
3. Model Extension Call executes tool requests (JSON format)
4. Response to Model returns execution results
5. **Context Revision**: Removes outdated information to optimize tokens
6. Model Response delivers final answer

**Token Optimization:**
- Summarization with smaller models
- Algorithmic content deletion
- Efficient file operations (find-replace over rewrites)

**Error Handling:**
- Captures errors and sends back to model for resolution (doesn't halt)
- Similar to moss's validator loop concept

**Agent Internals** (`crates/goose/src/agents/`):
- `subagent_handler.rs` - Multi-agent delegation
- `router_tool_selector.rs` - Routes requests to appropriate tools
- `extension_malware_check.rs` - Security validation
- `large_response_handler.rs` - Manages oversized outputs
- `retry.rs` - Error recovery

**Context Revision Deep Dive** (`crates/goose/src/context_mgmt/`):

Goose's context revision is sophisticated and worth studying:

1. **Threshold-based auto-compaction**: Default 80% of context limit triggers compaction (`DEFAULT_COMPACTION_THRESHOLD = 0.8`). Configurable via `GOOSE_AUTO_COMPACT_THRESHOLD`.

2. **Dual visibility metadata**: Messages have `agent_visible` and `user_visible` flags:
   - After compaction, original messages become `user_visible=true, agent_visible=false`
   - Summary message becomes `agent_visible=true, user_visible=false`
   - This keeps full history for user while agent only sees summary

3. **LLM summarization with structured sections** (`summarize_oneshot.md`):
   - User Intent, Technical Concepts, Files + Code, Errors + Fixes
   - Problem Solving, User Messages, Pending Tasks, Current Work, Next Step
   - Analysis tags for chain-of-thought reasoning
   - Key: "This summary will only be read by you so it is ok to make it much longer than a normal summary"

4. **Progressive tool response removal**: When context still exceeds limits after summarization:
   - Try removing 0%, 10%, 20%, 50%, 100% of tool responses
   - Removes from the middle ("middle-out") to keep recent and oldest context
   - Graceful degradation if summarization itself hits context limits

5. **Continuation text injection**: Invisible assistant messages instruct the model:
   - "Do not mention that you read a summary or that conversation summarization occurred"
   - Different text for conversation continuation vs tool loop continuation

6. **Token counting**: tiktoken with caching (`o200k_base` tokenizer, 10K cache limit):
   - Hash-based cache for text strings
   - Special handling for tool definitions (FUNC_INIT, PROP_KEY, ENUM_ITEM constants)
   - Counts both message tokens and tool schema tokens

7. **Conversation validation/fixing**: Sophisticated repair pipeline:
   - Merge consecutive same-role messages
   - Remove orphaned tool requests/responses
   - Remove leading/trailing assistant messages
   - Shadow map pattern preserves non-visible messages during fixes

**Moss Observations:**
- **MCP alignment**: Goose validates MCP as the right protocol choice - they're all-in
- **Trust model similarity**: Their 4 permission modes map almost exactly to our Smart Trust Levels design
- **Context Revision**: Their token optimization is more sophisticated than moss's current approach
- **Extension security**: Malware scanning is interesting - moss could add similar checks for MCP servers
- **Skills directory**: `.goose/skills` pattern similar to Claude Code's - could adopt for moss
- **Rust + MCP**: Proves Rust is viable for agent infrastructure (we're Python, but could learn from their patterns)

**Context Revision Takeaways for Moss:**

Goose uses multi-turn conversation with accumulated context. Moss uses a different paradigm:
**composable loops with structured data handoffs** (`LoopContext`). Each LLM call is single-shot.

What applies to moss:
- [x] Tool responses ephemeral by design (each LLM call is fresh, no history)
- [x] Smart context selection (skeleton > full file) already core philosophy
- [ ] Structured summary sections for prompt building (User Intent, Technical, Pending Tasks)

What doesn't apply (different architecture):
- Auto-compaction: moss doesn't accumulate conversation, no need to compress
- Dual visibility: no persistent conversation to hide from agent
- Progressive tool removal: tool outputs don't persist between steps

Key insight: Goose's context revision is reactive (compress when full).
Moss's approach is proactive (include only what's needed, structured views by default).

**Key Differentiator vs Moss:**
- Goose is more "general agent" (terminal, web, files), moss is more "structural awareness"
- Goose relies on MCP for everything; moss has native AST/structural tools
- Goose has mature desktop app; moss is library-first
- Both: multi-model, local execution, MCP integration, verification loops

### Sourcegraph
- **Site**: https://sourcegraph.com
- **Repo**: https://github.com/sourcegraph/sourcegraph (Apache 2.0)
- **What it is**: Code intelligence platform - search, navigation, and understanding across massive codebases

**Historical Significance:**
Sourcegraph pioneered many concepts that coding agents now build on:
- Universal code search across all repos, branches, languages
- Semantic code navigation ("go to definition", "find references" at scale)
- Code graph understanding (not just text search)
- Batch changes for multi-repo refactoring

**Key Architecture Insights:**

**Repository Layer:**
- **gitserver**: Sharded service storing all connected repositories
- **worker**: Keeps repos synchronized with code hosts, respects rate limits
- Persistent cache (code host is source of truth, eventually consistent)

**Code Intelligence (Two Approaches):**
1. **Search-based** (default): Regex patterns, no setup, may have false positives
2. **Precise** (SCIP/LSIF): Language-specific indexes uploaded to Sourcegraph, accurate cross-repo navigation

**Search Infrastructure:**
- **zoekt**: Trigram indexes for fast full-codebase search on default branches
- **searcher**: Fallback for non-indexed code/branches
- **Syntect**: Syntax highlighting across all code views

**Code Graph:**
Not a dependency graph, but semantic understanding through:
- Repository syncing from code hosts
- Permission syncing for authorization
- Settings cascade (user → org → global)
- Navigation connecting definitions, references, docs

**Products (2025):**
- **Code Search**: Core search and navigation product
- **Cody**: AI coding assistant (Enterprise focus after July 2025)
- **Amp**: Agentic coding tool (see `docs/research/ampcode.md`)

**Recent Evolution:**
- Cody Free/Pro discontinued July 2025, focusing on Enterprise
- MCP Server available for Enterprise plans
- Code Review Agent, Migration Agent, Testing Agent in EAP
- Agent API for building custom agents on Sourcegraph infrastructure

**Moss Observations:**
- **Foundational influence**: Sourcegraph's code graph concept directly influenced moss's index design
- **SCIP/LSIF**: Moss uses tree-sitter instead (simpler, no build pipeline integration needed)
- **zoekt trigrams**: Similar to moss's SQLite FTS for path search
- **Precise vs search-based**: Moss is "search-based" level (AST parsing, not full type resolution)
- **Scale difference**: Sourcegraph handles millions of repos; moss focuses on single-codebase depth
- **Key learning**: Universal code intelligence is infrastructure, not a feature - agents need it

**What Sourcegraph Does Better:**
- Cross-repository navigation and search
- Enterprise scale (permissions, deployment options)
- Language-agnostic precise navigation via SCIP
- Mature batch changes for large refactors

**What Moss Does Differently:**
- Single-codebase focus with deeper structural views (skeleton, CFG)
- No build pipeline integration needed
- Library-first API design
- LLM-optimized output (token efficiency)

## Competitive Analysis Summary

### What Competitors Do Better Than Moss Currently:

1. **SWE-agent**: Proven SWE-bench results, well-designed ACI interface
2. **Aider**: Mature edit format handling, architect/editor separation
3. **OpenHands**: Multi-agent coordination, sandbox runtime
4. **Claude Code**: Native Anthropic integration, checkpoint/rollback
5. **Cursor**: IDE integration, massive adoption, codebase indexing
6. **Goose**: MCP-native architecture, mature desktop app, extension security (malware scanning)
7. **Sourcegraph**: Cross-repo search at scale, precise navigation via SCIP, batch changes

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

## Additional IDE/Tool Research (Dec 2025)

### Warp (AI-Native Terminal)
- **Site**: https://www.warp.dev
- **What it is**: Rust-based GPU-accelerated terminal with deep AI integration

**Key Architecture Insights:**
- **Agent Mode (Agents 3.0)**: Multi-step task execution with terminal capabilities. Agents run interactive commands, work inside CLI apps, use MCP and codebase embeddings.
- **Active AI**: Proactive suggestions based on terminal errors/output (e.g., "folder doesn't exist, create it?")
- **Dispatch Mode**: Fully autonomous mode (Ctrl+Shift+I) - AI operates without permission prompts
- **Multi-Model**: Claude 3.5 Sonnet (default), Claude 3.5 Haiku, GPT-4o. Enterprise can bring custom LLMs.
- **Rust + GPU**: Fast input/output, low memory vs Electron-based terminals

**Security**: TLS 1.3 in transit, AES 256 at rest. No data stored on Warp servers. No training on user data.

**Moss Observations:**
- Dispatch mode is interesting - moss could have a "trust level" that determines how much confirmation is needed
- Active AI (proactive suggestions) could inform moss's policy engine - suggest fixes before failures
- Terminal-level integration gives access to system events that IDE plugins can't see

### Zed (GPU-Accelerated Editor)
- **Site**: https://zed.dev
- **Repo**: https://github.com/zed-industries/zed (GPL v3, fully open source)
- **What it is**: High-performance collaborative code editor in Rust

**Key Architecture Insights:**
- **GPUI Framework**: Custom GPU-accelerated UI, ~200 workspace crates, layered architecture
- **Buffer Architecture**: "Multi-thread-friendly snapshot-able copy-on-write B-tree" vs Atom's "array of strings"
- **Agent Client Protocol (ACP)**: Open protocol for third-party AI agents - enables external agents to connect
- **Edit Prediction**: Zeta, their open-source model that anticipates next edits
- **Agent Panel**: Agentic editing that leverages installed LSPs, linters, tests

**Privacy**: All code and agent interactions remain local, no data to Zed servers.

**Model Flexibility**: Claude 3.7 Sonnet, bring-your-own keys, Ollama for local models.

**Moss Observations:**
- ACP is interesting - moss could implement an ACP adapter to work inside Zed
- Their B-tree buffer is similar to what moss's structural editor needs
- Edit Prediction is a form of synthesis - predicting code before it's written
- Background AI work (continues while you code) aligns with moss's async design

## Protocols & Standards

### Agent Client Protocol (ACP)
- **Site**: https://zed.dev/acp
- **Repo**: https://github.com/zed-industries/agent-client-protocol
- **Spec**: https://agentclientprotocol.com
- **What it is**: Open standard for editor ↔ coding agent communication

**Vision**: "Just as LSP unbundled language intelligence from monolithic IDEs, ACP enables switching between agents without switching editors."

**Technical Details:**
- Bidirectional JSON-RPC 2.0 over stdio (stdin/stdout)
- Reuses MCP data types where possible (text content, code diffs, tool results)
- Human-readable text defaults to Markdown
- Schema-based validation (see `schema/schema.json`)

**SDKs Available:**
- Rust: `agent-client-protocol` (crates.io)
- TypeScript: `@agentclientprotocol/sdk` (npm)
- Python: Official SDK with examples
- Kotlin: JVM support

**Current Agents:**
- Gemini CLI (reference implementation)
- Claude Code (via ACP)
- Codex
- Custom agents via `agent_servers` config

**Editor Support:**
- Zed (native)
- JetBrains (coming soon)
- Neovim, Emacs (community adapters)

**Config Example:**
```json
{
  "agent_servers": {
    "My Custom Agent": {
      "type": "custom",
      "command": "python",
      "args": ["-m", "moss.acp_server"],
      "env": {}
    }
  }
}
```

**Moss Implementation Plan:**
- [ ] Create `moss.acp_server` module
- [ ] Implement ACP JSON-RPC handlers
- [ ] Map moss tools to ACP capabilities (multi-file edit, codebase context)
- [ ] Test with Zed as client
- Priority: High - gives moss access to Zed's growing user base

### Agent2Agent Protocol (A2A)
- **Site**: https://a2a-protocol.org
- **Repo**: https://github.com/google/A2A (now under Linux Foundation)
- **Blog**: https://developers.googleblog.com/en/a2a-a-new-era-of-agent-interoperability/
- **What it is**: Open protocol for agent-to-agent communication (Google, April 2025)

**Technical Details:**
- **Transport**: JSON-RPC 2.0 over HTTP(S), SSE for streaming, push notifications for async
- **Agent Discovery**: "Agent Cards" (JSON) advertise capabilities and connection info
- **Task Lifecycle**: Tasks have lifecycle with outputs called "artifacts"
- **Message Format**: "Parts" with content types enabling negotiation between agents
- **Authentication**: Enterprise-grade auth, parity with OpenAPI auth schemes
- **SDK**: Python (`pip install a2a-sdk`), with samples at github.com/a2aproject/a2a-samples

**Key Concepts:**
- **Client agents**: Formulate and communicate tasks
- **Remote agents**: Act on those tasks
- **Long-running tasks**: Supports hours/days with human-in-the-loop
- **Capability negotiation**: Agents discover what each other can do

**A2A vs MCP:**
- MCP: Provides tools and context TO an agent (agent ↔ tools)
- A2A: Enables agents to collaborate WITH each other (agent ↔ agent)
- "If MCP is what enables agents to use tools, then A2A is their conversation while they work"

**Adoption:**
- 150+ organizations (Dec 2025), Linux Foundation governance
- Partners: Atlassian, Salesforce, SAP, ServiceNow, PayPal, MongoDB, LangChain, etc.
- Version 0.3: gRPC support, signed security cards, extended Python SDK

**Moss Evaluation:**
- **Fit with ticket-based model**: A2A's task-based communication aligns well with moss's ticket-based agent design
- **Complements MCP**: Moss already has MCP server; A2A would add agent-to-agent capabilities
- **Use cases**:
  - Moss as "remote agent" providing structural analysis to other agents
  - Moss delegating specialized tasks (e.g., security scanning) to external agents
  - Multi-agent workflows coordinated via A2A
- **Implementation approach**: A2A server exposing moss tools, A2A client for delegation
- **Priority**: Medium - valuable for ecosystem interop, but not blocking core functionality

### Agent Frameworks: Google ADK vs LangGraph

**Google ADK (Agent Development Kit):**
- **Site**: https://google.github.io/adk-docs/
- **Repo**: https://github.com/google/adk-python
- **What it is**: Open-source Python framework for multi-agent systems (Google Cloud NEXT 2025)

Key features:
- Model-agnostic (Gemini, Claude via LiteLLM, etc.)
- MCP integration for tools
- Hierarchical agent composition and delegation
- Built-in evaluation framework
- Optimized for Vertex AI/Google Cloud
- Can use other frameworks (LangGraph, CrewAI) as tools

**LangGraph:**
- **Site**: https://langchain-ai.github.io/langgraph/
- **Repo**: https://github.com/langchain-ai/langgraph
- **What it is**: Python framework for graph-based agent control flow (LangChain extension)

Key features:
- Finite state machine model (nodes = steps, edges = transitions)
- Fine-grained control over workflows
- Lower latency via graph-based context passing
- Better for complex, iterative agents
- LangChain ecosystem integration (LangSmith for observability)

**Comparison:**
| Aspect | Google ADK | LangGraph |
|--------|-----------|-----------|
| Philosophy | "Batteries-included", higher-level | Fine-grained control |
| Multi-agent | Built for hierarchical teams | Possible but more manual |
| Cloud | Google Cloud/Vertex AI optimized | Cloud-agnostic |
| Observability | OpenTelemetry-first | LangSmith/Langfuse |
| Control | Abstracted orchestration | Full state machine control |

**Moss Observations:**
- Both validate need for structured agent loops (like moss's AgentLoop)
- ADK's MCP integration aligns with moss's approach
- LangGraph's graph model is similar to moss's step-based loops
- Moss differentiates via structural awareness (skeleton, AST), not orchestration
- Could potentially export moss tools as ADK/LangGraph integrations

### Windsurf (Codeium's Agentic IDE)
- **Site**: https://windsurf.com (formerly https://codeium.com/windsurf)
- **What it is**: VS Code fork built around AI-first philosophy

**Key Architecture Insights:**
- **Cascade**: Agentic assistant with deep codebase understanding, multi-step planning, tool calls
- **Supercomplete**: Predicts *intent* not just code - e.g., renaming variable suggests all occurrences
- **Rules System**: Granular rules in `.windsurf/rules/` - always-on, @mentionable, glob-attached
- **Preview + Deploy**: Preview web apps in editor, deploy to Netlify via Cascade tool calls
- **VS Code Fork**: Familiar environment but unconstrained by extension limitations

**Models**: Windsurf's SWE models, Claude 4 Sonnet/Opus via own API key, MCP server connections.

**Security**: SOC 2 Type II, FedRAMP High, ZDR (Zero Data Retention) options, self-hosted deployments.

**Moss Observations:**
- Rules system is like moss's policy engine - could sync or interop
- Supercomplete (intent prediction) is what moss's DWIM aims for
- Their deep fork approach shows IDE integration limits - why moss prioritizes library-first
- Cascade's multi-step planning + tool calls is very similar to moss's planner → executor flow

### Google Antigravity
- **Site**: https://antigravityai.org
- **What it is**: Google's agentic IDE, announced Nov 2025 with Gemini 3

**Key Architecture Insights:**
- **Agent-First IDE**: Not code completion or chat - agents with direct editor/terminal/browser access
- **Two Views**: Editor view (IDE + agent sidebar) and Manager view (orchestrate multiple agents)
- **Multi-Agent Management**: Dispatch 5 agents on 5 bugs simultaneously
- **Browser UI Testing**: Agents can interact with browser for testing
- **Self-Validation**: Agents validate their own work

**Models**: Gemini 3 Pro/Deep Think/Flash, Claude Sonnet 4.5/Opus 4.5, GPT-OSS-120B.

**Origin**: Google acquired Windsurf team for $2.4B, so Antigravity builds on that foundation.

**Moss Observations:**
- Manager View for multi-agent is what moss's ticket-based agent model enables
- Self-validation aligns with moss's verification loops
- Browser access for UI testing is interesting - moss could add browser automation tools
- The Windsurf acquisition shows value of agentic IDE approach

### VS Code + GitHub Copilot
- **Docs**: https://code.visualstudio.com/docs/copilot/overview
- **What it is**: Microsoft's AI integration in VS Code via GitHub Copilot

**Key Architecture Insights:**
- **Agent Mode** (GA in VS Code 1.99+): Autonomous multi-step coding, monitors errors, auto-corrects in loop
- **Tool System**: LLM calls tools (search workspace, read files, run terminal, get errors, apply changes)
- **MCP Integration** (GA in 1.102+): Supports stdio and SSE transports, max 128 tools per request
- **Three Extension Points**: Built-in tools, extension-contributed tools, MCP servers
- **LSP → MCP**: VS Code team invented LSP in 2016, MCP was inspired by it, now MCP returns to VS Code

**Moss Observations:**
- 128 tool limit is interesting - moss should be aware of tool count constraints
- MCP standardization means moss's MCP server can integrate directly
- Their tool architecture (workspace search, file read, terminal, errors, apply) maps well to moss tools
- Agent mode's error-monitoring loop is exactly what moss's validator does

## Program Synthesis Systems (Detailed)

### Escher (Enumerative Synthesis)
- **Paper**: "Recursive Program Synthesis" (CAV 2013)
- **What it is**: Generic enumerative synthesizer for recursive programs from I/O examples

**Technical Approach:**
- Parameterized by components (instructions) - can be instantiated for different domains
- Special data structures for inferring conditionals and synthesizing recursive procedures
- Outperformed SAT-based synthesis tools on integers, lists, and trees
- Used within LoopInvGen, a high-performing SyGuS synthesizer

**Moss Implementation Notes:**
- `EnumerativeGenerator` should enumerate ASTs bottom-up
- Key insight: special handling for conditionals and recursion patterns
- Could use moss's skeleton to identify likely recursion patterns in codebase

### Myth (Type-and-Example-Directed)
- **Paper**: "Type-and-Example-Directed Program Synthesis" (PLDI 2015)
- **Repo**: https://github.com/silky/myth
- **What it is**: Synthesizes recursive functions over algebraic datatypes

**Technical Approach:**
- Combines type information AND I/O examples to prune search space
- Uses "refinement trees" - data structure representing constraints on code shape
- Proof-theoretic techniques from type theory
- Smyth (successor) adds sketching: "Smyth = Sketching + Myth"

**Moss Implementation Notes:**
- `EnumerativeGenerator` could use Python type hints as refinement constraints
- Combining types + examples is powerful - moss has both (tests = examples, type hints = types)
- Refinement trees could map to moss's AST representation

### SyPet (Component-Based Synthesis)
- **Paper**: "Component-Based Synthesis for Complex APIs" (POPL 2017)
- **Repo**: https://github.com/utopia-group/sypet
- **What it is**: Synthesizes Java programs by composing API calls

**Technical Approach:**
- **Petri Net Representation**: Places = types, transitions = methods, tokens = variable counts
- **Two-Phase**: (1) Sketch generation via Petri net reachability, (2) Sketch completion via SAT
- Outperformed InSynth and CodeHint on real-world tasks

**Moss Implementation Notes:**
- `ComponentGenerator` should build type graph from available functions
- Petri net approach is elegant for API composition
- Could use moss's `deps` and `external-deps` to know available components
- SAT for argument binding is tractable for small sketches

### Synquid (Refinement Type Synthesis)
- **Paper**: "Program Synthesis from Polymorphic Refinement Types" (PLDI 2016)
- **Repo**: https://github.com/nadia-polikarpova/synquid
- **Demo**: http://comcom.csail.mit.edu/demos/
- **What it is**: Synthesizes programs from refinement types using Z3

**Technical Approach:**
- **Liquid Types**: Refinement types with logical predicates (e.g., `{List a | len _v = n}`)
- **Bidirectional**: Top-down and bottom-up type propagation
- **Liquid Abduction**: Novel rule for branching terms
- Uses Z3 SMT solver for constraint solving
- Evaluated on 64 synthesis problems

**Moss Implementation Notes:**
- `SMTGenerator` should translate Python specs to Z3 constraints
- Refinement types are more expressive than plain types - could use docstrings/contracts
- Z3 integration via `pip install z3-solver`
- Key insight: modularity enables pruning - check components independently

### LLM-Guided Enumerative Synthesis (2024)
- **Paper**: "Guiding Enumerative Program Synthesis with Large Language Models" (2024)
- **What it is**: Hybrid approach combining LLMs with enumerative synthesis

**Technical Approach:**
- LLM proposes (possibly incorrect) solutions
- Build probabilistic CFG (pCFG) from LLM proposals
- Use pCFG to guide enumerative search in CEGIS loop
- 2-way information exchange: LLM → enumerator → LLM
- Achieves 80% benchmark completion (vs lower for either alone)

**Moss Implementation Notes:**
- `NeuralGuidedGenerator` should use this hybrid approach
- LLM provides probability distribution over likely programs
- Enumerator explores systematically using that distribution
- CEGIS loop with counterexamples improves both components

### FlashFill / PROSE (Programming by Example)
- **Project**: https://www.microsoft.com/en-us/research/group/prose/
- **Repo**: https://github.com/microsoft/prose
- **What it is**: Microsoft's framework for synthesizing programs from I/O examples

**Technical Approach:**
- User provides input-output examples
- System synthesizes programs in a domain-specific language (DSL)
- Deductive meta-algorithm parameterized by DSL
- Synthesizes scripts with complex business logic in <1 second
- Ranking/disambiguation among multiple valid programs

**Applications:**
- FlashFill in Excel 2013 (hundreds of millions of users)
- Text extraction, web extraction, data wrangling
- Visual Studio, Office, PowerQuery, PowerApps, SQL

**Key Insight**: Requires (a) DSL design, (b) synthesis algorithm, (c) ranking for disambiguation.

**Status**: As of Oct 2025, Microsoft stopped releasing new PROSE SDK versions.

**Moss Implementation Notes:**
- `PBEGenerator` should define a Python-subset DSL
- Key challenge: disambiguation when multiple programs fit examples
- Ranking could use: complexity (prefer simpler), coverage (prefer more general)
- Could integrate with moss's test suite as example source

### Sketch / Rosette (Solver-Aided Synthesis)
- **Rosette Site**: https://emina.github.io/rosette/
- **Rosette Repo**: https://github.com/emina/rosette
- **Sketch Site**: https://people.csail.mit.edu/asolar/sketch.html
- **What it is**: Languages where you write programs with "holes" that solvers fill

**Technical Approach:**
- **Sketches**: Programs with holes (e.g., `(bvadd x (?? int32?))` = all programs adding constant to x)
- **Hole types**: Constants (`??`), choices (`choose`), grammars (`define-grammar`)
- Compiler translates to SMT constraints, solver fills holes
- Works for synthesis, verification, debugging, repair

**Example:**
```racket
; Sketch: multiply x by unknown constant
(define (mul c x) (* c x))
; Solver finds c such that assertions pass
```

**Moss Implementation Notes:**
- `SketchGenerator` should support Python-style hole syntax
- Could use comments: `# HOLE: int` or type annotations: `x: Hole[int]`
- Translate to Z3 constraints (same as SMTGenerator)
- Useful for "fill in the blanks" style synthesis

### miniKanren (Relational Programming)
- **Wikipedia**: https://en.wikipedia.org/wiki/MiniKanren
- **Book**: "The Reasoned Schemer"
- **What it is**: Family of languages for relational (bidirectional) programming

**Key Capability: Running Programs Backwards**
- Relations are bidirectional: specify inputs → get outputs, OR specify outputs → get inputs
- An interpreter written as a relation can synthesize programs from I/O examples
- Can generate quines (programs that output themselves)
- Can differentiate AND integrate (run differentiation backwards)

**Example:**
```scheme
; evalo relates expressions to their values
(evalo q q)  ; finds quines - expressions q that evaluate to themselves
```

**Technical Approach:**
- Core fits on 2 printed pages
- Unification-based search
- Purely relational programs run forward, backward, or "strangely"

**Moss Implementation Notes:**
- `RelationalGenerator` could embed miniKanren in Python
- Libraries exist: `kanren` (Python), `microKanren` (minimal impl)
- Key use case: given output spec, find program that produces it
- Could write moss tools as relations for "inverse" queries

### DeepCoder (Neural-Guided Synthesis)
- **Paper**: "DeepCoder: Learning to Write Programs" (ICLR 2017)
- **Recent**: ExeDec (ICLR 2024) builds on DeepCoder
- **What it is**: Neural network predicts program properties to guide search

**Technical Approach:**
- Train neural net to predict which DSL functions appear in solution
- Use predictions to prioritize search (enumerative or SMT-based)
- Order of magnitude speedup over non-augmented baselines
- Solves programming competition-style problems from I/O examples

**2024 Developments (ExeDec):**
- Execution decomposition for compositional generalization
- Breaks synthesis into sub-problems based on intermediate execution
- Improves generalization to larger/more complex programs

**Related: DeepSynth**
- Open-source synthesizer using DeepCoder approach
- Repo: https://github.com/nathanael-fijalkow/DeepSynth
- Combines ML predictions with efficient enumeration

**Moss Implementation Notes:**
- `NeuralGuidedGenerator` could train small model on codebase patterns
- Predict likely imports, function names, patterns
- Use predictions to weight enumeration (not replace it)
- Could fine-tune on repo-specific style

### λ² (Lambda Squared) - Bidirectional Synthesis
- **Paper**: "Type-and-Example-Directed Program Synthesis" (PLDI 2015)
- **What it is**: Combines type-directed and example-directed synthesis bidirectionally

**Technical Approach:**
- Guarantees simplest program that fits examples
- Three techniques combined:
  1. **Inductive generalization**: I/O examples → hypotheses about program structure
  2. **Deduction**: Infer new I/O examples for subexpressions
  3. **Best-first enumeration**: Search for hypothesis that works
- Each hypothesis leads to subproblems for subexpressions

**Results**: Synthesized programs for lists, trees, nested structures. Notably synthesized a program believed to be the world's earliest functional pearl.

**Moss Implementation Notes:**
- `BidirectionalStrategy` should combine type hints + tests
- Generate hypotheses about function structure from signature
- Use test cases to constrain subexpression synthesis
- Best-first search with deduction for pruning

### PushGP (Genetic Programming)
- **Site**: http://faculty.hampshire.edu/lspector/push.html
- **Python**: https://github.com/erp12/pyshgp (`pip install pyshgp`)
- **Clojure**: https://github.com/lspector/Clojush
- **What it is**: Evolutionary search over programs in the Push language

**The Push Language:**
- Stack-based with separate stack per type
- Syntactically minimal: only rule is balanced parentheses
- Trivial to generate valid programs (important for evolution)
- Supports runtime code manipulation and novel control structures

**Key Capabilities:**
- One of most powerful "general program synthesis" frameworks
- Handles multiple data types, control structures naturally
- **Autoconstructive evolution**: Programs evolve their own evolutionary mechanisms
- Applications: intelligent agents, quantum computing, etc.

**Tradeoffs:**
- Very high runtime (evolutionary search is expensive)
- Can solve problems other PBE systems cannot
- Good for exploration, less good for quick synthesis

**Moss Implementation Notes:**
- `GeneticGenerator` could use pyshgp as backend
- Best for problems where other methods fail
- Could use as "last resort" synthesizer
- Runtime concerns limit practical use

### DreamCoder (Abstraction Learning)
- **Paper**: "DreamCoder: Growing Generalizable, Interpretable Knowledge" (PLDI 2021)
- **ArXiv**: https://arxiv.org/abs/2006.08381
- **What it is**: Learns domain-specific languages through wake-sleep cycles

**Wake-Sleep Architecture:**
1. **Wake**: Synthesize programs for tasks using neural guidance
2. **Abstraction Sleep**: Extract common patterns into library (declarative knowledge)
3. **Dreaming Sleep**: Train neural net on replays + fantasies (procedural knowledge)

**Key Innovation: Library Learning**
- Automatic refactoring extracts reusable components
- E-graph matching identifies rewrites exposing patterns
- Library grows with experience, making future synthesis faster

**Results:**
- Rediscovers modern functional programming concepts
- Rediscovers vector algebra, classical physics (Newton's, Coulomb's laws)
- Solves creative tasks (drawing, scene building)
- Mean solve time: 54.1s, median: 15.0s

**Related: Stitch**
- 3-4 orders of magnitude faster than DreamCoder's library learning
- 2 orders of magnitude less memory
- Comparable library quality

**Moss Implementation Notes:**
- Core idea: moss should learn abstractions from synthesized code
- After each synthesis, check if pattern should join library
- Could use Stitch for efficient library extraction
- Long-term: moss learns project-specific idioms

## Code Generation Benchmarks

### Beyond HumanEval/MBPP/SWE-bench

The field is shifting from "Can the model code?" to "Can the model engineer?"

**Benchmark Evolution:**
| Benchmark | Focus | Tasks |
|-----------|-------|-------|
| HumanEval | Function synthesis | 164 problems |
| MBPP | Simple functions | 974 problems |
| EvalPlus (HumanEval+/MBPP+) | 80x/35x more tests | Reduces overfitting |
| HumanEval Pro/MBPP Pro | Self-invoking code | Progressive reasoning |
| MultiPL-E | 18 languages | Paradigm coverage |
| SWE-bench | Real GitHub issues | ~2000 problems |
| LiveCodeBench | Production code changes | Ongoing |
| RepoBench | Multi-file completion | Repository-level |
| BigCodeBench | Complex tasks | 76 tasks unsolved by all models |
| BFCL-v3 | Function/tool calling | Agent capabilities |
| DS-1000 | Data science (NumPy, Pandas) | 1000 problems |

**2025 SOTA Performance:**
- HumanEval: Claude 3.5 Sonnet 92%, GPT-4o 90.2%
- SWE-bench Verified: GPT-5 74.9%, Claude 3.7 Sonnet 70.3%
- Aider Polyglot: GPT-5 88%
- LiveCodeBench v5: Gemini 2.5 Pro 70.4%

**Key Insight**: Real-world engineering benchmarks (SWE-bench, RepoBench) matter more than toy problems. Models that ace HumanEval may fail on actual codebases.

**Moss Evaluation Strategy:**
- [ ] Start with SWE-bench Lite (manageable size)
- [ ] Add RepoBench for multi-file context evaluation
- [ ] Use EvalPlus to avoid false positives
- [ ] Track LiveCodeBench for ongoing comparison

## SWE-bench Evaluation

### Overview
- **Site**: https://www.swebench.com
- **Repo**: https://github.com/SWE-bench/SWE-bench
- **What it is**: Benchmark for LLMs resolving real GitHub issues

**Methodology:**
- Task: Given codebase + issue, generate patch that resolves it
- Evaluation: Apply patch, run repo's tests
- Environment: Docker containers for reproducibility
- Subsets: Full (~2000), Lite (~300), Verified (500 human-validated)

**Setup Requirements:**
- x86_64 machine, 120GB storage, 16GB RAM, 8 CPU cores
- Docker required (or Modal for cloud evaluation)
- ARM (M-series Mac): Use `--namespace ''` to build images locally

**Current SOTA (Dec 2025):**
- SWE-bench Verified: Claude 4 Opus at 73.20%
- SWE-bench Lite: Claude 4 Sonnet + ExpeRepair at 60.3%
- SWE-bench Pro: GPT-5 at 23.1%, Claude Opus 4.1 at 22.7%
- Pass@5 leader: Claude Sonnet 4.5 at 55.1%
- Budget options: Grok Code Fast 1, gpt-oss-120b ~30% at $0.03-0.04/problem

**Key Insights:**
- Frontier models dramatically outperform older models (GPT-4o at 4.9%)
- Agent architecture matters as much as model capability
- Multiple attempts (pass@k) significantly improves scores

### Moss Evaluation Plan
- [ ] Install SWE-bench harness: `pip install swebench`
- [ ] Start with Lite subset (smaller, faster iteration)
- [ ] Compare: moss patches vs raw LLM patches
- [ ] Measure: Does skeleton context improve patch accuracy?
- [ ] Measure: Does anchor-based patching reduce failed applies?

## Code Patching Approaches

### The Problem
Applying AI-generated code changes is "surprisingly difficult." LLMs generate valid code but fail to integrate it. Formats like unified diff are "too algorithmically complex for LLMs."

### Approaches Compared

| Approach | Pros | Cons |
|----------|------|------|
| **Whole File Rewrite** | Simple, no matching needed | Expensive (tokens), loses unrelated changes |
| **Search/Replace Blocks** | Intuitive, works without line numbers | Fails if search text not unique |
| **Unified Diff** | Standard format, efficient | Brittle, fails if file changed |
| **Fuzzy/Anchor-Based** | Robust to drift, confidence scoring | More complex implementation |
| **Semantic Edit** | 98% vs 70% success (claimed) | Requires deeper understanding |

### Key Insights from Research
- **Avoid line numbers**: LLMs struggle with exact line numbers
- **Clear delimiters**: Original vs replacement must be obvious
- **Fuzzy matching**: Cascade of methods (exact → anchor → similarity → Levenshtein)
- **Confidence scores**: Only apply if confidence > threshold (e.g., 0.95)
- **Error feedback**: When patches fail, explain why so LLM can retry

### Moss's Anchor-Based Approach
Moss uses structural anchors (AST nodes) rather than line numbers:
- Anchors identify code by structure, not position
- Robust to reformatting, comment changes, nearby edits
- Maps to actual semantic units (functions, classes, blocks)

**Comparison TODO:**
- [ ] Benchmark anchor-based vs search/replace on same tasks
- [ ] Measure retry rate (how often does first attempt fail?)
- [ ] Measure drift resistance (apply patch after other edits)

## Context Management for Coding Agents

### The Challenge
- **Context rot**: LLM recall degrades as context grows (finite "attention budget")
- **Lost in the middle**: Models recall beginning/end better than middle
- **Cost**: Token usage directly impacts API costs

### Four Core Techniques (2025)

| Technique | Description | Use Case |
|-----------|-------------|----------|
| **Offloading** | Summarize tool responses, store full data in references | Large outputs |
| **Reduction** | Compact conversations to reduce token count | Long sessions |
| **Retrieval (RAG)** | Dynamically fetch relevant info at runtime | Large codebases |
| **Isolation** | Sub-agents handle specific tasks without context overlap | Parallel work |

### Approaches Compared

**Observation Masking** (OpenHands, Cursor, Warp):
- Selectively hide/mask parts of context
- Keep critical info visible
- Fast, deterministic

**LLM Summarization** (Claude Code's auto-compact):
- Summarize full trajectory at 95% context usage
- Preserves semantic meaning
- Slower, uses tokens for summarization itself

### Best Practices (Token Budget Management)
- **70% soft cap**: Prefer summarization, warn user
- **85-90% hard cap**: Force summarize or drop least-valuable chunks
- **Absolute cap**: Refuse/clarify before exceeding provider limits

### Results
- Advanced memory systems: 80-90% token reduction
- 26% quality improvement with 90%+ token reduction (via intelligent memory)

### Moss Implementation Notes
- Already have: `context_memory.py` with summarization
- Needed: Token budget tracking, auto-compact trigger
- Consider: Hybrid approach (masking + summarization)
- Priority: Critical for long sessions (see `docs/log-analysis.md`)

## Tool Encoding & Schema Efficiency

### The Problem
MCP tool definitions use JSON Schema, which is verbose:
- 85 tools → ~8K tokens of passive context every turn
- 72% of overhead is in schemas (type definitions, descriptions per param)
- Complex tools like `search_query` (10 params) cost ~1K chars each

### Cloudflare Code Mode (Dec 2024)
- **Site**: https://blog.cloudflare.com/code-mode/
- **Approach**: Sidestep tool-calling entirely
- Convert MCP tools to TypeScript APIs with doc comments
- LLM writes code that calls APIs directly in sandbox
- Avoids: tool-call tokens, intermediate result round-trips
- Key quote: "The output of each tool call must feed into the LLM's neural network, just to be copied over to the inputs of the next call, wasting time, energy, and tokens"

**Moss implications:**
- For moss loop (where we control both sides), could use terse function signatures
- `grep(pattern, path?, glob?, limit=100)` = ~70 chars vs ~900 chars JSON Schema
- Potential 10x reduction in tool definition overhead

### CASS Memory System
- **Repo**: https://github.com/Dicklesworthstone/cass_memory_system
- Context-Aware Semantic Splitter for long-term memory
- Research for session/memory management approaches

### Beads (Steve Yegge)
- **Repo**: https://github.com/steveyegge/beads
- Chunking/context approach for managing LLM context windows
- Research for context window optimization

### Moss Implementation Notes
- [ ] Design compact tool encoding for moss agent (bypass JSON Schema)
- [ ] Investigate code-mode approach (LLM writes Python, not tool calls)
- [ ] Benchmark token savings vs MCP overhead

## Program Repair vs Program Synthesis

### Key Differences

| Aspect | Program Synthesis | Automatic Program Repair |
|--------|------------------|-------------------------|
| **Goal** | Create new programs from specs | Fix existing buggy programs |
| **Input** | Formal specification/examples | Buggy program + test suite |
| **Starting Point** | Builds from scratch | Modifies existing code |
| **Search Space** | All possible programs | Mutations of existing code |

### How They Connect
Semantics-based APR can frame repair as synthesis:
- **SemFix**: Component-based synthesis for repair
- **Angelix**: Extract constraints via symbolic execution, synthesize fixes
- **S3**: Syntax-guided synthesis for repair

### APR Categories
1. **Template-based**: Pattern matching on AST (GenProg, ARJA)
2. **Machine Learning**: Learn fix patterns from history
3. **Deep Learning**: End-to-end neural repair (current SOTA)
4. **Semantics-based**: Symbolic execution + synthesis

### Moss Positioning
Moss spans both:
- **Synthesis**: Generate code from specs (type hints, tests, natural language)
- **Repair**: Fix validation failures in synthesis loop
- Key insight: Repair is often easier than synthesis (smaller search space)

## Multi-Modal Code Generation

### Screenshot-to-Code (2025)

**Challenge**: Converting UI designs/screenshots to functional code is hard for MLLMs.
- Complex UIs overwhelm single-model approaches
- Need to unify: visual perception, layout planning, code synthesis

**ScreenCoder** (SOTA 2025):
- Modular multi-agent framework
- Three stages:
  1. **Grounding Agent**: VLM detects UI components with bounding boxes
  2. **Planning Agent**: Determine layout structure
  3. **Generation Agent**: Produce HTML/CSS code
- Outperforms end-to-end approaches

**DCGen** (Divide-and-Conquer):
- Identifies common MLLM failures in design-to-code
- Breaks task into subtasks
- Tested on GPT-4o, Gemini, Claude

**Google ScreenAI**:
- Visual language model for UI understanding
- Tasks: Q&A about screenshots, navigation, summarization
- Links Vision Encoder → Connector → LLM

### Moss Implementation Notes
- Could add `moss ui-to-code <screenshot>` command
- Multi-agent approach aligns with moss's architecture
- Use existing skeleton view to validate generated structure
- Consider: Figma/Sketch plugin that calls moss

## Formal Verification of Synthesized Code

### The Vision
Verify that LLM-generated code matches user intent, not just "passes tests."

### 2025 Research Highlights

**Astrogator** (arXiv July 2025):
- Formal specification of user intent for Ansible programs
- Custom query language + symbolic interpreter
- Results: 83% correct code verified, 92% incorrect code identified

**PREFACE Framework** (GLSVLSI 2025):
- Model-agnostic RL agent + LLM for Dafny code generation
- Dafny → SMT → correctness-by-construction guarantees
- No fine-tuning required

**Proof2Silicon** (SRC TECHCON 2025):
- Natural language → verified Dafny → HLS → RTL hardware
- RL agent optimizes prompts for verification
- 72% end-to-end hardware synthesis success

**Vericoding Benchmark** (Sept 2025):
- 12,504 tasks across Dafny, Lean, Verus
- Success rates: Dafny 82%, Lean 27%
- Rapid progress: 68% → 96% in one year

**LLMs for System Verification** (HotOS 2025):
- FSCQ file system as benchmark
- 38% proof coverage overall, 57% for simpler theorems
- Best-first tree search helps significantly

### Moss Implementation Notes
- Consider Dafny integration for verified synthesis
- Could generate specs from type hints + docstrings
- Verification as alternative to testing for critical code
- Long-term: `moss synth --verify` flag

## Interactive Program Synthesis

### User-in-the-Loop Paradigm
Interactive synthesis treats the user as an oracle, refining programs through feedback.

### Key Approaches

**Three Dimensions of Interactivity:**
1. **Incremental algorithm**: Build program piece by piece
2. **Step-based formulation**: Small specifications at a time
3. **Feedback-based refinement**: User corrects/guides synthesis

**LooPy** (OOPSLA 2021):
- Small-Step Live PBE inside loops
- User steps through incomplete code as oracle
- IDE-integrated synthesis

**Self-Refine** (NeurIPS 2023):
- LLM generates → critiques itself → refines iteratively
- No training/RL required, just prompting
- Works for code, summarization, many tasks

**Decision Flow Visualization:**
- Show synthesized logic as finite state machine
- Users annotate/correct visually
- Effective for complex collaborative behaviors

### Moss Implementation Notes
- TUI could show synthesis progress interactively
- User could approve/reject intermediate steps
- Self-Refine pattern: generate → validate → refine loop
- Already have: validator loop, could add user approval points

## Security in AI-Generated Code

### The Problem is Severe

**Vulnerability Rates (2025 Research):**
- **45%** of AI-generated code introduces security vulnerabilities (Veracode)
- **62%** contains design flaws or known vulnerabilities
- **Java worst**: 70%+ failure rate
- **37.6% increase** in critical vulns after 5 iterations of "improvement"

### Common Issues
- CWE Top 25 vulnerabilities (input validation, injection, etc.)
- Omits security unless explicitly prompted
- Optimizes for "passes tests" not "secure"
- Larger models don't perform significantly better (systemic issue)

### "Vibe Coding" Risk
Developers rely on AI without specifying security constraints. LLMs aren't incentivized
to reason securely—they minimize path to passing result.

### Vulnerabilities in AI Tools Themselves
- **CVE-2025-55284** (Claude Code): DNS exfiltration of developer data
- **CVE-2025-54135** (Cursor): Arbitrary command execution

### Model-Specific Issues
- DeepSeek-R1: 50% more vulns when prompted with politically sensitive topics

### Implications for Moss
**This is critical for moss's design:**
- [ ] **Security validation by default**: Run security linters (bandit, semgrep) in validator loop
- [ ] **Explicit security prompting**: Include security requirements in synthesis specs
- [ ] **Iteration monitoring**: Track vulnerability count across refinement iterations
- [ ] **OWASP Top 10 checks**: Built-in detection for common vulns
- [ ] **Secure defaults**: Err on side of safer code patterns
- [ ] **User awareness**: Warn when generating security-sensitive code (auth, crypto, input handling)

**Key insight**: Moss's structural awareness could help—AST analysis can detect
vulnerable patterns that text-based tools miss.

## Prompt Engineering for Code Generation

### Core Best Practices (2025)

| Practice | Description |
|----------|-------------|
| **Role Definition** | Frame LLM as software engineering agent with clear responsibilities |
| **Structured Tool Use** | Provide examples of expected tool calls and outputs |
| **Context Depth** | Quality correlates with accuracy—provide relevant context |
| **Few-Shot Examples** | Show expected input/output pairs, especially for structured output |
| **Self-Review** | Request model cross-check its own generated code |
| **Format Specification** | Define exact output format to reduce hallucinations |
| **Testing Instructions** | Explicitly instruct to write tests and validate patches |

### Advanced Techniques

**Chain-of-Thought (CoT):**
- Newer models (o1-preview, o1-mini) use inference-time reasoning tokens
- Prompting style differs significantly from non-reasoning models

**Self-Review Prompting:**
- Request systematic evaluation of generated code
- "Review this code for bugs, edge cases, and security issues"

**Iteration & Evals:**
- Build evals that measure prompt performance
- Monitor as you iterate and upgrade models

### Moss Implementation Notes
- Prompt templates in `moss.prompts` module
- Few-shot examples from codebase (use existing similar code)
- Self-review in validator loop (LLM reviews its own output)
- Consider: `moss prompt --template <name>` for standardized prompts

## Automated Test Generation

### The State of the Art (2025)

**Meta's ACH (Automated Compliance Hardening):**
- Mutation-guided, LLM-based test generation
- Combines mutant generation + test generation
- First deployment at large-scale industrial systems

**TestGen-LLM Results:**
- 75% of test cases built correctly
- 57% passed reliably
- 25% increased coverage
- 73% recommendations accepted for production at Meta

**TestLoter Framework:**
- 83.6% line coverage, 78% branch coverage
- +8.5% line coverage vs ChatUniTest
- +10% line coverage vs EvoSuite
- Logic-driven framework with error repair

**RUG (Rust Unit Generation):**
- Type-aware caching: 51.3% token reduction
- +10.4% coverage improvement

### Key Techniques

| Technique | Benefit |
|-----------|---------|
| **Chain-of-Thought** | Explicit reasoning about coverage objectives |
| **RAG** | Higher quality tests with more context |
| **Mutation Testing** | Generate tests that catch real bugs |
| **Context-Aware Prompting** | LLM tests match or exceed human-written |

### Model Performance Varies by Language
- Gemini better for Java
- All models better for Python
- Less-benchmarked languages (Go, Kotlin) worse

### Moss Implementation Notes
- Already have: `moss coverage` for pytest-cov stats
- Needed: `moss gen-tests <file>` command
- Use existing tests as few-shot examples
- Target: Improve coverage for uncovered functions
- Consider: Mutation testing integration (mutmut)

## Code Explanation & Documentation Generation

### Tools Landscape

| Tool | Features |
|------|----------|
| **doc-comments-ai** | Treesitter + LLM, local models (Ollama) |
| **Autodoc** | Depth-first traversal, folder-level docs |
| **RepoAgent** | Repository-level docs, auto-maintenance |
| **lmdocs** | Context-aware, references imported libraries |
| **llmdocgen** | Multi-language support |

### Approaches

**Static Generation:**
- Generate docstrings during development
- Iterate with LLM, commit as permanent docs
- Best for overview docs, API documentation

**Dynamic Generation:**
- Generate explanations on-the-fly for readers
- No permanent storage, always up-to-date
- Best for function/line-level comments

### Technical Patterns
- **AST Analysis**: Parse code structure, identify undocumented functions
- **Dependency Tracking**: Map imports to provide context
- **Fine-Tuning**: CodeLlama + LoRA for domain-specific docs
- **Cost Control**: Use open-source models (Llama, Gemma) for free generation

### Moss Implementation Notes
- Already have: AST parsing, skeleton view
- Could add: `moss explain <symbol>` - explain any code
- Could add: `moss document <file>` - generate missing docstrings
- Use skeleton as context for explanations
- Consider: Dynamic docs in TUI/LSP (hover for explanation)

## Multi-Agent Coordination Patterns

### Architectures

| Pattern | Description | Use Case |
|---------|-------------|----------|
| **Peer-to-Peer** | Decentralized, any agent talks to any | Maximum flexibility, complex coordination |
| **Centralized** | Supervisor directs all agents | Clear control, simpler debugging |
| **Hierarchical** | Nested supervisors | Large systems, domain separation |
| **Fully-Connected** | Every agent to every agent | Small systems, emergent behavior |

### Key Frameworks

**CAMEL**: Role-playing framework with task-specific + cooperating agents.
**AutoGen**: Flexible behaviors, conversation-based cooperation, subtask decomposition.

### Best Practices (from 94+ studies)
- **Functional correctness**: Rigorous specification adherence
- **Role-based decomposition**: Clear agent responsibilities
- **Continuous validation**: Verify outputs at each step
- **Modularity**: Formalized interfaces, hierarchical/adapter patterns
- **Orchestration logic**: State transitions, message routing, coordination

### Challenges
- **Communication breakdowns**: 13.48% failures from output verification
- **Goal misalignment**: Inconsistent understanding between agents
- **Memory management**: Context sharing and isolation
- **Theory of Mind**: LLMs struggle with partner beliefs/intentions

### Emerging: Evolving Orchestration
- "Puppeteer-style" centralized orchestrator
- Trained via RL to adaptively sequence agents
- Dynamic response to evolving task states

### Moss Implementation Notes
- Ticket-based model already isolates agents
- Add: Dynamic orchestrator that assigns tickets
- Consider: RL-based ticket prioritization
- Monitor: Inter-agent communication patterns

## Local LLMs for Code

### The 2025 Landscape
Running powerful coding AI locally is now practical, not aspirational.

### Key Tools

| Tool | Description |
|------|-------------|
| **Ollama** | One-line commands for popular models, handles model management |
| **llama.cpp** | C/C++ inference, extremely fast, cross-platform |
| **GGUF/GPTQ** | Quantization formats for running on less powerful hardware |

### Top Local Coding Models (2025)

| Model | VRAM | Notes |
|-------|------|-------|
| Code Llama 70B | 40GB+ (full), 12-24GB (quant) | Strong general coding |
| DeepSeek-Coder | Variable | 300+ languages, SOTA benchmarks |
| Qwen 2.5 Coder | 12-24GB | Agentic task handling |
| StarCoder2 | 12-24GB | Multi-language |
| Phi-3 Mini | 4-8GB | Entry-level GPUs, laptops |

### Hardware Requirements
- **High-end** (70B models): 40GB+ VRAM or ~12-24GB with quantization
- **Mid-tier** (14-20B): 12-24GB VRAM
- **Lightweight** (3-7B): 4-8GB, can run on laptops

### DeepSeek-Coder
- 2 trillion tokens training (code + natural language)
- 300+ programming languages
- State-of-the-art on coding benchmarks
- Install: `ollama pull deepseek-coder:33b`

### Moss Implementation Notes
- Support local models via Ollama integration
- Allow model selection in config
- Fallback chain: local → API
- Consider: Quantized models for fast iteration, API for final synthesis

## Fill-in-the-Middle (FIM) Code Completion

### The Paradigm
FIM generates code between a prefix and suffix, conditioning on both contexts.
Unlike left-to-right completion, must reconcile preceding AND succeeding code.

### Training Approach (OpenAI)
Split document into: prefix, middle, suffix (before tokenization)
Formats:
- **PSM** (Prefix-Suffix-Middle): Most common
- **SPM** (Suffix-Prefix-Middle): Alternative ordering
- 50% PSM/SPM split provides best results

### Models with FIM Support
- StarCoder, DeepSeek-Coder, Code Llama (modern)
- Codex, CodeGen (early, L2R only)

### Recent Advances

**AST-FIM (Structure-Aware):**
- Mask complete syntactic structures, not random spans
- Aligned with code editing patterns (blocks, expressions, functions)
- Better than treating code as plain text

**Horizon-Length Prediction (HLP):**
- Teaches planning over arbitrary horizons
- 24% improvement in FIM benchmarks
- Negligible training overhead, zero inference overhead

**Instruction-Aware FIM (IFIM):**
- Standard instruction-tuning degrades FIM performance
- IFIM preserves both instruction-following AND infilling

### Challenges
- OOV tokens, project-specific APIs
- Cross-language adaptation
- Accuracy vs latency trade-off (especially real-time IDE)

### Moss Implementation Notes
- Anchor-based patching is similar to FIM (prefix + suffix)
- Could use FIM models for hole-filling synthesis
- AST-FIM aligns with moss's structural awareness
- Consider: FIM for `SketchGenerator` (fill holes in templates)

## Query-Based Code Analysis

### CodeQL (GitHub)
- **Site**: https://codeql.github.com
- **Repo**: https://github.com/github/codeql
- **What it is**: Semantic code analysis engine treating code as queryable data

**Technical Approach:**
- Code → Database (relational representation of AST, control flow, data flow)
- Custom query language (QL) for pattern matching
- Declarative rules for vulnerability detection
- Deep data flow and taint tracking

**Key Concepts:**
- **Code as data**: Extract relational database from source code
- **QL language**: Logic-based queries over code structure
- **Data flow analysis**: Track values through program execution
- **Security focus**: Primary use case is finding vulnerabilities

**Use Cases:**
- Security vulnerability detection (OWASP Top 10)
- Code quality checks
- API misuse detection
- Migration analysis

**Moss Observations:**
- CodeQL's "code as data" is similar to moss's SQLite index approach
- QL queries could inform moss's structural queries
- Consider: Export moss index in CodeQL-compatible format
- Data flow analysis is more advanced than moss's current callers/callees

**Research Value:**
- How to represent code relationally
- Query language design for code patterns
- Scaling analysis to large codebases

## Code Search & Retrieval

### The Challenge
Simple semantic search (embed files, find similar) often fails on codebases.
Even queries like "Session management code" yield poor results.

### Why Codebases Are Hard (Greptile)
- Code is structured, not prose
- Meaning depends on context (imports, types, call sites)
- Noise negatively impacts retrieval significantly

### What Works Better

**Translate to Natural Language First:**
- Generate natural language descriptions before embedding
- Embed the descriptions, not raw code

**Tighter Chunking:**
- Per-function, not per-file
- Use AST-aware splitters (respect class/function boundaries)

**Agent-Based Search (RepoRift):**
- RAG-powered agents enhance queries with repo context
- 78.2% Success@10 on CodeSearchNet

**Cursor's Approach:**
- Train embedding model on agent session traces
- Agent searches → opens files → finds code
- Use these traces to rank what should have been retrieved
- 12.5% higher accuracy (6.5-23.5% depending on model)

### Code Embedding Models (2025)

| Model | Score | Notes |
|-------|-------|-------|
| Qodo-Embed-1-7B | 71.5 (CoIR) | State-of-the-art |
| Qodo-Embed-1 (1.5B) | 68.53 | Beats larger 7B models |
| CodeRankEmbed | - | Trained on Stack V2 |
| Nomic Embed Code | - | Excels at retrieval |
| CodeSage Large V2 | - | Various code understanding |

### Hybrid Search Pipeline
1. **First stage**: Bulk retrieval (embeddings)
2. **Second stage**: Reranking (slower, better model)
3. **Enhancements**: HyDE, hybrid vector-search

### Moss Implementation Notes
- Already have: grep-based search, AST parsing
- Needed: Semantic search with code embeddings
- Use skeleton as natural language descriptions
- Consider: Agent-based search refinement
- Key: Per-function chunking, not per-file

## LLM-Based Fault Localization

### The Problem
Developers spend ~66% of debugging time on fault isolation.
Finding the buggy line(s) is often harder than fixing them.

### 2025 State of the Art

| Tool | Approach | Results |
|------|----------|---------|
| **MemFL** | External memory + project context | +12.7% bugs (27.6% on complex), $0.0033/bug |
| **AgentFL** | Multi-agent: comprehend → navigate → confirm | 157/395 Top-1, $0.074/bug, 97s/bug |
| **FaR-Loc** | Analyze failing tests + functionality description | Method-level FL |
| **DEVLoRe** | End-to-end: FL + repair | 274 bugs fixed (60.2% more than GiantRepair) |
| **AutoCrashFL** | Industrial-scale crash localization | Stack trace analysis |

### Key Techniques

**MemFL's Memory Architecture:**
- Static summaries of project
- Dynamic debugging insights from previous attempts
- Iterative refinement

**AgentFL's Three-Step Process:**
1. **Comprehension**: Understand the bug report
2. **Navigation**: Explore codebase to find relevant code
3. **Confirmation**: Verify the suspicious location

### Moss Implementation Notes
- Could add `moss localize <failing_test>` command
- Use skeleton + deps for project context
- Iterative: narrow down with each attempt
- Integrate with validator loop (when tests fail, localize first)

## Automated Code Refactoring

### The Landscape

**MANTRA** (March 2025 - SOTA):
- Multi-agent + contextual RAG
- 582/703 compilable, test-passing refactorings
- 50% improvement over EM-Assist
- User study: similar to developer-written code

**ECO** (Google, March 2025):
- Mine historical commits for anti-patterns
- Find similar patterns in billions of LOC
- Fine-tuned LLM applies similar edits
- Auto-verify and submit for review

### Challenges (ICSE 2025)
- LLMs lack contextual understanding
- May conflict with project conventions
- **37% correct** without fact-checking
- **98% correct** with fact-checking

### Key Techniques

**RefactoringMirror** (Detect-and-Reapply):
- LLM identifies refactoring to apply
- Reapply using tested refactoring engines (not LLM)
- 94.3% accuracy, avoids all buggy solutions

**Few-Shot Learning:**
- Retrieve similar refactoring patterns from project history
- Use as contextual cues for LLM

### Traditional vs AI
Traditional tools (parsing, symbol resolution) are more reliable for:
- Enforcing coding style
- Guaranteed behavior preservation
- Complex architectural changes

AI is easier to set up but "you can never be sure if it gets everything."

### Moss Implementation Notes
- `moss refactor` command with specific patterns:
  - Extract method, rename, move
  - Use RefactoringMirror pattern (LLM identifies, tool applies)
- Mine project history for anti-patterns
- Validate: tests pass before/after
- Consider: Use rope/libcst for safe refactoring, not raw LLM edits

## Code Translation & Migration

### The Scale of the Problem
- By 2025, 40% of IT budgets dedicated to technical debt from legacy systems (Gartner)
- Commonwealth Bank: 5 years, $750M to migrate COBOL → Java
- 63% of businesses trialing generative AI for code migration (2024)

### Current LLM Performance
- **Best case**: 47.3% unit test pass rate (C/C++/Go/Java/Python translation)
- **Worst case**: 2.1% pass rate
- **Rust translation**: Claude 3-Opus 47% success, drops significantly for >100 lines
- **No approach guarantees correctness** - at best, unit tests verify equivalence

### Multi-Agent Migration Framework (7Rs of Modernization)
1. **Analysis Agent**: Interprets and maps legacy code
2. **Coder Agent**: Generates modern equivalents
3. **Review Agent**: Validates output

### Verification Approaches

**LLMLift** (Neuro-symbolic):
- Formal verification of LLM outputs
- Checks functional equivalence

**TransCoder**:
- Unit tests for equivalence checking
- No formal guarantees

### Best Practices
- Human oversight essential (edge cases, domain logic)
- Phased/incremental migration
- Migrate components while maintaining integration
- Don't attempt full system rewrites

### Moss Implementation Notes
- Could add `moss migrate <file> --to <lang>` command
- Use tests as equivalence oracles
- Incremental: function-by-function, not whole-file
- Generate type mappings between languages
- Consider: AST-to-AST translation (more reliable than text-based)

## Automated Code Review

### The Problem
- Review backlogs: PRs waiting days for attention
- Inconsistent feedback from different reviewers
- Complexity grows → thorough review harder

### Current State (2025)

**GitHub Copilot Code Review (CCR):**
- Integrates CodeQL + linters (ESLint)
- Combines semantic analysis + rule-based checks
- Can hand off fixes to Copilot coding agent
- @copilot mentions apply suggested fixes automatically

**Qodo PR Agent:**
- Industrial adoption studied (Dec 2024)
- LLM-based automated code review
- Evaluated: effectiveness, PR closure speed, review volume changes

**SWR-Bench** (Sept 2025):
- 1000 manually verified PRs from GitHub
- PR-centric review with full project context
- Addresses: existing benchmarks lack real-world complexity

### Tools

| Tool | Features |
|------|----------|
| **Codedog** | GPT-powered, GitHub/GitLab, summaries + suggestions |
| **PR Review Bot** | Open-source, auto-approve or request changes |
| **Code Llama + Docker** | Local review, pre-commit checks |

### What LLMs Catch
- Bugs and logic errors
- Security vulnerabilities
- Style inconsistencies
- Before human reviewers see the PR

### Integration Pattern
```yaml
# CI/CD pipeline integration
on: pull_request
jobs:
  ai-review:
    runs-on: ubuntu-latest
    steps:
      - uses: codedog-ai/codedog@v1
        with:
          openai_api_key: ${{ secrets.OPENAI_API_KEY }}
```

### Moss Implementation Notes
- `moss review` command for PR analysis
- Integration: GitHub Action / GitLab CI
- Use skeleton + deps for context
- Categories: bugs, security, style, performance
- Consider: Review guidelines from CLAUDE.md as prompt context

## Context Engineering Resources

### Agent Skills for Context Engineering
- **Repo**: https://github.com/muratcankoylan/Agent-Skills-for-Context-Engineering
- **What it is**: Curated collection of reusable "skills" for building AI agent systems, focused on context management

**Core Insight**: Context engineering differs from prompt engineering - it's about "holistic curation of all information entering the model's limited attention budget."

**Key Concepts:**

**1. Context Compression Strategies:**
- **Anchored Iterative Summarization**: Maintains persistent summaries with dedicated sections (session intent, file modifications, decisions, next steps). New content merged incrementally, not regenerated.
- **Opaque Compression**: Highly compressed representations for reconstruction, sacrifices human readability
- **Regenerative Full Summary**: Detailed structured summaries each cycle, readable but potentially lossy

**Compression Triggers:**
- Fixed threshold (70-80% context utilization)
- Sliding window (last N turns + summary)
- Importance-based (prioritize low-relevance sections)
- Task-boundary (compress at logical completion points)

**Evaluation**: Probe-based testing for factual recall, artifact trail integrity, continuation capability.
**Key Metric**: "Tokens-per-task" not tokens-per-request - aggressive compression often triggers costly re-fetching.

**2. Memory System Architectures (Spectrum Approach):**
| Level | Scope | Latency | Persistence |
|-------|-------|---------|-------------|
| Working Memory | Context window | Zero | Volatile |
| Short-Term | Session-scoped | Low | Session |
| Long-Term | Cross-session | Medium | Permanent |
| Entity Memory | Entity tracking | Low | Cross-session |
| Temporal KGs | Time-aware facts | Medium | Permanent |

**Implementation Patterns:**
- File-System-as-Memory: Directory hierarchies + structured formats (no infrastructure)
- Vector RAG with Metadata: Semantic search + rich filtering
- Knowledge Graphs: Explicit entity/relationship modeling
- Temporal KGs: Facts with "valid from/until" timestamps

**Performance**: Zep benchmark shows 90% latency reduction (2.58s vs 28.9s) at 94.8% accuracy vs 60-70% for vector RAG.

**3. Multi-Agent Patterns:**

| Pattern | Key Insight |
|---------|-------------|
| Supervisor/Orchestrator | **"Telephone game problem"** - supervisors paraphrase incorrectly. Solution: `forward_message` tool for direct passthrough |
| Peer-to-Peer/Swarm | No single point of failure, exploration-based |
| Hierarchical | Strategy → Planning → Execution layers |

**Critical Insight**: "Sub-agents exist primarily to isolate context" - not to simulate organizational roles.

**4. Tool Design Principles:**
- **Consolidation Principle**: "If a human can't definitively say which tool to use, an agent can't either" - favor comprehensive over fragmented tools
- **Descriptions as Prompts**: Answer what/when/inputs/outputs explicitly
- **Architectural Reduction**: Provide primitives over specialized tools (when data is well-documented)
- **Response Format Optimization**: Let agents control verbosity
- **Tool Limit**: ~10-20 tools to prevent selection confusion

**5. Context Optimization Techniques:**
- **Compaction**: Summarize at limits, never compress system prompt
- **Observation Masking**: Replace verbose tool outputs (80%+ of tokens) with references
- **KV-Cache Optimization**: Stable elements first (system prompt, tool defs) for cache hits
- **Context Partitioning**: Distribute to sub-agents with isolated contexts

**Triggers**: Optimize when context utilization >70%. Targets: 50-70% token reduction, 70%+ cache hits.

**Moss Observations:**

**Actually Useful:**
- **Tool Consolidation Principle**: Validates moss's few-powerful-tools philosophy (view, edit, analyze)
- **Progressive disclosure**: Moss's skeleton view is exactly this pattern
- **Memory architecture spectrum**: Could inform cross-session learning design

**Addresses Symptoms, Not Causes:**
Most techniques here are reactive fixes for the **append-only trajectory** anti-pattern:
- Compression triggers, observation masking, context compaction - all band-aids for "log grew too big"
- Sub-agent context isolation - treats sub-agents as garbage collectors rather than meaningful abstractions

The root cause: treating conversation as an append-only log that inevitably fills up.

Moss's approach differs on two axes:
1. **Structural awareness**: Load only what's needed (skeleton, targeted extraction)
2. **Dynamic context**: Trajectory is not append-only - context can be reshaped throughout execution

When context is dynamic rather than accumulated, compression/masking become unnecessary.

**Key Metric Worth Adopting**: "Tokens-per-task" not tokens-per-request. Measures end-to-end efficiency including re-fetching costs from over-aggressive compression.

## Benchmarking TODO

- [ ] Implement SWE-bench evaluation harness
- [ ] Compare moss's anchor-based patching vs search/replace vs diff
- [ ] Measure structural context (skeleton) value vs raw file context
- [ ] Test architect/editor pattern with moss infrastructure
