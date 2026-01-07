# Dead Code Elimination Workflow

Safely identifying and removing unused code paths without breaking functionality.

## Trigger

- Codebase has accumulated cruft over time
- Refactoring removed callers but not callees
- Feature was sunset but code remains
- Coverage reports show untouched code
- Build times / binary size concerns

## Goal

- Remove genuinely dead code
- **Not** remove code that's actually needed (critical!)
- Reduce maintenance burden
- Improve code clarity
- Potentially improve build times / binary size

## Prerequisites

- Test suite (to verify nothing breaks)
- Understanding of codebase entry points
- Access to production metrics / usage data (ideally)
- Version control (can revert if wrong)

## Why Dead Code Elimination Is Hard

1. **"Dead" has multiple meanings**: Unreachable? Unused? Rarely used?
2. **Dynamic dispatch**: Static analysis can't see all call paths
3. **Reflection/metaprogramming**: Code called by name, not reference
4. **External callers**: Library code, plugins, APIs
5. **Conditional compilation**: Feature flags, platform-specific code
6. **Fear of deletion**: "Someone might need this" paralysis

## Types of Dead Code

| Type | Definition | Detection Difficulty |
|------|------------|---------------------|
| **Unreachable** | No execution path leads here | Easy (compiler often warns) |
| **Unused** | Defined but never called | Medium (static analysis) |
| **Conditionally dead** | Behind always-false flag | Hard (need flag state analysis) |
| **Effectively dead** | Called but result ignored | Hard (need data flow analysis) |
| **Deprecated** | Has callers but shouldn't | Manual identification |
| **Test-only** | Only called from tests | Context-dependent |

## Core Strategy: Detect → Verify → Remove → Validate

```
┌─────────────────────────────────────────────────────────┐
│                      DETECT                              │
│  Find candidate dead code (static + dynamic analysis)   │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                      VERIFY                              │
│  Confirm code is truly dead (not reflection, etc.)      │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                      REMOVE                              │
│  Delete code (or deprecate first for gradual removal)   │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                     VALIDATE                             │
│  Tests pass, build succeeds, production healthy         │
└─────────────────────────────────────────────────────────┘
```

## Phase 1: Detection

### Static Analysis - Compiler Warnings

```bash
# Rust: Built-in dead code warnings
cargo build 2>&1 | grep "never used"
# warning: function `old_helper` is never used

# Go: Unused detection
go vet ./...
staticcheck ./...  # More thorough

# TypeScript: noUnusedLocals, noUnusedParameters
tsc --noUnusedLocals --noUnusedParameters

# Python: pylint, flake8
pylint --disable=all --enable=W0612,W0611 .  # Unused variables/imports

# C/C++: Compiler warnings
gcc -Wall -Wunused-function -Wunused-variable ...
```

### Static Analysis - Dedicated Tools

```bash
# JavaScript/TypeScript: ts-prune
npx ts-prune
# Shows exports with no imports

# Python: vulture
vulture src/
# Reports unused code with confidence scores

# Java: IntelliJ "Unused declaration" inspection
# Or: ProGuard/R8 shrinking (for Android)

# General: Universal Ctags + custom scripts
ctags -R --fields=+n .
# Parse tags file, find definitions without references
```

### Dynamic Analysis - Coverage

```bash
# Run with coverage, look for 0% covered code
# This catches code that's "reachable" but never actually reached

# Python
coverage run -m pytest
coverage report --show-missing | grep "0%"

# JavaScript
nyc npm test
# Look for files/functions with 0% coverage

# Rust
cargo tarpaulin
# Or: cargo llvm-cov

# Go
go test -coverprofile=coverage.out ./...
go tool cover -func=coverage.out | grep "0.0%"
```

### Production Metrics

The gold standard: what actually runs in production?

```
If you have:
- APM (Datadog, New Relic, etc.)
- Distributed tracing
- Custom instrumentation

Check:
- Functions that were never called in past N months
- Endpoints with zero requests
- Feature flags that are always off
```

### Runtime Call Graph Tracing

For reflection-heavy code, static analysis fails. Trace actual calls:

