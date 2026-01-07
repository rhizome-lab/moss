# High-Quality Code Synthesis Workflow

Generating correct code when training data is sparse: new languages, niche frameworks, novel domains where LLMs have limited exposure.

## Trigger

Need to write code for something with few examples:
- Bindings for a new library
- Implementation in an obscure language
- Domain-specific code (scientific, financial, protocol)
- Porting reference implementation to new language

## Goal

- Correct, idiomatic code
- Passes all available tests
- Consistent with all documentation claims
- No hallucinated APIs or behaviors

## Prerequisites

- Documentation available (even if incomplete)
- Reference implementation OR test suite (ideally both)
- Ability to run/test generated code

## Why This Is Hard

LLMs fail on low-data domains because:
1. **Hallucination**: Inventing APIs that don't exist
2. **Pattern mixing**: Applying patterns from similar-but-different domains
3. **Incomplete coverage**: Missing edge cases mentioned only in docs
4. **Inconsistency**: Different parts of generated code contradict each other

## Core Strategy: Triangulation + Exhaustive Verification

Don't trust any single source. Cross-reference everything against everything.

```
Sources:           Verification:
┌──────────┐      ┌─────────────────────────────────┐
│   Docs   │─────▶│  Every doc claim verified in    │
│  d1..dn  │      │  generated code (D × C check)   │
└──────────┘      └─────────────────────────────────┘
┌──────────┐      ┌─────────────────────────────────┐
│ Examples │─────▶│  Patterns extracted and applied │
│  e1..em  │      │  consistently                   │
└──────────┘      └─────────────────────────────────┘
┌──────────┐      ┌─────────────────────────────────┐
│  Tests   │─────▶│  All tests pass (oracle)        │
│  t1..tk  │      │                                 │
└──────────┘      └─────────────────────────────────┘
┌──────────┐      ┌─────────────────────────────────┐
│ Ref Impl │─────▶│  Behavior matches for same      │
│   (R)    │      │  inputs                         │
└──────────┘      └─────────────────────────────────┘
```

## Decomposition Strategy

### Phase 1: Source Preparation

```
1. COLLECT all available sources
   - Official documentation
   - Examples/tutorials
   - Test suites (in any language)
   - Reference implementations
   - Type definitions / schemas
   - Community discussions / issues

2. STRUCTURE the sources
   - Chunk docs into atomic claims
   - Extract function signatures
   - Identify invariants and constraints
   - Note edge cases explicitly mentioned

3. BUILD verification infrastructure
   - Set up test runner for target language
   - Create harness to run ref impl for comparison
   - Prepare diff tooling for output comparison
```

### Phase 2: Initial Synthesis

```
4. GENERATE skeleton from signatures
   - Types, function stubs, module structure
   - Don't implement logic yet

5. IMPLEMENT incrementally
   - Start with simplest functions
   - Use examples as guide for patterns
   - Cross-reference docs for each function

6. RUN tests after each addition
   - Fail fast, don't accumulate errors
   - Each test failure is immediate feedback
```

### Phase 3: Cartesian Verification (D × C)

This is the key differentiator. Exhaustively check every doc claim against the code.

```
7. EXTRACT claims from docs
   For each doc section:
   - "Function X returns Y when given Z"
   - "Parameter P must satisfy constraint C"
   - "Error E is raised when condition Q"
   - "Default value for V is D"

8. VERIFY each claim against code
   For each (claim, code_section) pair:
   - Does the code implement this claim?
   - Is the implementation correct?
   - Are there contradictions?

9. IDENTIFY gaps
   - Claims with no corresponding code
   - Code with no corresponding claim (sus!)
   - Contradictions between claims
```

### Phase 4: Introspection

```
10. INTERNAL CONSISTENCY check
    - Do types align across module boundaries?
    - Are error handling patterns consistent?
    - Do naming conventions match throughout?
    - Are there duplicate implementations?

11. PATTERN CONSISTENCY check
    - Extract patterns from working code
    - Apply same patterns to similar code
    - Flag deviations for review

12. EDGE CASE audit
    - Null/empty inputs
    - Boundary values
    - Error conditions
    - Concurrent access (if applicable)
```

### Phase 5: Refinement Loop

```
13. PRIORITIZE failures
    - Test failures (highest priority)
    - Doc inconsistencies
    - Pattern violations
    - Style issues (lowest)

14. FIX root causes
    - Trace each failure to source
    - Fix the understanding, not just the symptom
    - Update notes for similar cases

15. REPEAT until:
    - All tests pass
    - All doc claims verified
    - Internal consistency achieved
```

## Tools Used

| Phase | Tools |
|-------|-------|
| Collect | `fetch` (docs), `view` (ref impl) |
| Structure | LLM extraction, manual curation |
| Skeleton | `edit` (generate stubs) |
| Implement | `edit`, `view` (examples) |
| Test | `run` (test suite) |
| D×C check | Custom verification (see below) |
| Introspect | `analyze`, pattern matching |

