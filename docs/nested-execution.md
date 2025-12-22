# Nested Execution Design

Designing how compound steps, nested scopes, and multi-agent communication work together.

## Problem Statement

We have:
- WorkflowStep with optional sub-steps (compound steps)
- Scope with pluggable strategies (context, cache, retry)
- TaskTreeContext that tracks hierarchical task state

Questions:
1. Should execution structure mirror TaskTree structure?
2. How do child agents communicate results back to parents?
3. When should sub-steps share vs isolate context?

## Current Implementation

```python
@dataclass
class WorkflowStep:
    name: str
    action: str | None = None      # Simple step
    steps: list[WorkflowStep] | None = None  # Compound step
    on_error: str = "fail"
    max_retries: int = 1

def _run_steps(scope: Scope, steps: list[WorkflowStep]) -> bool:
    for step in steps:
        if step.steps:
            # Compound: child scope, recurse
            with scope.child() as child_scope:
                _run_steps(child_scope, step.steps)
        elif step.action:
            # Simple: execute in current scope
            scope.run(step.action)
```

Child scope inherits parent strategies but gets fresh context via `context.child()`.

## Design Space

### A. Execution-Context Correspondence

Should each compound step become a node in TaskTree?

**Option 1: Implicit correspondence** (current)
- Compound step creates child scope
- Child scope gets `context.child()` which for TaskTree creates a child node
- Structure emerges naturally

**Option 2: Explicit correspondence**
- Step declares its TaskTree node explicitly
- More control but more configuration

**Consideration:** What about TaskList? TaskList is flat - no hierarchy to mirror. This suggests:
- Context strategy determines feedback semantics, not step structure
- Compound steps with TaskList just run sub-steps in sequence, no tree structure
- Only TaskTree creates hierarchical correspondence

**LLM management of TaskTree:** How does an LLM interact with tree structure?
- Option A: LLM sees tree as indented text, manipulates via natural language ("add subtask under X")
- Option B: LLM gets structured commands (add_child, complete, move_to)
- Option C: LLM only sees current path (breadcrumb), system manages tree structure

Option C is simplest - LLM doesn't need to understand tree, just current context.

### B. Context Sharing Modes

When should sub-steps share parent context vs get isolated context?

| Mode | Behavior | Use Case |
|------|----------|----------|
| `isolated` | Child gets fresh context via `.child()` | Independent sub-tasks |
| `shared` | Child uses same context object | Continuation of same task |
| `inherited` | Child sees parent context, parent doesn't see child | One-way visibility |

**Current:** Always `isolated` (child gets `.child()`).

**Question:** Do we need the other modes?

For TaskTree specifically:
- `isolated` = sub-steps create subtree under compound step
- `shared` = sub-steps add to same node as parent (flat)
- `inherited` = sub-steps create subtree, but results don't propagate up

### C. Child-to-Parent Feedback

How does a child agent communicate results back to parent?

**Option 1: Return value (string)**
```python
def _run_steps(scope, steps) -> str:
    # Returns summary string
    return "completed 3 tasks, 1 skipped"
```
Simple, sufficient for most cases.

**Option 2: Structured return**
```python
@dataclass
class StepResult:
    success: bool
    summary: str
    tokens_used: int = 0
    files_modified: list[str] = field(default_factory=list)
    confidence: float = 1.0
```
More info but more complexity. Defer unless needed.

**Option 3: Context-based (TaskTree)**
```python
# TaskTree already tracks this:
class TaskTreeContext:
    task: str = ""      # What child was asked to do
    result: str = ""    # What child produced
    children: list      # Child nodes
```
Parent can inspect `child_ctx.result` after child scope exits.

**Decision:** Start with Option 1 (string). Parent gets full context access but only on explicit request - don't automatically inject child context into parent. Option 3 already works for TaskTree.

**Parent access pattern:**
- Full child context available but not given unless asked
- Parent can call `child_scope.context.get_context()` if needed
- Default: just get success/fail + summary string

