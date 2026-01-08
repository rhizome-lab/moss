# Quality Audit Workflow

Finding code quality issues: complexity, duplication, maintainability problems.

## Trigger

- New codebase assessment
- Regular health check
- Pre-refactoring analysis
- Technical debt inventory

## Goal

- Identify quality issues
- Prioritize by impact
- Provide actionable recommendations
- Track improvements over time

## Prerequisites

- Access to codebase
- Understanding of quality criteria
- Ability to build/run code (for some checks)

## Decomposition Strategy

**Measure → Analyze → Prioritize → Report**

```
1. MEASURE: Gather metrics
   - Complexity metrics
   - Code duplication
   - Test coverage
   - Dependency health
   - Documentation coverage

2. ANALYZE: Interpret findings
   - What's concerning?
   - What's normal for this context?
   - What patterns emerge?
   - What's getting worse?

3. PRIORITIZE: Rank issues
   - Impact (how bad is it?)
   - Effort (how hard to fix?)
   - Risk (what could go wrong?)
   - Dependencies (what blocks what?)

4. REPORT: Document findings
   - Executive summary
   - Detailed findings
   - Recommendations
   - Metrics baseline for future
```

## Tools Used

| Phase | Tools |
|-------|-------|
| Measure | `moss analyze`, coverage tools, linters |
| Analyze | Manual review, `view` |
| Prioritize | Judgment, stakeholder input |
| Report | Document findings |

## Quality Dimensions

### Complexity
```bash
# Find complex functions
moss analyze complexity --threshold 15

# Find long functions
moss analyze length --threshold 100
```

### Duplication
```bash
# Find duplicate functions
moss analyze duplicate-functions

# Find similar type definitions
moss analyze duplicate-types
```

### Maintainability
```bash
# Overall health check
moss analyze health

# Full analysis with grades
moss analyze all
```

### Testing
```bash
# Check test coverage
moss analyze test-coverage

# Find untested code paths
moss analyze untested
```

### Documentation
```bash
# Check doc coverage
moss analyze docs

# Find undocumented public APIs
moss view src --types-only --undocumented
```

## Quality Metrics

| Metric | Good | Concerning | Critical |
|--------|------|------------|----------|
| Cyclomatic complexity | <10 | 10-20 | >20 |
| Function length | <50 | 50-100 | >100 |
| File length | <500 | 500-1000 | >1000 |
| Test coverage | >80% | 50-80% | <50% |
| Duplicate blocks | <5% | 5-10% | >10% |

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| False positives | Good code flagged | Adjust thresholds, add allowlist |
| Missing context | Metrics without meaning | Understand the domain |
| Analysis paralysis | Too many issues | Focus on high-impact items |
| Metric gaming | Numbers improve but code doesn't | Look at actual code quality |

## Example Session

**Goal**: Audit quality of a mid-size codebase

```
Turn 1: Run comprehensive analysis
  $(moss analyze all)
  → Overall grade: C
  → Complexity: B
  → Duplication: D
  → Coverage: C

Turn 2: Investigate duplication
  $(moss analyze duplicate-functions)
  → 12 duplicate function pairs
  → 3 clusters of similar validation logic

Turn 3: Find worst complexity offenders
  $(moss analyze complexity --threshold 20)
  → src/parser.rs:parse_expression (CC: 45)
  → src/handlers.rs:handle_request (CC: 32)
  → src/validator.rs:validate (CC: 28)

Turn 4: Check test coverage for complex code
  $(moss analyze coverage path:src/parser.rs)
  → parse_expression: 23% coverage
  → High complexity + low coverage = high risk

Turn 5: Look for patterns
  → Validation logic duplicated because no shared lib
  → Parser complexity from inline error handling
  → Low coverage in legacy code

Turn 6: Prioritize findings
  High priority:
  - Extract shared validation library (effort: M, impact: H)
  - Add tests for parse_expression (effort: M, impact: H)

  Medium priority:
  - Refactor parse_expression (effort: H, impact: M)
  - Improve overall coverage (effort: H, impact: M)

  Low priority:
  - Minor duplication in tests (effort: L, impact: L)
```

## Audit Report Structure

```markdown
# Quality Audit Report

## Executive Summary
- Overall grade: X
- Key findings: A, B, C
- Top recommendations: 1, 2, 3

## Metrics Dashboard
| Metric | Value | Target | Status |
|--------|-------|--------|--------|
| ... | ... | ... | ... |

## Detailed Findings

### Finding 1: [Title]
- **Severity**: High/Medium/Low
- **Location**: [files/areas]
- **Impact**: [what's the problem]
- **Recommendation**: [how to fix]

## Action Items
1. [Prioritized list of improvements]

## Appendix
- Raw metrics data
- Tool configuration
- Methodology notes
```

## Variations

### Security Audit
Focus on security-specific patterns (see [Security Audit](security-audit.md)).

### Performance Audit
Focus on performance patterns, profiling data.

### Pre-Acquisition Audit
Comprehensive assessment for M&A due diligence.

### Regular Health Check
Lighter audit, track trends over time.

## Anti-patterns

- **Metrics without action**: Measuring but never improving
- **One-time audit**: No follow-up or trend tracking
- **Comparing unlike things**: Different codebases have different contexts
- **Ignoring trends**: Absolute numbers matter less than direction

## See Also

- [Security Audit](security-audit.md) - Security-focused audit
- [Tech Debt](tech-debt.md) - Addressing accumulated issues
- [Refactoring](refactoring.md) - Fixing quality issues
