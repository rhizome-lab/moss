# Agent Design Notes

Design decisions and implementation notes for `moss @agent`.

## Current Implementation

**File:** `crates/moss/src/commands/scripts/agent.lua`

### System Prompt

```
Coding session. Output commands in [cmd][/cmd] tags. Multiple per turn OK. Conclude quickly using done.
[commands]
[cmd]done answer here[/cmd]
[cmd]view [--types-only|--full|--deps] .[/cmd]
[cmd]view src/main.rs[/cmd]
[cmd]view src/main.rs/main[/cmd]
[cmd]text-search "pattern"[/cmd]
[cmd]edit src/lib.rs/foo [delete|replace|insert|move][/cmd]
[cmd]package [list|tree|info|outdated|audit][/cmd]
[cmd]analyze [complexity|security|callers|callees|...][/cmd]
[cmd]run cargo test[/cmd]
[cmd]ask which module?[/cmd]
[/commands]
```

Key design choices:
- `done` listed first to emphasize task completion
- "Conclude quickly" efficiency hint
- "Multiple per turn OK" enables batching
- BBCode `[cmd][/cmd]` format (see Rejected Formats below)
- `[commands][/commands]` wrapper for examples

### CLI Usage

```bash
moss @agent "What language is this project?"
moss @agent --provider anthropic "List dependencies"
moss @agent --max-turns 10 "Refactor the auth module"
moss @agent --explain "What does this do?"  # Asks for step-by-step reasoning
```

### Architecture

- **Pure Lua**: Agent loop is 100% Lua, Rust only exposes primitives
- **Multi-turn chat**: Proper user/assistant message boundaries via `llm.chat()`
- **Per-turn history**: Each turn stores `{response, outputs: [{cmd, output, success}]}`
- **Loop detection**: Warns if same command repeated 3+ times
- **Shadow git**: Snapshots before edits, rollback on failure
- **Memory**: Recalls relevant context from previous sessions via `recall()`

### Retry Logic

Exponential backoff (1s, 2s, 4s) for intermittent API failures. Reports total retry count at session end if > 0.

### Model Behavior

| Model | Turns for simple Q | Style |
|-------|-------------------|-------|
| Gemini Flash 3 | 4-5 | Thorough exploration, then answers |
| Claude Sonnet | 3-4 | Efficient, uses `[cmd]done[/cmd]` wrapper |

Flash explores more but concludes reliably with current prompt tuning.

## Python Implementation (to port)

### TaskTree / TaskList

Old orchestration had hierarchical task structures:

```python
# TaskTree: hierarchical decomposition
task = TaskTree(
    goal="implement feature X",
    subtasks=[
        TaskTree(goal="understand current code", ...),
        TaskTree(goal="write implementation", ...),
        TaskTree(goal="add tests", ...),
    ]
)

# TaskList: flat sequence with dependencies
tasks = TaskList([
    Task(id="1", goal="read spec", deps=[]),
    Task(id="2", goal="implement", deps=["1"]),
    Task(id="3", goal="test", deps=["2"]),
])
```

Questions:
- Did hierarchical actually help? Or just added complexity?
- Was flat list sufficient in practice?
- How were dependencies tracked/resolved?

### Driver Protocol

Agent decision-making abstraction:

```python
class Driver:
    def decide(self, context: Context) -> Action:
        """Given current state, return next action."""
        pass

    def observe(self, result: Result) -> None:
        """Update internal state after action."""
        pass
```

Multiple driver implementations:
- `SimpleDriver`: Just call LLM each turn
- `PlanningDriver`: Plan first, then execute
- `ReflectiveDriver`: Periodically assess progress

### Checkpointing / Session Management

```python
session = Session.create(goal="...")
session.checkpoint("before risky edit")
# ... do work ...
if failed:
    session.rollback("before risky edit")
```

Now we have `shadow.*` for this.

## Unification Opportunities

Fewer concepts = less cognitive load.

### Task = Task