### D. Multi-Agent Patterns

Different patterns for agent composition:

**1. Sequential delegation**
```
Parent → Child A → Child B → Parent
```
Parent waits for each child. Current `_run_steps` does this.

**2. Parallel delegation**
```
Parent → [Child A, Child B, Child C] → Parent
```
Parent spawns multiple children, waits for all.

**3. Hierarchical delegation**
```
Parent → Child A → [Grandchild 1, Grandchild 2]
       → Child B
```
Children can spawn their own children. TaskTree naturally supports this.

**4. Feedback loop**
```
Parent → Child → Parent (reviews) → Child (refines) → ...
```
Iterative refinement between parent and child.

## Proposed Design

### Step-Level Configuration

```python
@dataclass
class WorkflowStep:
    name: str
    action: str | None = None
    steps: list[WorkflowStep] | None = None

    # Execution control
    on_error: str = "fail"
    max_retries: int = 1

    # Context control (for compound steps)
    context_mode: str = "isolated"  # isolated, shared, inherited

    # Feedback control
    summarize: bool = False  # Summarize child results for parent
```

### Scope Changes

```python
@dataclass
class Scope:
    context: ContextStrategy
    cache: CacheStrategy
    retry: RetryStrategy
    parent: Scope | None = None

    # New: child results accessible to parent
    child_results: list[str] = field(default_factory=list)

    def child(self, mode: str = "isolated") -> Scope:
        if mode == "shared":
            return Scope(
                context=self.context,  # Same context
                cache=self.cache,
                retry=self.retry,
                parent=self,
            )
        elif mode == "inherited":
            # Child sees parent context (read), writes to own
            return Scope(
                context=InheritedContext(self.context),
                cache=self.cache,
                retry=self.retry,
                parent=self,
            )
        else:  # isolated
            return Scope(
                context=self.context.child(),
                cache=self.cache,
                retry=self.retry,
                parent=self,
            )
```

### TaskTree as Communication Bus

Since TaskTreeContext already tracks:
- `task`: what the node was asked to do
- `result`: what the node produced
- `children`: child nodes

Parent can inspect children after compound step completes:

```python
def _run_steps(scope, steps):
    for step in steps:
        if step.steps:
            with scope.child() as child_scope:
                _run_steps(child_scope, step.steps)

                # Parent can now access child results
                if isinstance(child_scope.context, TaskTreeContext):
                    for child_node in child_scope.context.children:
                        # child_node.task, child_node.result available
                        pass

                # Optionally summarize for parent context
                if step.summarize:
                    summary = summarize_children(child_scope.context)
                    scope.context.add("child_summary", summary)
```

## Open Questions

1. **TOML representation**: ~~How to express `context_mode` and `summarize` in TOML workflows?~~
   IMPLEMENTED: Both are parsed from step config in load_workflow().

2. **Parallel steps**: ~~Should compound steps support parallel execution of sub-steps?~~
   IMPLEMENTED: State machine has `parallel` and `join` fields for fork/join patterns.

3. **Conditional steps**: Should sub-steps support conditions?
   ```python
   condition: str | None = None  # e.g., "result contains 'error'"
   ```

4. **Break/continue**: Should sub-steps support early exit from compound step?

## Implementation Plan

Phase 1 (current): Basic nested steps
- [x] WorkflowStep with optional `steps` field
- [x] Recursive `_run_steps` with child scopes
- [x] TOML parsing for nested steps

Phase 2: Context modes
- [x] Add `context_mode` to WorkflowStep
- [x] Implement shared/inherited modes in Scope.child()
- [x] Document when to use each mode (see WorkflowStep docstring)

Phase 3: Feedback
- [x] Add `summarize` option to compound steps
- [x] Expose child results to parent (StepResult.child_results)
- [x] Add summary generation for TaskTree children (_summarize_children)

Phase 4: Advanced patterns
- [ ] Parallel step execution
- [ ] Conditional steps
- [ ] Break/continue semantics
