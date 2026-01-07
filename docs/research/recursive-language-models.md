# Recursive Language Models (RLM)

Prior art analysis of "Recursive Language Models" (Zhang, Kraska, Khattab, 2025).

Paper: https://arxiv.org/abs/2512.24601

## Core Thesis

Long-context handling should be an **inference strategy** (search + decompose), not a **parameter** (bigger context window). Models can handle 10M+ token tasks by treating prompts as external environments to query, rather than feeding everything into context.

## Key Mechanism

Instead of:
```
LLM(very_long_prompt + question) → answer
```

RLM does:
```python
context = very_long_prompt  # Store in REPL environment
# LLM writes code to search/filter/chunk
relevant = regex_search(context, pattern)
sub_answer = llm_query(relevant + question)  # Recursive call on subset
```

The LLM has access to:
1. **Context variable** - the full input as a searchable string
2. **llm_query()** - recursive self-invocation on subsets
3. **Python REPL** - persistent state, print for reasoning

## Results

- Handles inputs **100x beyond context window**
- **3x cheaper** than summarization approaches (selective viewing)
- Outperforms RAG/retrieval on information-dense tasks
- 58% F1 on quadratic-complexity task where base models get 0.1%

## Alignment with Moss

| RLM Concept | Moss Equivalent | Notes |
|-------------|-----------------|-------|
| Prompt as environment variable | Files on disk | Queryable via `view`, `text-search` |
| Python REPL for state | Agent ephemeral context | `$(keep)` for persistence |
| `llm_query(chunk)` | `$(view path/Symbol)` | Targeted retrieval |
| Regex filtering | `text-search`, `grep` | Pre-filter before LLM |
| Recursive sub-calls | Task tool / sub-agents | Spawn specialized agents |
| `FINAL()` output tag | `$(answer ...)` | Signal completion |
| Chunking strategies | `--types-only`, depth limits | Control granularity |

### Validated Design Decisions

1. **Dynamic context over append-only** - RLM confirms "context rot" in full-ingestion. Moss's reshapeable context avoids this.

2. **Search as primitive** - "Search capability enables long-context handling" - justifies `text-search`, `view` as core tools.

3. **Selective viewing** - 3x cheaper than summarization. Justifies `--types-only`, symbol-specific views.

4. **Sub-agent decomposition** - "Sub-calls help with information-dense inputs" - justifies Explore agent pattern.

### Gaps in Moss

1. **No true recursive self-invocation** - Agent can spawn sub-agents but can't call itself with modified context. RLM's `llm_query()` is more fluid.

2. **No programmatic chunking** - Human picks what to view. RLM lets model decide chunk boundaries dynamically.

3. **No REPL state persistence** - Moss ephemeral context expires after 1 turn. RLM REPL persists variables across iterations.

4. **No explicit decomposition prompting** - RLM system prompt encourages "look through entire context before answering". Moss doesn't guide decomposition strategy.

## Implementation Ideas

### Recursive View Pattern
```lua
-- Agent could do this today, but not prompted to
function investigate(query, scope)
  local overview = view(scope, {types_only = true})
  local relevant = llm_decide_relevant(overview, query)
  for _, symbol in ipairs(relevant) do
    local detail = view(scope .. "/" .. symbol)
    -- Recurse if still too large
    if needs_decomposition(detail) then
      investigate(query, scope .. "/" .. symbol)
    end
  end
end
```

### Chunking Strategy
RLM models naturally discover chunking:
```python
# Model-generated code in RLM
chunk_size = len(context) // 10
for i in range(10):
    chunk = context[i*chunk_size:(i+1)*chunk_size]
    if keyword in chunk:
        answer = llm_query(f"Answer {query} given:\n{chunk}")
```

Moss could expose similar primitives:
- `view path --chunk N` - return Nth chunk of large file
- `view path --around "pattern"` - context around matches

### Cost Control
RLM notes high variance in costs due to recursion depth. Moss mitigations:
- Depth limits on sub-agent spawning
- Token budgets per investigation
- Early termination on diminishing returns

## Key Quotes

> "We convert long-context scaling from a parameter (bigger context window) into an inference-time algorithm."

> "The ability to search prompts enables handling long-context inputs; sub-calls help with information-dense inputs."

> "RLMs are up to 3× cheaper while maintaining stronger performance across all tasks because the model is able to selectively view context."

> "Current models are inefficient decision makers over their context" - room for better decomposition strategies.

## Limitations Noted

- Models without coding capability struggle
- Distinguishing "final answer" from intermediate thoughts is brittle
- Synchronous sub-calls limit parallelism
- High cost variance (95th percentile spikes)

## References

- Paper: https://arxiv.org/abs/2512.24601
- Authors: Alex L. Zhang, Tim Kraska (MIT), Omar Khattab (Databricks)
- Related: DSPy (Khattab), Learned Indexes (Kraska)