```python
# Python: sys.settrace or coverage.py
import sys

called_functions = set()

def trace_calls(frame, event, arg):
    if event == 'call':
        called_functions.add(frame.f_code.co_qualname)
    return trace_calls

sys.settrace(trace_calls)
# Run your application
# called_functions now has actual call graph
```

**APM-based tracing**:
- Datadog APM: Shows function-level call graphs
- New Relic: Transaction traces with method calls
- Jaeger/Zipkin: Span-level visibility

**Workflow**:
1. Deploy with tracing enabled
2. Collect data for representative period (cover all code paths)
3. Export call graph
4. Diff against static "all functions" list
5. Functions never called = dead code candidates

This catches dynamic dispatch that static analysis misses.

## Phase 2: Verification

### Check for Dynamic Dispatch

```python
# Python: Watch for getattr, eval, exec
code = "process_" + action  # Dynamic function name
getattr(module, code)()     # Static analysis won't see this

# JavaScript: Similar with obj[key]()
const handler = handlers[eventType];  # Dynamic lookup
handler();

# Solution: Search for patterns
grep -r "getattr\|eval\|exec" .
grep -r "\[.*\]\s*(" .  # obj[key]() pattern
```

### Check for Reflection

```java
// Java reflection
Class.forName("com.example.OldClass").newInstance();

// Search for reflection patterns
grep -r "Class.forName\|getMethod\|invoke" .
```

### Check External Callers

Questions to ask:
- Is this a library? External code might call it
- Is this an API endpoint? Clients might depend on it
- Is this a CLI entry point? Scripts might invoke it
- Is this a plugin interface? Plugins might implement it

```bash
# For libraries: check if exported
# Public API should be treated as "potentially used"

# For internal code: check if referenced elsewhere
grep -r "function_name" --include="*.py" .
```

### Check Conditional Compilation

```rust
// Rust: cfg attributes
#[cfg(feature = "deprecated_feature")]
fn old_code() { ... }  // Dead if feature never enabled

// C/C++: preprocessor
#ifdef LEGACY_MODE
void legacy_function() { ... }
#endif

// Check: Is the condition ever true?
grep -r "LEGACY_MODE" . | grep -v "#ifdef"
```

### Build a Confidence Score

```
High confidence (safe to delete):
  - Compiler says unused AND coverage is 0% AND no reflection patterns
  - Private function with no callers in same file

Medium confidence (verify first):
  - Static analysis says unused but coverage > 0% (tests only?)
  - Public function with no internal callers (external use?)

Low confidence (investigate deeply):
  - Used via reflection/dynamic dispatch
  - Part of public API
  - Feature-flagged code
```

## Phase 3: Removal Strategies

### Direct Deletion

For high-confidence dead code:

```bash
# Just delete it
rm src/old_module.py
# Or remove the function

git commit -m "Remove unused old_module

Static analysis confirmed no callers, 0% coverage,
no reflection patterns found."
```

### Deprecation-First

For medium-confidence or public API:

```python
# Step 1: Add deprecation warning
import warnings

def old_function():
    warnings.warn(
        "old_function is deprecated and will be removed in v2.0",
        DeprecationWarning,
        stacklevel=2
    )
    # ... existing code

# Step 2: Monitor for warnings in production
# Step 3: If no warnings after N weeks, remove
```

```rust
// Rust
#[deprecated(since = "1.5.0", note = "Use new_function instead")]
pub fn old_function() { ... }
```

### Gradual Removal (Large Deletions)

For large amounts of dead code:

```
1. Create tracking issue listing all candidates
2. Remove in small, reviewable batches
3. Each batch:
   - Delete N items
   - Run tests
   - Deploy to staging/canary
   - Wait for any issues
   - Merge to main
4. Repeat until list exhausted
```

### Soft Deletion (Feature Flags)

When uncertain:

```python
# Wrap in feature flag
if settings.ENABLE_LEGACY_CODE:
    old_function()

# Deploy with flag OFF
# Monitor for issues
# After confidence period, delete code AND flag
```

### Tombstoning

Leave a marker when deleting significant code - helps future archaeologists:

