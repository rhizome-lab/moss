# Execution Primitives Design

Work in progress. Designing composable execution architecture.

## Core Question

What are the minimal primitives that compose into workflows and agents?

## Current State (Problems)

- DWIMLoop: 1151 lines, bakes in TaskTree + EphemeralCache + specific retry logic
- AgentLoop: 2744 lines, generic step executor but confusingly named
- Workflows: TOML → AgentLoop, can't use DWIMLoop features
- No composition: can't mix strategies

## First Principles

### What does execution need?

1. **Steps** - units of work
2. **Context** - what the executor knows
3. **Control flow** - what happens next

### For "minimize LLM usage":

4. **Caching** - don't repeat, store + preview
5. **Tracking** - what's been seen/done

## Proposed Primitives

### Execution

| Primitive | Purpose |
|-----------|---------|
| `Step` | Single unit of work (tool call or LLM call) |
| `Sequence` | Run steps in order |
| `Loop` | Repeat until condition |
| `Branch` | Conditional execution |

### Context

| Primitive | Purpose |
|-----------|---------|
| `Scope` | Container for state, can nest |
| `Note` | Information with lifetime (expires after N turns, on done, etc.) |
| `History` | What happened (for LLM context building) |

### Context Strategies

| Strategy | Behavior |
|----------|----------|
| `TaskTree` | Hierarchical - path from root to current |
| `TaskList` | Flat - list of pending/done items |
| `Flat` | Just last N results |
| `None` | Stateless |

### Cache

| Primitive | Purpose |
|-----------|---------|
| `Store(content) → id` | Cache content, get ID |
| `Retrieve(id) → content` | Get cached content |
| `Preview(content) → (summary, id)` | Summarize + cache full |

### Cache Strategies

| Strategy | Behavior |
|----------|----------|
| `Ephemeral` | TTL-based, in-memory |
| `Persistent` | Disk-backed, survives restarts |
| `None` | No caching |

### Retry

| Primitive | Purpose |
|-----------|---------|
| `Policy` | How to retry (exponential, fixed, immediate) |
| `Limit` | Max attempts |
| `Fallback` | What to do when exhausted |

## Composition

A Step combines these:

```
Step {
    action: ToolCall | LLMCall
    context: ContextStrategy
    cache: CacheStrategy
    retry: RetryPolicy
}
```

Nesting:

```
Scope(context=TaskTree) {
    Step("analyze")

    Scope(context=TaskList) {  // nested, different strategy
        Step("fix issue 1")
        Step("fix issue 2")
    }

    Step("verify")  // back to parent scope
}
```

## Open Questions

1. **Inheritance** - Does nested scope inherit parent's cache/retry? Override? Merge?

2. **Representation** - Code? TOML? DSL?
   - Simple cases: TOML
   - Complex nesting: Code or DSL

3. **LLM in the loop** - How does "agent" fit?
   - Agent = Loop where LLM decides next Step
   - Uses same primitives, just dynamic control flow

4. **DWIM** - Where does intent parsing fit?
   - Just a function: `parse_intent(text) → Step`
   - Not a primitive, uses primitives

## Agent as Composition

An "agent" is not a special thing. It's just:

```
Agent = Loop(until="done") {
    Step(action=LLM) → output
    parse_intent(output) → next_step
    Step(action=next_step)
}
```

With context strategy (TaskTree), cache strategy (Ephemeral), retry (exponential).

DWIM is just a function:
```python
def parse_intent(text: str) -> Step:
    """Parse 'view foo.py' into Step(action=view, target='foo.py')"""
```

Not a primitive. Uses primitives.

## Representation Options

### Simple: TOML
```toml
[[steps]]
name = "analyze"
action = "view ."

[[steps]]
name = "fix"
action = "edit main.py 'add logging'"
```

### Complex nesting: Python
```python
with Scope(TaskTree):
    run("analyze")
    with Scope(TaskList):
        for issue in issues:
            run(f"fix {issue}")
```

### Middle ground: DSL
```
scope TaskTree:
    analyze
    scope TaskList:
        fix issue1
        fix issue2
    verify
```

**Recommendation:** Start with Python (it's already there), add TOML for simple cases.

## Prototype Status

Implemented in `src/moss/execution/__init__.py` (~200 lines):

- [x] Scope with pluggable ContextStrategy
- [x] Context strategies: FlatContext, TaskListContext
- [x] Cache strategies: NoCache, InMemoryCache
- [x] Nested scopes with different strategies
- [x] Basic agent_loop using primitives

Works:
```python
with Scope(context=FlatContext()) as outer:
    outer.run('analyze')
    with outer.child(context=TaskListContext()) as inner:
        # Different strategy for sub-tasks
        inner.context.add('task', 'fix issue 1')
    outer.run('verify')
```

## Next Steps

- [ ] Wire up real tool execution (rust_shim.passthrough)
- [ ] Wire up real LLM calls
- [ ] Add parse_intent() as simple function
- [ ] Add TaskTree strategy (hierarchical)
- [ ] Test with real tasks
- [ ] Compare to DWIMLoop output
