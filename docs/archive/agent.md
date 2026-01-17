# Agent Design Notes

Design decisions and implementation notes for `moss @agent`.

## Current Implementation

**File:** `crates/moss/src/commands/scripts/agent.lua`

### System Prompt

```
Coding session. Output commands in $(cmd) syntax. Multiple per turn OK.

Command outputs disappear after each turn. To retain information:
- $(keep) or $(keep 1 3) saves outputs to working memory
- $(note key fact here) records insights
- $(done YOUR FINAL ANSWER) ends the session

$(done The answer is X because Y)
$(keep)
$(note uses clap for CLI)
$(view .)
$(view --types-only .)
$(view src/main.rs/main)
$(text-search "pattern")
$(edit src/lib.rs/foo delete|replace|insert|move)
$(package list|tree|info|outdated|audit)
$(analyze complexity|security|callers|callees)
$(run cargo test)
$(ask which module?)
```

Key design choices:
- Shell-like `$(cmd)` syntax - natural for LLMs trained on shell scripts
- `done` listed first to emphasize task completion
- "Multiple per turn OK" enables batching
- Ephemeral context with explicit `keep`/`note` for memory management

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
| Claude Sonnet/Haiku | 1-2 | Efficient, uses `[cmd]done ...[/cmd]` with answer |
| Gemini Flash 3 | max | Loops without concluding, doesn't use done format |

Claude works reliably. Gemini Flash has issues with the BBCode format - it explores endlessly without using `[cmd]done[/cmd]` to provide final answers. May need prompt tuning or switching to function calling for Gemini.

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

**`[cmd][/cmd]` (BBCode style)**
- Tested but replaced - Gemini Flash would loop without concluding
- Claude works but sometimes wraps in code blocks
- Less natural than shell syntax

#### Current Format: Shell-like $()

**`$(cmd)` syntax**
- Familiar from shell command substitution
- Models trained on lots of shell scripts recognize this pattern
- Both Claude and Gemini follow format correctly
- Gemini now concludes with `$(done answer)` instead of looping
- Lua pattern: `%$%((.-)%)`
- Caveat: nested parens would break parsing (rare in practice)
- Issue: Gemini still hallucinates command output content (invents fake file contents)
  - Format is correct, but model "thinks ahead" instead of waiting for real execution
  - May need explicit "do not simulate outputs" instruction

**Potential fix:**
- Rig library hardcodes `safety_settings: None` for Gemini
- Could set lower thresholds via `additional_params` or fork rig
- Currently using retry logic as workaround (3 attempts)

### Context Model

Current implementation has bandaids that violate our own "No Budget" principle:
- `keep_last = 10` - caps history to last 10 turns
- Output truncation - 2000 head + 1000 tail chars per command

These exist because of append-only thinking. The problem isn't context size - 200k tokens is huge. The problem is treating context as a log.

**Key insight: Alternating messages ≠ linear history.**

Provider APIs want alternating user/assistant messages. But the content doesn't have to be a literal transcript. Messages can be assembled fresh each call as curated working memory.

#### Ephemeral by Default

Outputs are shown once, then gone. Agent explicitly keeps what matters:

```
[1] $ view .
src/, tests/, Cargo.toml

[2] $ view src/main.rs --types-only
fn main(), Commands enum, ...
```

Commands:
- `keep` / `keep all` - keep all outputs from this turn
- `keep 2` - keep only output #2
- `keep 2 3` - keep outputs #2 and #3
- `note <fact>` - record a synthesized insight (not raw output)

#### Working Memory vs Sensory Buffer

- **Sensory buffer**: Current turn's outputs. Visible now, gone next turn unless kept.
- **Working memory**: Kept outputs + notes. Persists until task done.

Context only grows when agent explicitly says "this matters."

#### Why Not Drop?

Considered `drop #id` to remove items. Problems:
- Requires global ID tracking
- Agent must remember to drop (forgets → bloat)
- Default is bloated, requires active cleanup

Inverting to `keep` is better:
- No global IDs (per-turn indices only)
- Default is lean
- Agent takes positive action for what matters
- Matches how attention works: notice and retain, not accumulate and prune

#### Implementation

```lua
working_memory = {}  -- kept outputs and notes

for turn = 1, max_turns do
    -- Build context: task + working memory (not turn history)
    context = build_context(task, working_memory)

    -- Get response, execute commands
    response = llm.chat(...)
    outputs = execute_commands(response)

    -- Show indexed outputs to agent
    -- [1] $ cmd1 \n output1 \n [2] $ cmd2 \n output2

    -- Parse keep/note commands
    for kept_idx in response:gmatch("keep (%d+)") do
        table.insert(working_memory, outputs[kept_idx])
    end
    for fact in response:gmatch("note (.+)") do
        table.insert(working_memory, {type="note", content=fact})
    end
    -- bare "keep" or "keep all" keeps everything
end
```

**Not the solution:** sliding windows, priority queues, compression, `keep_last`. These manage symptoms.

**Actual solution:**
- Outputs ephemeral by default
- Agent explicitly keeps what matters
- Tools support `--jq` for extracting exactly what's needed
- "What do I know?" not "What did I do?"

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
- [x] Ephemeral context model with `keep`/`note` commands
- [x] Command limit guard (max 10 per turn) to prevent runaway output
- [x] Done command can coexist with exec commands (runs commands first, then returns)

## Next Steps

1. Fix Gemini Flash format compliance - doesn't use `[cmd]done[/cmd]` reliably
2. Test with edit tasks (not just exploration)
3. Test keep/note with multi-turn information gathering
4. Consider streaming output for long responses