```python
# TOMBSTONE: process_legacy_orders() removed 2024-03-15
# Reason: Legacy order system sunset, all orders migrated to v2
# Commit: abc123
# Contact: @alice if questions
#
# The function handled order processing for the pre-2020 system.
# If you're seeing errors related to legacy orders, the migration
# script is in scripts/migrate_legacy_orders.py
```

**When to tombstone**:
- Deleting major features (not small helpers)
- Removing code that external systems might expect
- Code with complex history that's hard to reconstruct from git

**When NOT to tombstone**:
- Small utility functions (git blame is enough)
- Obvious dead code with no special context
- Routine cleanup (tombstones become noise)

**Tombstone contents**:
```
# TOMBSTONE: <what was removed>
# Removed: <date>
# Reason: <why it was removed>
# Commit: <hash for git archeology>
# Context: <anything non-obvious>
# Contact: <who to ask if questions>
```

**Alternative: Git notes**
```bash
# Attach note to deletion commit without polluting code
git notes add -m "Removed process_legacy_orders: sunset of v1 system" abc123

# View notes
git log --show-notes
```

**Alternative: Architecture Decision Records (ADRs)**
For major removals, document in `docs/adr/`:
```markdown
# ADR-042: Remove Legacy Order Processing

## Status: Accepted

## Context
The v1 order system was sunset in January 2024...

## Decision
Remove all code in src/legacy_orders/...

## Consequences
- External partners using legacy API must migrate
- Historical order data remains in archive database
```

## Phase 4: Validation

### Immediate Validation

```bash
# 1. Build succeeds
cargo build  # or npm run build, etc.

# 2. Tests pass
cargo test

# 3. Lint clean
cargo clippy
```

### Staged Rollout

```
1. Merge to main
2. Deploy to staging environment
3. Run integration tests
4. Deploy to canary (small % of production)
5. Monitor metrics for anomalies
6. If clean, roll out fully
```

### Monitoring After Removal

Watch for:
- Error rate increase
- New exception types
- Features that stopped working (user reports)
- Import errors / resolution failures

```bash
# Quick rollback plan
git revert <commit>  # Restore deleted code
```

## Language-Specific Tools

### Rust

```bash
# Built-in: Excellent dead code detection
cargo build  # Warns on unused

# For more thorough analysis
cargo udeps  # Unused dependencies
cargo +nightly udeps  # More accurate

# Machine-readable
cargo build --message-format=json 2>&1 | jq 'select(.reason == "compiler-message")'
```

### Go

```bash
# staticcheck - comprehensive
staticcheck -checks U1000 ./...  # Unused code

# deadcode (experimental, in x/tools)
go install golang.org/x/tools/cmd/deadcode@latest
deadcode ./...
```

### JavaScript/TypeScript

```bash
# ts-prune - unused exports
npx ts-prune

# eslint with rules
# eslint-plugin-unused-imports

# Webpack/Rollup tree-shaking (see below)
```

### Tree Shaking (Bundler-Based Elimination)

Modern bundlers automatically eliminate dead code at build time:

**How it works**:
```javascript
// math.js
export function add(a, b) { return a + b; }
export function multiply(a, b) { return a * b; }  // Never imported

// app.js
import { add } from './math.js';
console.log(add(1, 2));

// Bundle output: multiply() is eliminated
```

**Bundler support**:
- **Webpack**: Tree shaking via `mode: 'production'` + ES modules
- **Rollup**: Excellent tree shaking by default
- **esbuild**: Fast, good tree shaking
- **Parcel**: Automatic tree shaking

**Requirements for effective tree shaking**:
```javascript
// GOOD: ES modules (static imports)
import { specific } from 'library';

// BAD: CommonJS (dynamic, can't tree shake)
const lib = require('library');

// BAD: Import entire namespace
import * as lib from 'library';  // Bundler can't know what's used
```

**Interaction with manual elimination**:
- Tree shaking handles *imported* dead code at build time
- Manual elimination handles *exported* dead code in your source
- Both are complementary:
  - Tree shake dependencies you don't fully use
  - Manually delete your own dead exports

**Analyzing what's shaken**:
```bash
# Webpack: analyze bundle
npx webpack-bundle-analyzer dist/stats.json

# Rollup: see what's excluded
rollup -c --plugin 'visualizer()'

# esbuild: metafile shows imports
esbuild --metafile=meta.json ...
```

