# Refactoring Workflow

Improving code without changing behavior: restructuring, renaming, simplifying.

## Trigger

- Code is hard to understand
- Adding feature requires cleanup first
- Quality audit identified issues
- Duplication needs consolidation

## Goal

- Improve code structure/readability
- Preserve all existing behavior
- Enable future changes
- Tests pass before and after

## Prerequisites

- Tests exist and pass
- Clear understanding of current behavior
- Permission to modify
- No concurrent feature work (ideally)

## Decomposition Strategy

**Understand → Plan → Execute → Verify**

```
1. UNDERSTAND: Know the code deeply
   - What does it do?
   - What are the edge cases?
   - What depends on it?
   - What are the invariants?

2. PLAN: Design the target state
   - What should it look like after?
   - What steps get us there?
   - What can go wrong?
   - How to verify each step?

3. EXECUTE: Small, verified steps
   - Make one change
   - Run tests
   - Commit
   - Repeat

4. VERIFY: Confirm behavior preserved
   - All tests pass
   - Manual verification
   - Performance unchanged
   - No new warnings
```

## Tools Used

| Phase | Tools |
|-------|-------|
| Understand | `view`, `analyze callers/callees`, `text-search` |
| Plan | Document target state |
| Execute | `edit`, IDE refactoring |
| Verify | Test suite, `moss analyze` |

## Refactoring Catalog

### Extract Function
```
Before: Long function with embedded logic
After: Smaller function + extracted helper
```

### Inline Function
```
Before: Function that adds no abstraction
After: Code inlined at call sites
```

### Rename Symbol
```
Before: Unclear name (e.g., `tmp`, `data`)
After: Descriptive name (e.g., `connection`, `user_preferences`)
```

### Extract Variable
```
Before: Complex expression inline
After: Named variable with clear meaning
```

### Move Function
```
Before: Function in wrong module
After: Function in appropriate module
```

### Extract Module
```
Before: Large file with multiple concerns
After: Smaller files with single responsibility
```

### Replace Conditional with Polymorphism
```
Before: Switch/match on type
After: Trait/interface with implementations
```

### Consolidate Duplicate Code
```
Before: Same logic in multiple places
After: Single function, called from all places
```

## Safe Refactoring Process

```
1. Ensure tests exist for code being refactored
2. Make the smallest possible change
3. Run tests immediately
4. Commit if tests pass
5. If tests fail, revert and try smaller step
6. Repeat until done
```

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| Behavior change | Tests fail | Revert, try smaller step |
| Missing tests | Confident but wrong | Add tests before refactoring |
| Too many changes | Hard to debug failures | Revert to last good, go smaller |
| Performance regression | Slower after refactor | Profile, optimize or reconsider |

## Example Session

**Goal**: Extract duplicate validation logic

```
Turn 1: Identify duplication
  $(moss analyze duplicate-functions)
  → validate_email in users.rs and contacts.rs
  → validate_phone in users.rs and orders.rs

Turn 2: Understand both versions
  $(view src/users.rs/validate_email)
  $(view src/contacts.rs/validate_email)
  → Nearly identical, minor formatting differences

Turn 3: Plan extraction
  → Create src/validation/mod.rs
  → Move validate_email there
  → Update callers

Turn 4: Create validation module
  $(edit src/validation/mod.rs)
  → Add validate_email function
  → Add tests

Turn 5: Run tests
  $(cargo test)
  → Pass (new function, nothing uses it yet)
  $(git commit)

Turn 6: Update first caller
  $(edit src/users.rs)
  → Replace local function with use validation::validate_email
  $(cargo test)
  → Pass
  $(git commit)

Turn 7: Update second caller
  $(edit src/contacts.rs)
  → Same change
  $(cargo test)
  → Pass
  $(git commit)

Turn 8: Verify no duplication
  $(moss analyze duplicate-functions)
  → validate_email no longer duplicated
```

## Refactoring vs. Rewriting

### Refactor When:
- Behavior is well-understood
- Tests exist
- Structure is salvageable
- Changes are incremental

### Rewrite When:
- Requirements have changed completely
- Architecture is fundamentally wrong
- No tests, behavior unclear
- Incremental change impossible

## Anti-patterns

- **Big-bang refactoring**: Changing everything at once
- **Refactoring without tests**: No way to verify behavior
- **Mixing with feature work**: Hard to isolate bugs
- **Premature abstraction**: Abstracting before understanding patterns
- **Cosmetic refactoring**: Changes that don't improve anything

## Metrics

Track before/after:
- Cyclomatic complexity
- Duplication percentage
- Lines of code (less isn't always better)
- Test coverage
- Time to understand (subjective)

## See Also

- [Quality Audit](quality-audit.md) - Finding what to refactor
- [Dependency Tracing](dependency-tracing.md) - Understanding impact
- [Tech Debt](tech-debt.md) - Managing accumulated issues
