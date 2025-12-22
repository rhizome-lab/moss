# Moss Philosophy

This document contains the design philosophy and architectural overview for Moss.
For behavioral rules and conventions, see `CLAUDE.md`.

## Project Overview

Moss is tooling orchestration with structural awareness. It implements a "Compiled Context" approach that prioritizes architectural awareness (AST-based understanding) over raw text processing, with verification loops ensuring correctness before output.

## Architecture

Core components:
- **Event Bus**: Async communication (`UserMessage`, `PlanGenerated`, `ToolCall`, `ValidationFailed`, `ShadowCommit`)
- **Context Host**: Manages View Providers (Skeleton, CFG, Dependency Graph) - delegates to plugins
- **Structural Editor**: AST-based editing with fuzzy anchor matching
- **Policy Engine**: Enforces safety rules (velocity checks, quarantine)
- **Validator**: Domain-specific verification loop (compiler, linter, tests)
- **Shadow Git**: Atomic commits per tool call, rollback via git reset

Data flow: User Request → Config Engine → Planner → Context Host (Views) → Draft → Shadow Git → Validator → (retry loop if error) → Commit Handle

Multi-agent model: Ticket-based (not shared chat history). Agents are isolated microservices passing Handles, not context.

See `docs/spec.md` for the full specification.

## Design Tenets

### Generalize, Don't Multiply

When facing N use cases, prefer one flexible solution over N specialized ones. Composability reduces cognitive load, maintenance burden, and token cost.

Examples:
- Three primitives (view, edit, analyze) with rich options, not 100 specialized tools
- Log formats as plugins, not N hardcoded parsers
- `--json` + `--jq` + `--compact` flags, not `--format=X` for every format
- Distros that compose, not fork

Complexity grows linearly with primitives, exponentially with combinations. A system with 3 composable primitives is simpler than one with 30 specialized tools, even if the 30-tool system has less code per tool.

### Separate Interface, Unify Plumbing

The user-facing interface should reflect intent (view, edit, analyze are different actions). The implementation should share machinery (path resolution, filtering, node targeting, output formatting).

This gives us:
- **Clarity**: Users know what operation they're performing
- **Safety**: Read vs write intent is explicit
- **DRY**: One path resolver, one filter implementation, one tree model
- **Consistency**: Same `--type`, `--calls`, `--deps` flags work everywhere

The interface serves the user's mental model. The plumbing stays unified.

**Rust/Python boundary:** Rust is the plumbing (fast, deterministic, syntax-aware). Python is the interface (LLM integration, orchestration, TUI). See `docs/rust-python-boundary.md` for the full decision framework.

### Minimize LLM Usage

LLM calls are expensive (cost) and slow (latency). Design everything to reduce them:
- Structural tools first: Use AST, grep, validation - not LLM - for deterministic tasks
- LLM only for judgment: Generation, decisions, ambiguity resolution
- Measure separately: Track LLM calls vs tool calls in benchmarks
- Cache aggressively: Same query → same answer (where applicable)

This is why we have skeleton views (understand code without LLM) and validation loops (catch errors without LLM). The goal: an agent that calls the LLM 10x less than naive approaches.

### Never Extract Data Manually

LLMs should never manually extract, enumerate, or guess data that tools can provide deterministically. This includes:
- Symbol names (use `view` to get actual symbols)
- File lists (use glob/find tools)
- Function signatures (use AST-based extraction)
- Dependencies (use import graph tools)

When an LLM tries to manually enumerate symbols, it hallucinates. We've seen models generate 2000+ fake symbol names following plausible patterns (`resolve_path_command`, `resolve_path_chain`, etc.) that don't exist. The fix isn't better prompting—it's ensuring the LLM never attempts extraction in the first place.

Rule: If data exists in the codebase, there must be a tool to retrieve it. The LLM's job is to decide *what* to look up, not to guess *what exists*.

### Resource Efficiency

Moss should be extremely lightweight. High memory usage is a bug:
- **Low RAM footprint**: Favor streaming and lazy loading over large in-memory caches
- **Minimal context**: Never send full code when a skeleton or snippet suffices
- **Transparent metrics**: Every command should optionally show context and RAM usage breakdowns

### Minimizing Error Rates

Validation and heuristics are primary citizens in Moss. Because LLMs are not 100% reliable, we must never trust their output implicitly:
- **Verification First**: Every change must pass through a domain-specific validator (compiler, linter, test suite)
- **Heuristic Guardrails**: Use structural rules to detect obvious mistakes before they reach the validator
- **Correction over perfection**: Focus on fast feedback loops that allow the agent to correct itself based on deterministic error signals

### Prompt Engineering for Token Efficiency

When you do call an LLM, minimize output tokens. Our system prompt in `src/moss/agent_loop.py` explicitly forbids:
- Preamble and summary
- Markdown formatting (bold, headers, code blocks unless asked)
- More than 5 bullet points for analysis

Result: 12x reduction in output tokens (1421 → 112) with same quality insights.

### Structure Over Text

Return structure, not prose. Structured data composes; text requires parsing.

Hierarchy implies trees. Code (AST), files (directories), tasks (subtasks), agents (sub-agents)—all trees. Design operations that work on trees: prune, query, navigate, transform. When something isn't a tree (call graphs, dependencies), it's a tree with cross-links.

Few orthogonal primitives beat many overlapping features. Lua got this right with tables. Find the smallest set of operations that compose well, not the largest set of features that cover cases.

### Unified Codebase Tree

