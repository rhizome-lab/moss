# Tech Debt Workflow

Managing accumulated technical debt: tracking, prioritizing, paying down.

## Trigger

- Quality audit revealed issues
- Development velocity slowing
- Bugs increasing in specific areas
- Regular tech debt review

## Goal

- Identify and catalog tech debt
- Prioritize by impact and effort
- Systematically pay down debt
- Prevent new debt accumulation

## Prerequisites

- Understanding of codebase health
- Time allocated for debt work
- Stakeholder buy-in

## Decomposition Strategy

**Inventory → Assess → Prioritize → Address**

```
1. INVENTORY: Find all tech debt
   - Code quality issues
   - Outdated dependencies
   - Missing tests
   - Documentation gaps
   - Known shortcuts/workarounds

2. ASSESS: Evaluate each item
   - Impact (what's the cost of not fixing?)
   - Effort (how hard to fix?)
   - Risk (what could go wrong?)
   - Dependencies (what blocks/enables?)

3. PRIORITIZE: Decide what to fix
   - Quick wins (low effort, high impact)
   - Strategic investments (high effort, high impact)
   - Defer (low impact)
   - Accept (not worth fixing)

4. ADDRESS: Fix systematically
   - Allocate time each sprint
   - Track progress
   - Verify improvements
   - Update inventory
```

## Tools Used

| Phase | Tools |
|-------|-------|
| Inventory | `moss analyze all`, `grep TODO/FIXME/HACK` |
| Assess | `view`, `analyze complexity/coverage` |
| Prioritize | Judgment, stakeholder input |
| Address | Normal development workflow |

## Debt Categories

### Code Debt
- Complex functions
- Duplicate code
- Poor naming
- Missing abstractions

### Test Debt
- Low coverage
- Flaky tests
- Slow tests
- Missing integration tests

### Dependency Debt
- Outdated packages
- Security vulnerabilities
- Deprecated APIs
- Missing updates

### Documentation Debt
- Stale docs
- Missing docs
- Undocumented decisions
- Broken examples

### Architecture Debt
- Wrong patterns
- Misplaced code
- Missing boundaries
- Circular dependencies

## Debt Tracking

```markdown
# Tech Debt Register

| ID | Description | Impact | Effort | Priority | Status |
|----|-------------|--------|--------|----------|--------|
| TD-001 | validate_email duplicated in 3 files | M | L | Quick Win | TODO |
| TD-002 | No tests for payment module | H | H | Strategic | In Progress |
| TD-003 | Using deprecated serde API | L | L | Backlog | TODO |
```

## Prioritization Matrix

```
                    Low Effort    High Effort
                  ┌─────────────┬─────────────┐
    High Impact   │ QUICK WINS  │ STRATEGIC   │
                  │ Do now      │ Plan for    │
                  ├─────────────┼─────────────┤
    Low Impact    │ FILL-INS    │ DON'T DO    │
                  │ When time   │ Not worth   │
                  └─────────────┴─────────────┘
```

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| Debt ignored | Velocity drops | Schedule debt time |
| All debt all the time | No features delivered | Balance with features |
| Wrong prioritization | Easy stuff done, hard ignored | Re-assess impact |
| Debt grows faster | More items than resolved | Address root causes |

## Example Session

**Goal**: Assess and prioritize tech debt

```
Turn 1: Run quality analysis
  $(moss analyze all)
  → Grade: C
  → 15 complexity warnings
  → 8 duplicate code blocks
  → 45% test coverage

Turn 2: Find TODO/FIXME markers
  $(text-search "TODO|FIXME|HACK|XXX" --only "*.rs")
  → 23 TODO markers
  → 8 FIXME markers
  → 4 HACK markers

Turn 3: Assess complexity hotspots
  $(moss analyze complexity --threshold 15)
  → parse_request: CC 32, modified 15 times this year
  → High complexity + frequent changes = high priority

Turn 4: Assess test debt
  $(moss analyze coverage)
  → src/payments/: 12% coverage
  → Handles money = high risk = high priority

Turn 5: Build inventory
  TD-001: Split parse_request (Impact: H, Effort: M)
  TD-002: Add payment tests (Impact: H, Effort: H)
  TD-003: Remove duplicate validators (Impact: M, Effort: L)
  TD-004: Update deprecated API (Impact: L, Effort: L)

Turn 6: Prioritize
  Quick wins: TD-003, TD-004
  Strategic: TD-001, TD-002
  Action: Do quick wins this sprint, schedule strategic for next
```

## Debt Budget

Allocate consistent time for debt:
- **20% rule**: 1 day per week, 1 sprint per quarter
- **Boy scout rule**: Leave code better than you found it
- **Debt sprints**: Occasional focused debt reduction
- **Zero-bug policy**: Fix bugs immediately, prevent debt

## Prevention

### At Code Review
- Flag shortcuts being introduced
- Require tests for new code
- Enforce style/quality gates

### At Architecture
- Design for change
- Document decisions
- Avoid premature optimization

### At Process
- Regular debt reviews
- Track debt metrics
- Celebrate debt paydown

## Anti-patterns

- **Ignoring debt**: "We'll fix it later" (never happens)
- **Debt bankruptcy**: So much debt it's overwhelming
- **Wrong balance**: All debt or all features, not both
- **Invisible debt**: Not tracking, so can't prioritize
- **Perfectionism**: Treating all debt as critical

## Metrics

Track over time:
- Total debt items
- Debt added vs. resolved per sprint
- Mean time to address debt
- Complexity/coverage trends
- Developer satisfaction survey

## See Also

- [Quality Audit](quality-audit.md) - Finding debt
- [Refactoring](refactoring.md) - Paying down debt
- [Test Coverage](test-coverage.md) - Addressing test debt