## The D × C Verification Algorithm

```python
def verify_docs_against_code(docs: List[DocSection], code: List[CodeUnit]) -> List[Issue]:
    issues = []

    # Extract claims from docs
    claims = []
    for doc in docs:
        claims.extend(extract_claims(doc))

    # For each claim, find and verify corresponding code
    for claim in claims:
        matching_code = find_related_code(claim, code)

        if not matching_code:
            issues.append(UnimplementedClaim(claim))
            continue

        for code_unit in matching_code:
            if not implements_correctly(code_unit, claim):
                issues.append(IncorrectImplementation(claim, code_unit))

    # Check for code without doc backing (potential hallucination)
    for code_unit in code:
        if not any(claim_covers(c, code_unit) for c in claims):
            issues.append(UndocumentedCode(code_unit))  # Review carefully!

    return issues
```

Key insight: **Undocumented code is suspicious**. If the LLM generated something not in the docs, it might be hallucinated.

## Example: Generating Tree-Sitter Queries for New Language

**Situation**: Need highlight queries for language X. Few examples exist.

```
Turn 1: Collect sources
  - Grammar definition (node types)
  - 2-3 example query files from similar languages
  - Language syntax documentation
  - Sample source files in language X

Turn 2: Extract patterns from examples
  $(view grammars/rust/highlights.scm)
  $(view grammars/go/highlights.scm)
  → Pattern: (identifier) @variable, (type_identifier) @type, etc.

Turn 3: Map node types to highlight groups
  $(view grammars/x/src/node-types.json)
  → List all node types, categorize: keywords, types, literals, etc.

Turn 4: Generate initial queries
  $(edit grammars/x/highlights.scm)
  ; Keywords
  ["if" "else" "for" ...] @keyword
  ; Types
  (type_identifier) @type
  ...

Turn 5: Verify against sample files
  $(run moss highlight sample.x)
  → Check: are keywords highlighted? types? strings?

Turn 6: D × C check
  For each node type in grammar:
    - Is it captured by a query?
    - Is the capture group appropriate?
  For each syntax construct in docs:
    - Is it handled in queries?

Turn 7: Compare with similar language
  $(run diff <(moss highlight sample.rs) <(moss highlight sample.x))
  → Similar constructs should highlight similarly

Turn 8: Iterate until coverage complete
```

## Example: Porting Python Library to Rust

**Situation**: Port `coollib` from Python to Rust. Tests exist in Python.

```
Phase 1: Setup
  - Clone Python coollib, run its tests (baseline)
  - Create Rust project structure
  - Set up FFI or translation test harness

Phase 2: Extract specification
  $(view coollib/*.py --types-only)
  → List all public functions, classes, methods

  For each function:
    - Input types (from type hints or docstrings)
    - Output type
    - Side effects
    - Error conditions

Phase 3: Generate Rust skeleton
  $(edit src/lib.rs)
  pub fn function_name(args) -> Result<ReturnType, Error> {
      todo!()
  }

Phase 4: Implement with D × C checking
  For each function:
    1. Read Python implementation
    2. Read docstring/docs
    3. Write Rust implementation
    4. Extract all claims from docs
    5. Verify each claim in Rust code
    6. Run corresponding Python tests via harness

Phase 5: Cross-language test verification
  For each Python test:
    - Extract input/output pairs
    - Run same inputs through Rust
    - Compare outputs exactly

Phase 6: Edge case sweep
  $(view coollib/tests/*.py)
  → Ensure all test cases covered
  → Add Rust tests for edge cases
```

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| Hallucinated API | D×C check finds undocumented code | Remove or find doc backing |
| Misunderstood semantics | Tests fail | Re-read docs, check ref impl |
| Missing edge case | D×C finds unimplemented claim | Implement the missing case |
| Wrong pattern applied | Introspection finds inconsistency | Align with dominant pattern |
| Incomplete docs | Can't verify claim | Test empirically, document assumption |

## Metrics

- **Test pass rate**: % of reference tests passing
- **Doc coverage**: % of doc claims verified in code
- **Code coverage by docs**: % of code backed by docs (higher = less hallucination risk)
- **Iterations to convergence**: Fewer is better

## When To Use This Workflow

**Good fit:**
- Porting between languages with test suite
- Implementing spec with formal documentation
- Generating bindings for well-documented library
- Any case where correctness > speed

**Poor fit:**
- Exploratory coding (no spec)
- Domains with no reference implementation
- Time-critical prototyping
- Well-trodden paths (just use normal LLM generation)

## Relationship to RLM

This workflow embodies RLM principles:
- **Search before generate**: Extract all claims before writing code
- **Decompose exhaustively**: D×C is systematic decomposition
- **Verify incrementally**: Test after each addition
- **External memory**: Docs/tests as queryable environment

## Open Questions

- How to handle contradictions in docs?
- Optimal chunk size for claims?
- Automating D×C check vs. manual review?
- Handling probabilistic/non-deterministic behavior?
