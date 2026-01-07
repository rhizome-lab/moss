# Workflow Design

Design documents for moss workflows - structured patterns for accomplishing common software engineering tasks.

## What is a Workflow?

A workflow is a repeatable pattern that combines:
- **Trigger**: What initiates it (user request, file change, schedule)
- **Goal**: What success looks like
- **Tools**: Which moss primitives are needed
- **Decomposition**: How to break it into steps
- **Validation**: How to verify it worked

## Workflow Categories

### Investigation (understanding)
- [Question Answering](question-answering.md) - "How does X work?"
- [Codebase Orientation](codebase-orientation.md) - "What is this project?"
- [Dependency Tracing](dependency-tracing.md) - "What depends on X?"
- [Bug Investigation](bug-investigation.md) - "Why is X happening?"
- [Flaky Test Debugging](flaky-test-debugging.md) - "Why does this test sometimes fail?"
- [Performance Regression Hunting](performance-regression-hunting.md) - "Why did it get slow?"
- [Debugging Production Issues](debugging-production-issues.md) - "It's broken in prod, can't reproduce"

### Modification (changing)
- [Feature Implementation](feature-implementation.md) - "Add X feature"
- [Bug Fix](bug-fix.md) - "Fix X bug"
- [Refactoring](refactoring.md) - "Improve X without changing behavior"
- [Migration](migration.md) - "Update X to new version/pattern"
- [Merge Conflict Resolution](merge-conflict-resolution.md) - "Resolve conflicts preserving intent"
- [Dead Code Elimination](dead-code-elimination.md) - "Remove unused code safely"
- [Cross-Language Migration](cross-language-migration.md) - "Port Python to Rust"
- [Breaking API Changes](breaking-api-changes.md) - "Dependency update broke my code"

### Review (auditing)
- [Code Review](code-review.md) - "Review this PR"
- [Security Audit](security-audit.md) - "Find vulnerabilities"
- [Quality Audit](quality-audit.md) - "Find code smells"
- [API Review](api-review.md) - "Is this API well-designed?"

### Maintenance (keeping healthy)
- [Documentation Sync](documentation-sync.md) - "Keep docs up to date"
- [Dependency Updates](dependency-updates.md) - "Update dependencies"
- [Test Coverage](test-coverage.md) - "Improve test coverage"
- [Tech Debt](tech-debt.md) - "Address accumulated issues"

## Workflow Anatomy

Each workflow document should cover:

```
## Trigger
What initiates this workflow? User request, file change, CI, schedule?

## Goal
What does success look like? Concrete deliverable or state change?

## Prerequisites
What must be true before starting? Index built, tests passing, etc.

## Decomposition Strategy
How to break this into steps? Sequential, parallel, recursive?

## Tools Used
Which moss primitives? view, edit, text-search, analyze, etc.

## Validation
How to verify success? Tests pass, lint clean, human approval?

## Failure Modes
What can go wrong? How to detect and recover?

## Example Session
Concrete example of the workflow in action.

## Variations
Different flavors of this workflow for different contexts.
```

## Design Principles

### 1. Search Before Act
RLM research shows: filtering/searching before LLM processing is 3x cheaper and more accurate. Every modification workflow should start with investigation.

### 2. Validate Early and Often
Don't batch validation at the end. Check after each step where possible. Fail fast.

### 3. Decompose Recursively
Large tasks should spawn sub-tasks. The depth of recursion should match the complexity of the task.

### 4. Preserve Reversibility
Prefer workflows that can be undone. Shadow editing, git commits as checkpoints, etc.

### 5. Explicit Over Implicit
Log decisions, show what was considered, explain why alternatives were rejected.

## Workflow Composition

Workflows can compose:
- **Sequential**: Bug Investigation → Bug Fix → Code Review
- **Nested**: Feature Implementation contains multiple Refactoring sub-workflows
- **Conditional**: If tests fail after edit, spawn Bug Investigation

## Edge Case Workflows (to explore)