**Caveats**:
- Side effects prevent shaking: `import 'polyfill';`
- Mark pure packages: `"sideEffects": false` in package.json
- Dynamic imports can't be analyzed: `import(variable)`

### Python

```bash
# vulture - most comprehensive
vulture src/ --min-confidence 80

# pylint
pylint --disable=all --enable=W0611,W0612,W0613 .

# Coverage-based
coverage run -m pytest && coverage report --fail-under=X
```

### Java

```bash
# IntelliJ IDEA: Analyze → Run Inspection by Name → "Unused declaration"

# ProGuard (Android): Shrinking removes unused code

# UCDetector (Eclipse plugin)
```

## LLM-Specific Techniques

### Automated Detection Report

```bash
# Gather all signals
echo "=== Compiler Warnings ===" > dead_code_report.txt
cargo build 2>&1 | grep "never used" >> dead_code_report.txt

echo "=== Zero Coverage ===" >> dead_code_report.txt
cargo llvm-cov --text | grep "0.00%" >> dead_code_report.txt

echo "=== Vulture (if Python) ===" >> dead_code_report.txt
vulture src/ >> dead_code_report.txt
```

### Verification Prompts

For each candidate:
```
Function: process_legacy_data()
File: src/legacy.py:45

Questions:
1. Is this called via reflection/dynamic dispatch?
2. Is this part of a public API?
3. Is this behind a feature flag that might be enabled?
4. Are there external systems that might call this?

Search results:
- grep "process_legacy_data": [results]
- grep "getattr.*legacy": [results]
```

### Batch Analysis

```
Given N dead code candidates:
1. Group by module/package
2. Identify dependencies between candidates (remove together)
3. Prioritize by confidence score
4. Generate removal plan with batches
```

## Common Mistakes

| Mistake | Consequence | Prevention |
|---------|-------------|------------|
| Deleting reflection targets | Runtime crash | Search for dynamic patterns |
| Deleting public API | Break external users | Check exports, deprecate first |
| Deleting feature-flagged code | Break when flag enabled | Check all flag states |
| Big-bang deletion | Hard to identify cause of bugs | Small batches |
| No monitoring after | Silent failures | Watch metrics post-deploy |

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| Deleted needed code | Runtime errors, test failures | `git revert`, investigate callers |
| Broke external user | Bug reports, API errors | Restore, add to public API docs |
| Deleted test utility | Tests fail | Restore to test helpers |
| Broke rare code path | Production errors (eventually) | Revert, add test coverage |

## Anti-patterns

- **"Might need it someday"**: If it's unused, delete it. Git remembers.
- **Commented-out code**: Worse than dead code - delete it
- **TODO: remove this**: Either remove it or remove the TODO
- **Copy-paste to "archive"**: Just delete. Git is your archive.
- **Deleting without tests passing**: Always verify first

## Prevention

1. **Delete as you go**: When removing callers, remove callees
2. **Regular audits**: Periodic dead code sweeps (quarterly?)
3. **CI warnings as errors**: Don't let unused code accumulate
4. **Coverage tracking**: Watch for declining coverage trends
5. **Feature flag hygiene**: Remove flags (and code) when feature is stable

## Open Questions

### Test-Only Code

Code called only by tests - is it dead?
- If testing internal implementation: maybe dead (delete test + code)
- If testing edge cases: not dead (keep)
- How to distinguish programmatically?

### Reflection/Metaprogramming

Static analysis fundamentally can't handle:
- `getattr(module, user_input)()`
- Dependency injection frameworks
- Plugin systems

Options:
- Convention-based exceptions (annotate as "dynamically called")
- Runtime tracing to see actual calls
- Accept false negatives in these areas

### Gradual Type Systems

In gradually typed Python/JS:
- Static analysis has limited visibility
- Type coverage affects dead code detection accuracy
- Strategy: Type more → Better dead code detection

## See Also

- [Refactoring](refactoring.md) - Dead code removal is often part of refactoring
- [Code Review](code-review.md) - Review dead code removal carefully