No separate "TaskTree" vs "TaskList". A task has optional subtasks. That's a tree. Flattened, it's a list. Same data structure.

```lua
task = {
  goal = "implement feature",
  subtasks = {  -- optional
    { goal = "understand code" },
    { goal = "write impl" },
    { goal = "add tests" },
  }
}
```

### One Invocation Format

Don't support 5 formats. Pick one.

Text/shell-style is probably right - it's what LLMs see in training data. JSON schemas are learned; shell commands are native.

The "structured tool call" is the provider's problem (Anthropic tool_use, OpenAI function_call). Our agent just emits text.

### Context is Context

No separate concepts for "conversation", "memory", "relevant code", "system prompt". It's all strings in the prompt.

The only question is what goes in and what stays out. That's curation, not categorization.

## Open Questions

### Tool Invocation Format

**Assumption to question:** "Structured tool calls (JSON) are better than text parsing"

Counter-argument: LLMs are trained on text. Shell commands and prose are native. JSON schemas are learned later. Which actually works better?

Options:
1. **Text/shell style**: `> view src/main.rs --types-only`
2. **XML tags**: `<tool name="view"><arg>src/main.rs</arg></tool>`
3. **JSON**: `{"tool": "view", "args": {"target": "src/main.rs"}}`
4. **Function calling API**: Provider-specific (Anthropic tool_use, OpenAI function_call)
5. **Prose**: "Please show me the structure of src/main.rs"
6. **BBCode style**: `[cmd]view src/main.rs[/cmd]`

Factors:
- Reliability of parsing
- Token efficiency
- Model familiarity (training data distribution)
- Provider compatibility

#### Rejected Formats

**`> ` prefix (text/shell style)**
- Problem: Claude models hallucinate command outputs after the `> ` line
- Cause: `> ` is common in training data (shell prompts, quotes), model continues naturally
- Result: Agent generates fake outputs instead of waiting for real execution
- Tested with: Claude Sonnet

**`<cmd></cmd>` (XML tags)**
- Problem: Gemini models hallucinate command outputs
- Cause: XML tags look too similar to HTML, thinking blocks, or structured formats in training data
- Gemini 3 Flash: Hallucinated fake project outputs, responded in Chinese
- Gemini 3 Pro: Works correctly, used multiple commands per turn effectively (but more expensive)
- Claude models: Works correctly with multi-turn chat
- Result: Inconsistent across providers

#### Current Format: BBCode

**`[cmd][/cmd]` (BBCode style)**
- Chosen because it's visually distinct from HTML, XML, and thinking blocks
- Less likely to trigger learned patterns from structured data
- Still machine-parseable: `%[cmd%](.-)%[/cmd%]` (Lua pattern)

#### Gemini Flash 3 Quirks (500 errors / empty responses)

Testing revealed certain phrases trigger 500 errors or empty responses:

**Trigger phrases (500 error):**
- "shell" command in examples → use "run" or "exec" instead
- "Multi-turn shell session" → use "Coding session" instead
- Multiple example commands without `<pre>` wrapper

**Trigger phrases (empty response):**
- "Shell session" alone
- "I execute them" → sounds like asking AI to do something dangerous
- Various "session" + "execute" combinations

**Workarounds:**
- Wrap command examples in `[commands][/commands]` tags (BBCode, not `<pre>`)
- Use "Coding session" instead of "shell session"
- Avoid "execute" - use "run" or "show results"
- Avoid "shell" command - use "run"

**Works reliably:**
- "Coding session. Output commands in [cmd][/cmd] tags."
- `[commands][cmd]view .[/cmd][/commands]`
- "view", "text-search", "edit", "done", "run", "ask" commands

**Potential fix:**
- Rig library hardcodes `safety_settings: None` for Gemini
- Could set lower thresholds via `additional_params` or fork rig
- Currently using retry logic as workaround (3 attempts)

### Context Model

Current: append everything (conversation style).

The problem isn't "context fills up" - 200k tokens is huge. The problem is:
- Append-only model keeps irrelevant old stuff
- Verbose tools waste space
- Same content re-read multiple times

