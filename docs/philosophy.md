# Moss Philosophy

This document contains the design philosophy and architectural overview for Moss.
For behavioral rules and conventions, see `CLAUDE.md`.

## Project Overview

Moss is **structural code intelligence as a platform**. It provides tools for understanding, navigating, and modifying code at a structural level (AST, control flow, dependencies) rather than treating code as text.

| User | Interface | Use Case |
|------|-----------|----------|
| Developer | CLI, TUI | Understand unfamiliar code, explore structure |
| AI Agent | MCP, Library | Get structured context, make safe modifications |
| IDE | LSP | Code intelligence, navigation, refactoring |
| CI/CD | CLI | Quality gates, validation, analysis |
| Tool Builder | Library | Build custom tools on structural primitives |

Moss is useful alone and powerful with AI. A human can `moss view` to understand a file; an agent can use the same capability to build context for code generation.

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

---

## Design Tenets

### Core Philosophy

#### Generalize, Don't Multiply

When facing N use cases, prefer one flexible solution over N specialized ones. Composability reduces cognitive load, maintenance burden, and token cost.

Examples:
- Three primitives (view, edit, analyze) with rich options, not 100 specialized tools
- Log formats as plugins, not N hardcoded parsers
- `--json` + `--jq` + `--pretty` flags, not `--format=X` for every format
- Lua for workflows instead of TOML-for-simple + DSL-for-complex
- Distros that compose, not fork

Complexity grows linearly with primitives, exponentially with combinations. A system with 3 composable primitives is simpler than one with 30 specialized tools, even if the 30-tool system has less code per tool.

#### Separate Interface, Unify Plumbing

The user-facing interface should reflect intent (view, edit, analyze are different actions). The implementation should share machinery (path resolution, filtering, node targeting, output formatting).

This gives us:
- **Clarity**: Users know what operation they're performing
- **Safety**: Read vs write intent is explicit
- **DRY**: One path resolver, one filter implementation, one tree model
- **Consistency**: Same `--type`, `--calls`, `--deps` flags work everywhere

The interface serves the user's mental model. The plumbing stays unified.

#### Structure Over Text

Return structure, not prose. Structured data composes; text requires parsing.

Hierarchy implies trees. Code (AST), files (directories), tasks (subtasks), agents (sub-agents)—all trees. Design operations that work on trees: prune, query, navigate, transform. When something isn't a tree (call graphs, dependencies), it's a tree with cross-links.

Few orthogonal primitives beat many overlapping features. Lua got this right with tables. Find the smallest set of operations that compose well, not the largest set of features that cover cases.

#### Unified Codebase Tree

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

#### Three Primitives

| Primitive | Purpose | Composable options |
|-----------|---------|-------------------|
| `view` | See/find nodes | `--depth`, `--deps`, `--type`, `--calls`, `--called-by` |
| `edit` | Modify a node | `--insert`, `--replace`, `--delete`, `--move` |
| `analyze` | Compute properties | `--health`, `--complexity`, `--security` |

Depth controls expansion: `view src/ --depth 2` shows files and their top-level symbols. Filters compose: `view --type function --calls "db.*"` finds functions that call database code.

Discoverability through simplicity. With 100+ tools, users can't find what they need. With 3 primitives and composable filters, the entire interface fits in working memory.

Put smarts in the tool, not the schema. Tool definitions cost context. With only 3 primitives, there's no ambiguity about which tool to use—the cognitive load disappears entirely.

#### Err on Keeping Data

