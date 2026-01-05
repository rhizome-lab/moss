# Agent State Machine Design

## Motivation

The freeform agent loop has fundamental problems (see `prior-art.md`):
1. Single LLM must both explore AND judge when to stop
2. Ephemeral context means model forgets what it saw
3. Model loops because it can't synthesize across turns

## Solution: Specialized States

Separate exploration from evaluation:

| State | Job | Sees | Outputs |
|-------|-----|------|---------|
| **Explorer** | Suggest next commands | Current task + last outputs | Commands to run |
| **Evaluator** | Judge: enough? what matters? | Task + ALL outputs accumulated | Keep/drop, continue/conclude |

## State Machine Definition

```lua
local MACHINE = {
    start = "explorer",

    states = {
        explorer = {
            prompt = [[
Suggest commands to gather information for the task.
Output one or more $(cmd) commands.
]],
            -- What context does this state see?
            context = "last_outputs",  -- only most recent outputs (ephemeral)
            -- Next state after execution
            next = "evaluator",
        },

        evaluator = {
            prompt = [[
Review the gathered information. Decide:
1. What findings are important? Use $(note finding) to record them.
2. Do we have enough to answer? If yes: $(done ANSWER)
3. If not enough: explain what's missing, then I'll explore more.
]],
            -- Evaluator sees EVERYTHING
            context = "all_outputs",  -- accumulated history
            -- Next state (unless $(done) terminates)
            next = "explorer",
        },
    },
}
```

## Execution Flow

```
Turn 1:
  [explorer] Task: "How many Provider variants in llm.rs?"
             → $(text-search "enum Provider")
  [execute commands]
  [evaluator] Sees: search results showing llm.rs:38
             → "Found location but not definition. Need to view file."
             → continues

Turn 2:
  [explorer] Task + "Need to view file"
             → $(view llm.rs:38-55)
  [execute commands]
  [evaluator] Sees: search results + enum definition
             → $(note Provider enum has 13 variants: Anthropic, OpenAI, ...)
             → $(done 13)
```

## Key Design Decisions

### 1. Context Visibility

| State | Sees | Why |
|-------|------|-----|
| Explorer | Last outputs only | Tactical: "what should I look at next?" |
| Evaluator | All accumulated | Strategic: "do we have enough across everything?" |

### 2. Who Can Conclude?

Only **evaluator** can output `$(done)`. Explorer just suggests commands.

### 3. State Transitions

For now, unconditional:
- explorer → evaluator (always, after commands execute)
- evaluator → explorer (if not concluded)
- evaluator → done (if $(done) output)

Future: conditions could be added (e.g., "if error, go to recovery state")

### 4. Accumulated Context Format

Evaluator sees all outputs in order:
```
**Task:** How many Provider variants in llm.rs?

**History:**

Turn 1 (explorer):
`text-search "enum Provider"`
```
crates/moss/src/workflow/llm.rs:38: pub enum Provider {
```

Turn 2 (explorer):
`view llm.rs:38-55`
```
pub enum Provider {
    Anthropic,
    OpenAI,
    ...
}
```

**Your job:** Note important findings, then $(done ANSWER) or explain what's missing.
```

## Implementation Status

**Implemented** in `agent.lua` as `--v2` flag. Usage:
```bash
moss @agent "query" --v2 --max-turns 10
```

### Actual MACHINE Config
```lua
local MACHINE = {
    start = "explorer",
    states = {
        explorer = {
            prompt = [[
Suggest commands to explore. Available:
$(view path) - file structure/symbols
$(view path:start-end) - specific lines
$(text-search pattern) - search codebase
$(run cmd) - shell command

Output commands directly. Example: $(view src/main.rs)
]],
            context = "last_outputs",
            next = "evaluator",
        },
        evaluator = {
            prompt = [[
Review the information above. Do NOT explore more - only evaluate what's here.
- If you can answer: $(answer The complete answer)
- If more info needed: just say what's missing (explorer will gather it)

Example: $(answer Cli is the first struct at line 13)
]],
            context = "all_outputs",
            next = "explorer",
        },
    },
}
```

### Test Results (2026-01-05)

1. **Simple query**: "What is the first struct in main.rs?"
   - 2 turns: explore → evaluate → done
   - Correct answer: Cli at line 13

2. **Complex query**: "How many enum variants does Provider have?"
   - 4 turns: explore → evaluate (need more) → explore → evaluate → done
   - Correct answer: 13 variants

3. **Open-ended query**: "What commands are available in moss CLI?"
   - 4 turns: multiple explorations → comprehensive list
   - Found 11 commands correctly

### Key Learnings

1. **Prompt examples matter**: Adding `Example: $(answer ...)` stopped models from using `$(done ANSWER) - actual answer` format
2. **Evaluator must NOT explore**: Models would hallucinate file contents instead of asking for more info. Fixed with "Do NOT explore more"
3. **Both $(done) and $(answer) accepted**: Models use both, so we handle both
4. **One LLM call per state** (not 2 per turn as originally planned): cleaner separation

## Future Extensions

- **Recovery state**: Handle errors, suggest fixes
- **Planning state**: Before exploring, plan the approach
- **Refinement state**: After draft answer, verify/improve
- **Conditional transitions**: Based on output patterns, errors, etc.

## Comparison to Current Design

| Aspect | Current (single agent) | State machine |
|--------|----------------------|---------------|
| LLM calls per turn | 1 | 2 (explore + evaluate) |
| Context | Ephemeral (forget each turn) | Explorer: ephemeral, Evaluator: accumulated |
| Who concludes | Same agent that explores | Only evaluator |
| When to stop | Model must judge while exploring | Dedicated evaluation step |