The codebase is one tree. Filesystem and AST are not separate—they're levels of the same hierarchy:

```
project/                    # root
├── src/                    # directory node
│   ├── main.py             # file node
│   │   ├── class Foo       # class node
│   │   │   └── bar()       # method node
│   │   └── def helper()    # function node
```

Uniform addressing with `/` everywhere:
- `src/main.py/Foo/bar` - method `bar` in class `Foo` in file `main.py`
- Resolution uses filesystem as source of truth: check if each segment is file or directory
- No ambiguity: can't have file and directory with same name in same location
- Accept multiple separators for familiarity, normalize internally:
  - `/` (canonical): `src/main.py/Foo/bar`
  - `::` (Rust-style): `src/main.py::Foo::bar`
  - `:` (compact): `src/main.py:Foo.bar`
  - `#` (URL fragment): `src/main.py#Foo.bar`

Same primitives work at every level.

**Three primitives, not 100 tools:**

| Primitive | Purpose | Composable options |
|-----------|---------|-------------------|
| `view` | See/find nodes | `--depth`, `--deps`, `--type`, `--calls`, `--called-by` |
| `edit` | Modify a node | `--insert`, `--replace`, `--delete`, `--move` |
| `analyze` | Compute properties | `--health`, `--complexity`, `--security` |

Depth controls expansion: `view src/ --depth 2` shows files and their top-level symbols. Filters compose: `view --type function --calls "db.*"` finds functions that call database code.

Discoverability through simplicity. With 100+ tools, users can't find what they need. With 3 primitives and composable filters, the entire interface fits in working memory.

Nothing good appears from scratch. Iterate. CLAUDE.md grew through 20+ commits, not upfront investment. Features emerge from use, not design documents. Start minimal, capture what you learn, repeat.

Put smarts in the tool, not the schema. Tool definitions cost context. With only 3 primitives, there's no ambiguity about which tool to use—the cognitive load disappears entirely.

### Hyper-Modular Architecture

Prefer many small, focused modules over fewer large ones:
- Maintainability: Easier to understand, modify, and test small units
- Composability: Small pieces combine flexibly
- Refactorability: Can restructure without rewriting everything

### Library-First Design

The core should be an importable Python library. Interfaces (CLI, HTTP, MCP, LSP) are wrappers around the library, ideally autogenerated from the API via introspection.

### Everything is a Plugin

Where possible, use plugin protocols instead of hardcoded implementations. Even "native" integrations should implement the same plugin interface as third-party ones.

### Maximally Useful Defaults

Every configurable option should have a default that:
- Works well for the common case (80% of users shouldn't need to configure it)
- Errs on the side of usefulness over safety-theater
- Can be discovered and changed when needed

### Good Defaults, Fast Specialization

Good defaults mean acceptable general performance out of the box. But we should absolutely support hyper-specialization for those who want it:
- **Quick wins first**: Default config should "just work" reasonably well
- **Escape hatches**: When defaults aren't enough, specialization should be one step away
- **Zero-to-custom fast**: The path from "using defaults" to "fully customized" should be short and obvious
- **No ceiling**: Power users shouldn't hit walls. If someone wants to optimize for their exact workflow, let them

This is a conscious tradeoff: defaults optimize for breadth (works for everyone), specialization optimizes for depth (works perfectly for you). Both are valid, and the system should excel at both ends of the spectrum.

### Low Barrier to Entry

Make it easy to get started:
- Works out of the box with minimal configuration
- Sensible defaults for common workflows
- Progressive disclosure: simple things simple, complex things possible
- Clear error messages that guide users toward solutions

### Forgiving Lookups

Agents make mistakes—typos, wrong conventions, forgotten paths. Every lookup should be forgiving:
- Fuzzy file resolution: `prior_art` finds `prior-art.md`
- Symbol search tolerates partial names and typos
- Pattern: try exact → try fuzzy → try corrections → ask for clarification

Note: With only 3 primitives, tool selection ambiguity is eliminated. This section applies to path and symbol resolution, not tool choice.

### Works on Messy Codebases

Real-world code is often messy. Moss should:
- Handle legacy code without requiring refactoring first
- Degrade gracefully when AST parsing fails (text fallbacks)
- Support incremental improvement (clean up as you go, or don't)
- Not impose architectural opinions unless asked

### Workflows Become Presets

If you have a workflow, the intuitive way to proceed should be to codify it. Custom presets are first-class citizens:
- **Capture patterns**: Repeated sequences of actions should become single commands
- **User-defined skills**: `.moss/skills/` for domain-specific behaviors
- **Progressive formalization**: Start ad-hoc, graduate to preset when patterns emerge
- **Shareable**: Presets should be easy to share, version, and compose

The goal: reduce the distance between "I do this often" and "now it's a command."

### Accelerate Vibe Coding

Maximize useful work per token. Minimize friction in the creative flow:
- **Token efficiency**: Never send full code when a skeleton suffices. Compress context. Use structured output parsing instead of free-form text
- **Minimal LLM calls**: Use structural tools (AST, grep, validation) for deterministic tasks. LLM only for judgment
- **Parallelism**: Run independent operations concurrently. Batch when possible
- **Fast feedback loops**: Validation before commit, not after. Catch errors early
- **Minimize error rate**: Avoid wasted retry cycles. Get it right the first time
- **Usefulness per token**: Avoid busywork. Every action should move toward the goal
- **Future goal**: Diff-based editing to avoid sending unchanged code