Disk is cheap. Lost work is expensive. When in doubt, preserve:
- History should be additive (undo creates branches, doesn't destroy)
- Deletions should be recoverable (shadow git, trash, soft delete)
- Prune explicitly, not automatically
- Default to archaeology over cleanup

This doesn't mean keep everything forever—it means destruction requires intent, not accident.

---

### LLM & Agent Design

#### LLM Efficiency

LLM calls are expensive (cost) and slow (latency). Design everything to reduce them:
- **Structural tools first**: Use AST, grep, validation—not LLM—for deterministic tasks
- **LLM only for judgment**: Generation, decisions, ambiguity resolution
- **Measure separately**: Track LLM calls vs tool calls in benchmarks
- **Cache aggressively**: Same query → same answer (where applicable)

This is why we have skeleton views (understand code without LLM) and validation loops (catch errors without LLM). The goal: an agent that calls the LLM 10x less than naive approaches.

**Token efficiency in prompts**: When you do call an LLM, minimize tokens:
- Never send full code when a skeleton or snippet suffices
- Forbid preamble, summary, markdown formatting unless asked
- Use structured output parsing instead of free-form text
- Limit analysis to 5 bullet points max

Result: 12x reduction in output tokens (1421 → 112) with same quality insights.

**Operational efficiency**:
- Run independent operations concurrently (parallelism)
- Validation before commit, not after (fast feedback loops)
- Avoid wasted retry cycles—get it right the first time
- Future goal: diff-based editing to avoid sending unchanged code

#### Never Extract Data Manually

LLMs should never manually extract, enumerate, or guess data that tools can provide deterministically. This includes:
- Symbol names (use `view` to get actual symbols)
- File lists (use glob/find tools)
- Function signatures (use AST-based extraction)
- Dependencies (use import graph tools)

When an LLM tries to manually enumerate symbols, it hallucinates. We've seen models generate 2000+ fake symbol names following plausible patterns (`resolve_path_command`, `resolve_path_chain`, etc.) that don't exist. The fix isn't better prompting—it's ensuring the LLM never attempts extraction in the first place.

Rule: If data exists in the codebase, there must be a tool to retrieve it. The LLM's job is to decide *what* to look up, not to guess *what exists*.

#### Non-Interactive Fallbacks

Every interactive feature must have a non-interactive equivalent. LLMs cannot respond to prompts, and scripts need deterministic behavior.

- Interactive prompts → flags that specify the choice upfront
- `--hunk` (interactive selection) → `--hunk-id h1,h3` or `--lines 10-25`
- Confirmation prompts → `--yes` / `--no` flags
- Prompts with defaults → flags that match default behavior (`--all-worktrees` vs `--local`)

This isn't just about LLMs—it's about scriptability, CI/CD, and reproducibility. If a human can do it interactively, automation should be able to do it non-interactively.

#### Minimizing Error Rates

Validation and heuristics are primary citizens in Moss. Because LLMs are not 100% reliable, we must never trust their output implicitly:
- **Verification First**: Every change must pass through a domain-specific validator (compiler, linter, test suite)
- **Heuristic Guardrails**: Use structural rules to detect obvious mistakes before they reach the validator
- **Correction over perfection**: Focus on fast feedback loops that allow the agent to correct itself based on deterministic error signals

---

### User Experience

#### Defaults & Onboarding

**Maximally useful defaults**: Every configurable option should have a default that:
- Works well for the common case (80% of users shouldn't need to configure it)
- Errs on the side of usefulness over safety-theater
- Can be discovered and changed when needed

**Low barrier to entry**:
- Works out of the box with minimal configuration
- Sensible defaults for common workflows
- Progressive disclosure: simple things simple, complex things possible
- Clear error messages that guide users toward solutions

**Fast specialization**: Good defaults mean acceptable general performance out of the box. But we should absolutely support hyper-specialization:
- **Quick wins first**: Default config should "just work" reasonably well
- **Escape hatches**: When defaults aren't enough, specialization should be one step away
- **Zero-to-custom fast**: The path from "using defaults" to "fully customized" should be short and obvious
- **No ceiling**: Power users shouldn't hit walls. If someone wants to optimize for their exact workflow, let them

This is a conscious tradeoff: defaults optimize for breadth (works for everyone), specialization optimizes for depth (works perfectly for you). Both are valid, and the system should excel at both ends of the spectrum.

#### Error Recovery & Affordances

**Forgiving lookups**: Agents and humans make mistakes—typos, wrong conventions, forgotten paths. Every lookup should be forgiving:
- Fuzzy file resolution: `prior_art` finds `prior-art.md`
- Symbol search tolerates partial names and typos
- Pattern: try exact → try fuzzy → try corrections → ask for clarification

Note: With only 3 primitives, tool selection ambiguity is eliminated. This applies to path and symbol resolution, not tool choice.

**Suggest obvious corrections**: When something seems wrong, suggest the likely fix. Not "here's what you could do" (overwhelming) but "did you mean X?" (helpful).
- Symbol not found → "Did you mean: `moss text-search 'foo' file.rs`"
- File not found → suggest fuzzy matches or similar names
- Operation failed → suggest the recovery action

**Report what was done**: After mutations, show what changed so users can validate nothing unexpected happened.
- Files changed, lines added/removed
- Summary matches what `--dry-run` would show
- Especially important when the effect isn't obvious from the command
- Enables quick "undo if wrong" decisions

The goal: users should never wonder "did that work?" or "what did that do?"

#### Works Anywhere

**Messy codebases**: Real-world code is often messy. Moss should:
- Handle legacy code without requiring refactoring first
- Degrade gracefully when AST parsing fails (text fallbacks)
- Support incremental improvement (clean up as you go, or don't)
- Not impose architectural opinions unless asked

**Just work, then customize**: Tools should work immediately on whatever users already have:
- Parse common formats without configuration (TODO.md, CHANGELOG, configs)
- Handle variations gracefully (checkboxes, numbers, bullets, headers)
- Never require users to restructure files to match tool expectations
- Detect patterns, don't mandate them

When structure is ambiguous, make a reasonable choice. When truly unclear, ask—but aim for that to be rare. The goal: zero configuration for 90% of cases, explicit config for edge cases.

#### Workflows Become Presets

If you have a workflow, the intuitive way to proceed should be to codify it. Custom presets are first-class citizens:
- **Capture patterns**: Repeated sequences of actions should become single commands
- **User-defined skills**: `.moss/skills/` for domain-specific behaviors
- **Progressive formalization**: Start ad-hoc, graduate to preset when patterns emerge
- **Shareable**: Presets should be easy to share, version, and compose

The goal: reduce the distance between "I do this often" and "now it's a command."

#### UX Principles

- **No modals** - Everything inline, no popups blocking context
- **No nested menus** - Flat, searchable action lists
- **Actions visible** - Show what's possible, don't hide capabilities
- **Direct manipulation** - Click/select to act, not navigate menus
- **Mouse support** - Full mouse interaction everywhere (especially TUI)
- **Progressive disclosure** - Start simple, reveal depth on demand

Same mental model across all interfaces:
1. **Start anywhere** - File, function, class, symbol, or natural language query
2. **Traverse by relationship** - calls → called-by → imports → similar-to
3. **Zoom fluently** - Full source ↔ skeleton ↔ signature ↔ one-liner
4. **Context preserved** - Breadcrumbs, back/forward, history

---

### Implementation

#### Architecture

**Hyper-modular**: Prefer many small, focused modules over fewer large ones:
- Maintainability: Easier to understand, modify, and test small units
- Composability: Small pieces combine flexibly
- Refactorability: Can restructure without rewriting everything

**Library-first**: The core should be a reusable Rust library (`crates/moss/`). Interfaces (CLI, MCP, LSP) are thin wrappers around the library.

**Everything is a plugin**: Where possible, use plugin protocols instead of hardcoded implementations. Even "native" integrations should implement the same plugin interface as third-party ones.

#### Resource Efficiency

Moss should be extremely lightweight. High memory usage is a bug:
- **Low RAM footprint**: Favor streaming and lazy loading over large in-memory caches
- **Minimal context**: Never send full code when a skeleton or snippet suffices
- **Transparent metrics**: Every command should optionally show context and RAM usage breakdowns

---

## Meta

Nothing good appears from scratch. Iterate. CLAUDE.md grew through 20+ commits, not upfront investment. Features emerge from use, not design documents. Start minimal, capture what you learn, repeat.
