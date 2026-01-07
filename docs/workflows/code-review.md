# Code Review Workflow

Reviewing code changes: PR review, commit review, pre-merge checks.

## Trigger

- Pull request opened/updated
- Code submitted for review
- Pre-commit/pre-merge hook

## Goal

- Identify issues before merge
- Ensure code meets quality bar
- Knowledge transfer (reviewer learns, author learns)
- Documented decision trail

## Prerequisites

- Diff available (PR, commit, local changes)
- Context accessible (base branch, related code)
- Review criteria defined (style, correctness, security)

## Decomposition Strategy

**Understand → Examine → Verify → Respond**

```
1. UNDERSTAND CONTEXT
   - What is this change trying to do?
   - Read PR description, linked issues
   - Understand the "why" before the "what"

2. EXAMINE CHANGES
   - For each changed file:
     a. What was changed? (diff)
     b. Does the change make sense? (logic)
     c. Are edge cases handled? (robustness)
     d. Is it consistent with codebase? (style)

3. VERIFY BEHAVIOR
   - Do tests cover the changes?
   - Do tests pass?
   - Any new warnings/lints?
   - Performance implications?

4. CHECK CROSS-CUTTING CONCERNS
   - Security implications
   - API compatibility
   - Documentation updates needed
   - Migration/rollback plan

5. RESPOND
   - Categorize feedback: blocking vs. nit vs. question
   - Be specific: file:line + suggestion
   - Acknowledge good work
```

## Tools Used

| Phase | Tools |
|-------|-------|
| Understand | `gh pr view`, linked issues, `view` for context |
| Examine | `--diff` flag, `view` changed files |
| Verify | `run` tests, `analyze` (lint, complexity) |
| Cross-cutting | `analyze security`, `text-search` for related code |
| Respond | PR comments, structured feedback |

## Review Checklist

### Correctness
- [ ] Logic is sound
- [ ] Edge cases handled
- [ ] Error handling appropriate
- [ ] No obvious bugs

### Design
- [ ] Fits codebase architecture
- [ ] Abstractions make sense
- [ ] Not over-engineered
- [ ] Not under-engineered

### Maintainability
- [ ] Code is readable
- [ ] Names are clear
- [ ] Comments where needed (not obvious things)
- [ ] No dead code

### Testing
- [ ] Tests exist for new code
- [ ] Tests cover edge cases
- [ ] Tests are maintainable (not brittle)

### Security
- [ ] No hardcoded secrets
- [ ] Input validation present
- [ ] No injection vulnerabilities
- [ ] Permissions checked

### Performance
- [ ] No obvious N+1 queries
- [ ] No unnecessary allocations
- [ ] Appropriate data structures

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| Missing context | Don't understand why change was made | Ask author, read linked issues |
| Rubber stamping | No substantive feedback | Slow down, use checklist |
| Bikeshedding | Feedback on trivial issues only | Focus on correctness/design first |
| Blocking on style | Style issues marked as blocking | Use linter, mark nits as nits |
| Missing the bug | Bug found post-merge | Retrospective, improve checklist |

## Example Session

**PR**: "Add caching to user lookup"

```
Turn 1: Understand context
  $(gh pr view 123)
  → "Adds Redis cache for user lookups to reduce DB load"

Turn 2: View the diff
  $(view src/users.rs --diff main)
  → +cache_get(), +cache_set(), modified get_user()

Turn 3: Examine cache logic
  $(view src/users.rs/get_user)
  → Cache miss → DB lookup → Cache set → Return
  Issue: No TTL on cache entries

Turn 4: Check for cache invalidation
  $(text-search "cache" --only "src/users.rs")
  → No invalidation on user update

Turn 5: Verify tests
  $(view src/users_test.rs --diff main)
  → Tests added for cache hit/miss, but not invalidation

Turn 6: Check security
  $(analyze security src/users.rs)
  → No issues found

Turn 7: Respond
  Comments:
  - BLOCKING: No cache invalidation on user update (data staleness)
  - BLOCKING: No TTL on cache entries (memory growth)
  - nit: Consider extracting cache logic to separate module
  - nice: Good test coverage for happy path
```

## Variations

### Self-Review (before opening PR)
Same checklist but you're both author and reviewer. Harder to catch own mistakes - use `analyze` heavily.

### Security-Focused Review
Prioritize security checklist, use `analyze security`, think like an attacker.

### Architecture Review
Focus on design, ignore implementation details. Does this fit the system?

### Quick Review (small change)
Abbreviated process for typo fixes, doc updates. Still verify tests pass.

## Feedback Categories

| Category | Meaning | Action Required |
|----------|---------|-----------------|
| BLOCKING | Must fix before merge | Author fixes |
| SHOULD | Strong suggestion | Author considers |
| NIT | Minor, optional | Author's discretion |
| QUESTION | Need clarification | Author responds |
| PRAISE | Good work | None (morale boost) |

## Anti-patterns

- **Drive-by approvals**: LGTM without reading
- **Gatekeeper syndrome**: Blocking on personal preference
- **Delayed reviews**: Sitting in queue for days
- **Review by committee**: Too many reviewers, conflicting feedback
- **No positive feedback**: Only pointing out problems
