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

## TOML vs Code: What Can Each Express?

### TOML works for: Sequences and State Machines

```toml
[workflow]
name = "validate-fix"
context = "flat"
cache = "ephemeral"

# Linear sequence
[[workflow.steps]]
action = "analyze --health"

[[workflow.steps]]
action = "edit {file} 'fix issues'"

[[workflow.steps]]
action = "analyze --health"  # verify
```

State machines are also expressible:

```toml
[[states]]
name = "analyzing"
action = "analyze --health"

[[states.transitions]]
condition = "has_errors"
next = "fixing"

[[states.transitions]]
condition = "success"
next = "done"

[[states]]
name = "fixing"
action = "edit {file} 'fix issues'"

[[states.transitions]]
next = "analyzing"  # always loop back
```

### TOML awkward for: Nested scopes

```toml
[[workflow.steps]]
action = "analyze"

[[workflow.steps]]
type = "scope"
context = "task_list"  # different strategy

[[workflow.steps.scope.steps]]  # deeply nested, ugly
action = "fix issue 1"
```

### TOML can't express: Computed values

```python
# Agent: LLM decides next step
while not done:
    action = llm.decide(context)  # Can't put LLM call in TOML
    result = run(action)

# Dynamic iteration
for issue in find_issues():  # Result of function call
    run(f"fix {issue}")

# Computed conditions
if len(errors) > threshold:  # Python expression
    run("escalate")
```

### Potential: Inline Python in TOML?

```toml
[[workflow.steps]]
action = "analyze"
condition = "python:len(context.errors) > 0"  # Embedded expression

[[workflow.steps]]
for_each = "python:find_issues()"  # Iterator expression
action = "fix {item}"
```

This bridges the gap but adds complexity. Evaluate whether simpler
"just use Python" is better than hybrid TOML+Python.

### Conclusion

| Use Case | Representation |
|----------|----------------|
| Linear recipe | TOML |
| State machine | TOML |
| Nested scopes | TOML (verbose) or Python |
| Computed values/LLM | Python (or TOML+inline Python) |

**Key insight:** The dividing line is computed values, not control flow.
TOML can express arbitrary static control flow (including state machines).
Python is needed when values are computed at runtime.

## Prototype Status

Implemented in `src/moss/execution/__init__.py` (~300 lines):

- [x] Scope with pluggable ContextStrategy
- [x] Context strategies: FlatContext, TaskListContext, TaskTreeContext
- [x] Cache strategies: NoCache, InMemoryCache
- [x] Nested scopes with different strategies
- [x] Basic agent_loop using primitives
- [x] parse_intent() - DWIM verb parsing (~20 lines)
- [x] execute_intent() - routes to rust_shim.passthrough()
- [x] Scope.run() wired to real execution

Works:
```python
# Nested TaskTree context
with Scope(context=TaskTreeContext()) as outer:
    outer.context.add('task', 'fix all issues')
    with outer.child() as inner:
        inner.context.add('task', 'fix type errors')
        # Context shows hierarchical path:
        # fix all issues
        #   └── fix type errors

# Intent parsing
parse_intent('view main.py')  # → Intent(verb='view', target='main.py')
parse_intent('fix foo.py')    # → Intent(verb='edit', target='foo.py')
```

## Next Steps

- [ ] Wire up real LLM calls
- [ ] Test with real tasks end-to-end
- [ ] Compare to DWIMLoop output
- [ ] Replace DWIMLoop with new primitives
