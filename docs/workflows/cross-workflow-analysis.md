# Cross-Workflow Analysis

Patterns, principles, and insights extracted from analyzing all documented workflows.

## Universal Workflow Structure

Almost every workflow follows a four-phase pattern:

```
┌─────────────────┐
│    SCOPE/       │  Define boundaries, understand context
│    TRIAGE       │  What are we working with?
└────────┬────────┘
         ▼
┌─────────────────┐
│    ANALYZE/     │  Gather information, identify targets
│    SURVEY       │  What needs attention?
└────────┬────────┘
         ▼
┌─────────────────┐
│    ACT/         │  Do the work
│    IMPLEMENT    │  Execute the core task
└────────┬────────┘
         ▼
┌─────────────────┐
│    VERIFY/      │  Confirm success
│    DOCUMENT     │  Record what was done
└─────────────────┘
```

### Workflow-Specific Names

| Workflow | Phase 1 | Phase 2 | Phase 3 | Phase 4 |
|----------|---------|---------|---------|---------|
| Bug Fix | Reproduce | Locate | Fix | Verify |
| Security Audit | Scope | Survey | Deep Dive | Report |
| Code Review | Context | Read | Feedback | Conclude |
| Feature Impl | Understand | Design | Implement | Verify |
| RE Binary | Collect | Analyze | Synthesize | Verify |
| Malware | Triage | Static | Dynamic | Document |
| Steganography | Triage | Analyze | Extract | Verify |
| Code Synthesis | Collect | Generate | Verify (D×C) | Iterate |

The names change, the structure doesn't.

## Principle 1: Triage Before Dive

Every workflow starts with understanding what you're working with:

| Workflow | Triage Step |
|----------|-------------|
| Bug Fix | Reproduce the bug first |
| Security Audit | Define threat model and scope |
| Performance | Measure baseline, identify if actually slow |
| RE Binary | `file`, `xxd`, magic bytes before disassembly |
| Malware | Safe environment, file identification |
| Code Review | Understand PR context and goals |
| Steganography | File type, size, metadata before analysis |

**Why**: Prevents wasted effort on wrong target, establishes baseline, identifies constraints.

## Principle 2: Multiple Evidence Sources

No workflow trusts a single source of information:

| Workflow | Multiple Sources |
|----------|-----------------|
| Performance Regression | git bisect + profiler + traces + benchmarks |
| Flaky Tests | Logs + race detectors + CI patterns + timing analysis |
| Production Debugging | Logs + metrics + traces + recent changes |
| Security Audit | Automated scanners + manual review + threat model |
| Dead Code | Static analysis + runtime tracing + coverage |
| Bug Investigation | Reproduction + logs + debugger + git history |

**Why**: Each source has blind spots. Cross-referencing catches what individual sources miss.

## Principle 3: Hypothesis-Driven Iteration

Most workflows follow a hypothesis-test-refine loop:

```
Form hypothesis → Test hypothesis → Refine or reject → Repeat
```

| Workflow | Hypothesis Pattern |
|----------|-------------------|
| RE Binary | "This looks like a header" → parse → validate |
| Bug Fix | "The bug is in X" → add logging → confirm/reject |
| Performance | "Y is the bottleneck" → profile → measure |
| Flaky Test | "Race condition in Z" → add synchronization → rerun |
| Cryptanalysis | "Using weak RNG" → check entropy → verify |

**Why**: Directed search beats exhaustive search. Wrong hypotheses still provide information.

## Principle 4: Coarse-to-Fine Refinement

Start broad, zoom in on areas of interest:

| Workflow | Coarse → Fine |
|----------|---------------|
| Code Review | Architecture → modules → functions → lines |
| Security Audit | Attack surface → high-risk areas → specific vulns |
| RE Binary | File structure → sections → fields → bytes |
| Performance | System → service → function → line |
| Codebase Onboarding | Survey → trace key paths → deep dive on core |

**Why**: Efficient use of attention. Don't spend time on unimportant areas.

## Principle 5: Domain-Specific Tools Matter

Each workflow has specialized tools that dramatically improve effectiveness:

| Domain | Key Tools |
|--------|-----------|
| Security | semgrep, bandit, SAST scanners |
| Performance | perf, flame graphs, profilers |
| Flaky Tests | ThreadSanitizer, race detectors |
| RE Binary | Ghidra, hex editors, binwalk |
| Malware | Sandboxes, Cuckoo, YARA |
| Code Quality | Linters, type checkers |
| Crypto | Constant-time checkers |

**Why**: Generic tools miss domain-specific patterns. The right tool for the job.

## Principle 6: Verification at Boundaries

Check correctness at transition points:

