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
You are an EXPLORER. Suggest commands to gather information.

Commands:
$(view path) - file structure/symbols
$(view path:start-end) - specific lines
$(text-search "pattern") - search codebase
$(run cmd) - shell command

Output commands directly. Do NOT answer the question - that's the evaluator's job.
Example: $(view src/main.rs) $(text-search "config")
]],
            context = "last_outputs",
            next = "evaluator",
        },
        evaluator = {
            prompt = [[
You are an EVALUATOR, not an explorer. Your ONLY job is to judge what we found.

RULES:
1. NEVER output commands (not even in backticks like `view` or `text-search`)
2. NEVER say "I need to", "Let me", or "I will" - those are explorer phrases
3. You MUST either $(answer) or explain what specific info is missing

If results contain the answer: $(answer The complete answer here)
If results are partial: $(note what we found) then explain what's still needed
If results are irrelevant: explain what went wrong

Memory commands: $(keep 1 3), $(keep), $(drop 2), $(note finding)

Example good response:
"The search found `support_for_extension` in registry.rs.
$(note Language detection uses support_for_extension())
$(answer moss detects language by file extension via support_for_extension())"
]],
            context = "working_memory",
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
5. **Evaluator needs strong role framing**: Models output commands in backticks when prompt is passive. Fixed by:
   - "You are an EVALUATOR, not an explorer" (role assertion)
   - "NEVER output commands (not even in backticks)" (explicit prohibition)
   - "NEVER say 'I need to', 'Let me'" (ban exploration phrases)
   - Concrete good/bad examples showing the distinction

### Evaluator Prompt Evolution

**Problem** (session zj3y5yu4): Evaluator kept outputting commands as markdown (`view file.rs`) instead of using $(answer). Hit max turns (12) without concluding despite finding relevant code.

**Root cause**: Passive prompt "Do NOT run commands - only evaluate" didn't stop the model from *suggesting* commands. Models interpret "don't run" as "describe what you would run".

**Fix**: Strong role framing + banned phrases + concrete examples:
- Before: "Review results. Do NOT run commands - only evaluate."
- After: "You are an EVALUATOR, not an explorer. NEVER output commands. NEVER say 'I need to'..."

**Results** (post-fix):
- "How does moss detect language?" → 4 turns, correct answer (was 12 turns, no answer)
- "What CLI commands?" → 2 turns, comprehensive answer
- "Purpose of moss-languages?" → 2 turns (Gemini), correct answer

## Planning State (--plan flag)

Optional planning state that runs before exploration. Usage:
```bash
moss @agent "complex task" --v2 --plan --max-turns 12
```

Flow: planner → explorer → evaluator → (repeat or done)

The planner:
- Receives only the task (no outputs yet)
- Creates a 2-4 step plan
- Plan is shown to explorer in context

Example output:
```
[agent-v2] Turn 1/12 (planner)
[agent-v2] Thinking... 1. Find all main.rs files in the project
2. Read each file and search for struct definitions
3. Extract struct names and line numbers

Ready to explore.
[agent-v2] Turn 2/12 (explorer)
...
```

## Future Extensions

- **Recovery state**: Handle errors, suggest fixes
- **Refinement state**: After draft answer, verify/improve
- **Conditional transitions**: Based on output patterns, errors, etc.

## V1 vs V2 Comparison (2026-01-05)

**Query: "How many Provider variants are in llm.rs?"**
- V1: 5 turns, correct answer (13)
- V2: 6 turns (3 cycles), correct answer (13)

**Query: "What is the first struct in main.rs?"**
- V1: 6 turns, correct answer (Cli)
- V2: 6 turns (3 cycles), ran out of turns (ambiguous "main.rs")

**Observations:**
- V2 uses 2x turns for same work (explorer + evaluator per cycle)
- V2 has explicit memory curation ($(keep), $(drop))
- V2 prevents pre-answering (explorer can't conclude)
- V2 struggles with limited turn budgets (need ~2x v1's max_turns)

**Recommendation:** Use max_turns = 12-16 for V2 to match V1's effective exploration depth.

## Design Comparison

| Aspect | V1 (freeform) | V2 (state machine) |
|--------|--------------|-------------------|
| LLM calls per "turn" | 1 | 1 (but 2 per cycle) |
| Context | Ephemeral + working memory | Explorer: ephemeral, Evaluator: accumulated |
| Who concludes | Same agent | Only evaluator |
| Memory curation | Agent decides inline | Evaluator explicitly keeps/drops |
| Pre-answering | Can happen | Prevented by design |
| Turn efficiency | Higher | Lower (2 calls per cycle) |