**Not the solution:** sliding windows, priority queues, compression. These are bandaids.

**Actual solution:**
- Tools return minimal structural output (already what moss does)
- Context can be *reshaped*, not just appended (drop irrelevant, keep relevant)
- Memory (`store`/`recall`) for cross-session persistence, not in-session compression

### Planning vs Reactive

Two modes:
1. **Reactive**: Each turn, decide what to do next based on current state
2. **Planning**: Produce a plan upfront, then execute steps

Trade-offs:
- Planning: Better for known workflows, worse when plan is wrong
- Reactive: More flexible, but can loop/wander
- Hybrid: Plan loosely, revise as you go

### Loop Detection

Agents get stuck. Signs:
- Same action repeated 3+ times
- Same error message recurring
- No progress toward goal

Responses:
- Reflect ("I'm stuck because...")
- Backtrack (rollback to checkpoint)
- Escalate (ask user for help)
- Give up (exit with explanation)

### No Budget

**Rejected idea:** token budgets, context compression, `/compact` commands.

Why budgets exist elsewhere:
- Append-only conversation fills up
- Verbose tools dump entire files
- Agent re-reads same content repeatedly

Why budgets are a footgun:
- Lost-in-the-middle problem (compression loses signal)
- Complexity (priority queues, summarization, knobs to tune)
- Bandaid for bad tool design

**Instead:** Design tools that don't waste context.
- `view --types-only` returns 50 lines, not 2000
- `view Foo/bar` returns one function, not whole file
- Index queries return answers, not grep output to scan
- Context can be reshaped, not just appended

If tools are good, 200k tokens is a novel. Plenty of room.

## Integration with Existing Primitives

### shadow.* for Rollback

```lua
shadow.open()
local before = shadow.snapshot({"src/"})

-- agent works...
result = agent.execute(task)

if not result.success then
    shadow.restore(before)
    print("Rolled back due to failure")
end
```

### store/recall for Memory

```lua
-- After learning something
store("The auth module uses JWT tokens in cookies", {
    context = "architecture",
    weight = 0.8
})

-- Before starting a task
local hints = recall(task.description, 5)
for _, h in ipairs(hints) do
    context:add(h.content)
end
```

### Investigation Flow

From dogfooding notes:
```
view . → view <file> --types-only → analyze --complexity → view <symbol>
```

Agent should learn this pattern, not rediscover it each time.

## Adaptation (from agent-adaptation.md)

Moss tools are **T1** (agent-agnostic). They improve independently:
- Index refresh via file watching
- Grammar updates
- Output format improvements

Not doing **A1/A2** (agent adaptation) - that requires fine-tuning LLMs, outside scope.

**T2** (agent-supervised tool adaptation) is the interesting edge:
- Observe agent friction (repeated queries, workarounds, corrections)
- Adjust tool defaults based on usage patterns
- No LLM fine-tuning, just tool improvement

## Related Files

- `crates/moss/src/commands/scripts/agent.lua` - Agent loop implementation
- `crates/moss/src/workflow/lua_runtime.rs` - Lua bindings including `llm.chat()`
- `crates/moss/src/workflow/llm.rs` - Multi-provider LLM client
- `docs/research/agent-adaptation.md` - Adaptation framework analysis
- `docs/lua-api.md` - Available Lua bindings

## Completed

- [x] Tool invocation format: BBCode `[cmd][/cmd]`
- [x] Minimal agent loop in Lua
- [x] Loop detection (same command 3+ times)
- [x] Multi-turn chat with proper message boundaries
- [x] Retry logic with exponential backoff
- [x] Shadow git integration for rollback
- [x] Memory integration via `recall()`

## Next Steps

1. Tune prompt for faster conclusions (Flash takes 4-5 turns vs ideal 2-3)
2. Test with edit tasks (not just exploration)
3. Improve `--explain` flag (Flash ignores step explanation request)
4. Consider streaming output for long responses