| Workflow | Boundary Checks |
|----------|-----------------|
| Bug Fix | Test after fix, regression test |
| Migration | Behavior unchanged after each step |
| Binding Generation | Test at FFI boundary |
| Cross-Language | Differential testing old vs new |
| Feature Impl | Tests at each phase |

**Why**: Catch errors early. Smaller search space when something breaks.

## Principle 7: Output Should Inform Action

Workflow outputs should tell you what to do, not just present data:

| Bad Output | Good Output |
|------------|-------------|
| "Performance data collected" | "Function X is 80% of time, optimize there" |
| "Security scan complete" | "SQL injection at line 45, parameterize query" |
| "Review done" | "Block: auth bypass in login(), Fix: add check" |
| "Analysis finished" | "Dead code: remove files A, B, C" |

**Why**: The human/LLM shouldn't have to interpret raw data. Conclusions are the value.

## Common Workflow Patterns

### The Reproduction Pattern

Before fixing, prove you can trigger the problem:

- Bug Fix: Reproduce bug locally
- Flaky Test: Reproduce flakiness (even probabilistically)
- Performance: Reproduce slowness with benchmark
- Security: PoC the vulnerability

### The Bisection Pattern

When something changed, find when:

- Performance: git bisect to find slow commit
- Bug: git bisect to find breaking commit
- Flaky: git bisect to find when test became flaky

### The Differential Pattern

Compare known-good with suspected-bad:

- RE Binary: Compare similar files to find structure
- Migration: Compare output of old vs new
- Performance: Compare fast vs slow runs
- Synthesis: Compare generated code vs reference

### The Isolation Pattern

Reduce to minimal case:

- Bug: Minimal reproducer
- Flaky: Smallest test that flakes
- Performance: Isolated benchmark
- Security: Minimal PoC

## Workflow Composition

Workflows naturally compose:

### Sequential
```
Bug Report → Bug Investigation → Bug Fix → Code Review
```

### Nested
```
Feature Implementation
├── Question Answering (understand existing code)
├── Security Audit (new auth feature)
├── Implementation
│   └── Bug Fix (when tests fail)
└── Code Review
```

### Conditional
```
if tests_fail:
    spawn Bug Investigation
elif performance_regressed:
    spawn Performance Regression Hunting
elif security_relevant:
    spawn Security Audit
```

## Anti-Patterns Across Workflows

| Anti-Pattern | Appears In | Why Bad |
|--------------|------------|---------|
| Skip triage | All | Waste time on wrong thing |
| Single evidence source | Investigation | Miss blind spots |
| Fix before understand | Bug Fix | Create new bugs |
| No baseline | Performance | Can't measure improvement |
| No reproduction | Bug Fix, Flaky | Can't verify fix |
| Review without context | Code Review | Miss intent |
| Audit without threat model | Security | Miss what matters |

## LLM Assistance Patterns

Every workflow has LLM-specific techniques that share structure:

### Investigation Prompt
```
Given [artifact], identify:
1. [Specific question]
2. [Specific question]
```

### Generation Prompt
```
Generate [output] that:
1. [Constraint]
2. [Constraint]
Based on: [context]
```

### Review Prompt
```
Review [artifact] for [concerns]:
Check: [categories]
Output: [format]
```

## Failure Mode Prevention

Every workflow documents failures, but documentation alone is unreliable. Prevention hierarchy (most to least reliable):

| Method | Reliability | Example |
|--------|-------------|---------|
| **Tooling** | High | Linter catches pattern, CI blocks merge |
| **Type system** | High | Invalid states unrepresentable |
| **Tests** | Medium-High | Regression test encodes invariant |
| **Process** | Medium | Workflow step forces check |
| **Documentation** | Low | Requires someone to read and remember |

The failure mode tables (`| Failure | Detection | Recovery |`) are useful for:
- Human pattern matching after failure occurs
- Knowing what to do when tooling doesn't exist yet
- Identifying what tooling to build

But the real fix is encoding the failure mode in tooling so it can't happen silently.

## Open Questions

### Workflow Selection
How to choose the right workflow for a task?
- Intent classification?
- Trigger matching?
- Multiple workflows in parallel?

### Workflow Learning
Can workflows improve from experience?
- Track success/failure
- Adjust phases based on patterns
- Codebase-specific customization

### Cross-Workflow State
How to share context between composed workflows?
- Investigation findings → Bug Fix
- Security Audit findings → Code Review priorities
- Performance data → Refactoring targets

## See Also

- [RLM Research](../research/recursive-language-models.md) - Decomposition theory
- [Debugging Practices](debugging-practices.md) - Cross-cutting debugging patterns