Unusual or challenging scenarios that don't fit standard patterns:

### Investigation Edge Cases
- [Reverse Engineering Code](reverse-engineering-code.md) - understanding undocumented/legacy code with no context
- **Reverse engineering binary formats** - understanding file formats, protocols without docs
- **Debugging production issues** - working from logs/traces without local reproduction
- **Performance regression hunting** - finding what made things slow
- **Flaky test debugging** - non-deterministic failures, timing issues, environment dependencies

### Modification Edge Cases
- **Merge conflict resolution** - understanding both sides, choosing correct resolution
- **Cross-language migration** - porting code between languages (Python→Rust, JS→TS)
- **Breaking API changes** - handling upstream dependency changes that break your code
- **Dead code elimination** - safely removing unused code paths

### Synthesis Edge Cases (low training data)
- **High-quality code synthesis** - generating correct code with minimal examples
  - Extract patterns from sparse existing data
  - Use test suites from reference implementations (other languages) as specification
  - Cartesian product: compare each doc page against all synthesized code
  - Introspect generated code for internal consistency
  - Iterative refinement against known-good tests
- [Binding Generation](binding-generation.md) - generating FFI/bindings for libraries
- [Grammar/Parser Generation](grammar-parser-generation.md) - creating parsers from examples + informal specs

### Meta Workflows
- [Codebase Onboarding](codebase-onboarding.md) - "Understand this new project"
- [Documentation Synthesis](documentation-synthesis.md) - generating docs from code (inverse of code synthesis)

### Security/Forensic Edge Cases
- [Cryptanalysis](cryptanalysis.md) - analyzing crypto implementations for weaknesses
- [Steganography Detection](steganography-detection.md) - finding hidden data in files
- [Malware Analysis](malware-analysis.md) - understanding malicious code behavior (read-only!)

## Implementation Status

| Workflow | Status | Notes |
|----------|--------|-------|
| Question Answering | Documented | Investigator role in agent |
| Bug Fix | Documented | - |
| Code Review | Documented | - |
| Code Synthesis | Documented | D×C verification, low-data domains |
| Binary RE | Documented | Hypothesis-driven differential analysis |
| Flaky Test Debugging | Documented | Race detectors, CI diagnosis, Antithesis |
| Perf Regression | Documented | Profiling, distributed tracing, continuous profiling |
| Merge Conflicts | Documented | Semantic merge, resolution reasoning logs |
| Dead Code Elimination | Documented | Runtime tracing, tree shaking, tombstoning |
| Codebase Onboarding | Documented | Survey → Trace → Map → Verify |
| Production Debugging | Documented | Scope → Correlate → Hypothesize → Verify |
| Cross-Language Migration | Documented | Concept mapping, verification, incremental |
| Breaking API Changes | Documented | Assess, compatibility shims, semantic verification |
| Reverse Eng. Code | Documented | Execute → Trace → Understand → Document |
| Binding Generation | Documented | Analyze → Generate → Wrap → Test |
| Grammar/Parser Gen | Documented | Collect → Infer → Generate → Validate |
| Documentation Synth | Documented | Extract → Organize → Generate → Validate |
| Cryptanalysis | Documented | Survey → Analyze → Verify → Report |
| Steganography | Documented | Triage → Analyze → Extract → Verify |
| Malware Analysis | Documented | Triage → Static → Dynamic → Document |
| Security Audit | Partial | Auditor role + analyze security |
| Feature Implementation | Not started | - |

## Living Documents

These workflows are perpetually incomplete. They capture current understanding and known techniques, but:
- New tools emerge (pattern languages, analysis frameworks)
- Edge cases surface during real usage
- Better decomposition strategies are discovered
- LLM capabilities evolve, changing what's automatable

Treat each workflow as a starting point, not a complete prescription. Update as you learn.

## See Also

- `docs/research/recursive-language-models.md` - RLM paper insights on decomposition
- `docs/design/agent.md` - Agent architecture
- `.moss/scripts/` - Lua workflow implementations
