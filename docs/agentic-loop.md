# DWIM-Driven Agentic Loop

Design for an indefinite agent loop where the LLM outputs terse intents and DWIM handles tool routing.

## Philosophy

- **LLM makes decisions**, not tool selections
- **DWIM interprets intent** and routes to appropriate tools
- **No tool schemas in prompts** - saves tokens, reduces coupling
- **Terse agent output** - "skeleton foo.py" not "please show me the structure"
- **Natural language for humans** - same DWIM handles both

## Architecture

```
┌──────────────────────────────────────────────────────┐
│                   AGENT LOOP                          │
├──────────────────────────────────────────────────────┤
│                                                       │
│  USER/LLM                                             │
│     │                                                 │
│     ▼                                                 │
│  "skeleton foo.py"  ─or─  "show me the structure"    │
│     │                                                 │
│     ▼                                                 │
│  DWIM PARSER                                          │
│     ├─ Extract verb: skeleton, expand, fix, validate │
│     ├─ Extract target: file path, symbol, error desc │
│     └─ Route to tool with confidence score           │
│     │                                                 │
│     ▼                                                 │
│  TOOL EXECUTOR                                        │
│     ├─ MossAPI (skeleton, patch, validate, etc.)     │
│     ├─ MCP servers (external capabilities)           │
│     └─ LLM (when tool needs generated content)       │
│     │                                                 │
│     ▼                                                 │
│  RESULT → fed back to LLM                            │
│     │                                                 │
│     ▼                                                 │
│  LLM → next intent ─or─ "done"                       │
│                                                       │
└──────────────────────────────────────────────────────┘
```

## Agent Output Format

Terse, token-efficient. Verb + target(s):

```
skeleton src/moss/agent_loop.py
expand Patch
validate
fix: Patch.anchor should accept str | Anchor
grep "def analyze" src/moss/
done
```

No prose, no "I will now...", just action.

## DWIM Responsibilities

1. **Verb extraction** - identify action: read, skeleton, expand, fix, validate, grep, done
2. **Target extraction** - parse file paths, symbol names, search patterns
3. **Tool routing** - map intent to best MossAPI/MCP tool
4. **Confidence scoring** - know when to ask for clarification
5. **Parameter construction** - build tool call from extracted parts

## Integration Points

### Existing Infrastructure

- `moss.dwim` - already has `analyze_intent()`, `ToolRouter`, TF-IDF matching
- `moss.session` - tracks tool calls, file changes, LLM usage
- `moss.agent_loop` - has `AgentLoopRunner`, executors, metrics
- `litellm` - unified LLM access

### What Needs Building

1. **Intent parser** - extract verb + targets from terse commands
2. **Main loop** - iterate: LLM → DWIM → execute → result → LLM
3. **Completion detection** - recognize "done" signal
4. **Context management** - what to feed back to LLM (truncation, summarization)

## Example Flow

User: "Fix the type error in Patch.apply"

```
LLM: skeleton src/moss/patches.py
     → DWIM routes to skeleton_format
     → Returns: class Patch, def apply(...)

LLM: expand Patch.apply
     → DWIM routes to skeleton_expand
     → Returns: full function body

LLM: fix: add type check for anchor parameter
     → DWIM detects "fix" verb, asks LLM for patch content
     → LLM generates: if isinstance(anchor, str): anchor = Anchor(anchor)
     → DWIM routes to patch_apply

LLM: validate
     → DWIM routes to validation_validate
     → Returns: no errors

LLM: done
     → Loop terminates
```

## Token Efficiency

Compared to tool-schema approach:

| Approach | Tokens/turn | Notes |
|----------|-------------|-------|
| OpenAI function calling | ~500-2000 | Full schemas every request |
| Claude Code XML | ~200-500 | Tool blocks + formatting |
| DWIM terse | ~10-50 | Just "skeleton foo.py" |

90%+ token reduction for tool selection.

## Context Model: Hierarchical Path

The agent does NOT accumulate conversation history. Context is structured as a **path** from root task to current leaf, with optional attachments.

### Core Structure

```
Task: Fix auth bug
  → Find failure point ✓ (token expires during refresh)
  → Implement fix
    → [now] Patching refresh_token()

[note: refresh_token() is called from 3 places | expires: on_done]
```

### Design Principles

- **Context-excluded by default**: Start lean, pull what's needed
- **Path, not history**: Chain of refinements, not transcript
- **Levels emerge, not predefined**: Arbitrary depth, task dictates structure
- **Recursive breakdown is fundamental**: Agent decomposes until leaf is actionable

### Path Components

Each node in the path:
- `goal`: What this step aims to do
- `status`: pending | active | done | blocked
- `summary`: One-line result (when done)
- `description`: Expandable detail (on demand)
- `children`: Subtasks (if decomposed)

### Attachments

Standalone notes that travel with context:
- `content`: The note itself
- `condition`: When to expire (on_done, after:N_turns, until:pattern_found, manual)
- `scope`: Which subtree it applies to

```
note("refresh_token calls: auth.py:45, session.py:120, api.py:89", expires="on_done")
note("avoid changing public API", expires="manual")
```

### Prompt Structure

Each turn:
```
[system: terse agent role]
[path: Task → Subtask → Current step]
[notes: active attachments]
[last_result: preview + id:0042]
[action?]
```

~300 tokens typical, scales with path depth not turn count.

### State is External

- Path nodes: TaskTree structure
- Full outputs: EphemeralCache (by ID)
- Notes: Attachment store with TTL
- Findings: Working memory (compact)

## Open Questions

1. **Ambiguity handling** - when DWIM confidence is low, ask LLM to clarify or just pick best?
2. **Error recovery** - tool fails, how does LLM know? Structured error format?
3. **Multi-step intents** - "fix and validate" in one line?
4. **Decomposition trigger** - when does a task become subtasks? LLM decides? Heuristic?

## Related Docs

- `docs/dwim-architecture.md` - DWIM internals
- `docs/hybrid-loops.md` - CompositeToolExecutor for multi-source tools
- `docs/philosophy.md` - minimize LLM usage, structure over text
