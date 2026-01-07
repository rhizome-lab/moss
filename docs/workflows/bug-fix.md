# Bug Fix Workflow

Fixing a reported bug: "X doesn't work", "Y crashes when Z", "Expected A but got B".

## Trigger

Bug report with reproduction steps or error message.

## Goal

- Bug no longer reproduces
- Tests pass (existing + new regression test)
- No new bugs introduced
- Root cause documented

## Prerequisites

- Bug can be reproduced (or clear error message/logs)
- Tests can be run locally
- Permission to modify code

## Decomposition Strategy

**Investigate → Hypothesize → Fix → Validate**

```
1. REPRODUCE: Confirm bug exists
   - Run reproduction steps
   - Capture actual vs expected behavior

2. LOCATE: Find the buggy code
   - Search for error messages, stack traces
   - Trace from symptom to cause
   - Identify the specific function/line

3. UNDERSTAND: Why does the bug occur?
   - Read surrounding code
   - Check edge cases, assumptions
   - Form hypothesis about root cause

4. FIX: Make minimal change
   - Fix root cause, not symptom
   - Prefer smallest change that works
   - Don't refactor unrelated code

5. TEST: Add regression test
   - Test should fail before fix, pass after
   - Cover the specific edge case

6. VALIDATE: Ensure fix is complete
   - Run full test suite
   - Re-run reproduction steps
   - Check for related edge cases
```

## Tools Used

| Phase | Tools |
|-------|-------|
| Reproduce | `run` (tests), manual execution |
| Locate | `text-search` (error messages), `analyze trace` |
| Understand | `view`, `analyze callers/callees` |
| Fix | `edit` |
| Test | `edit` (add test), `run` (tests) |
| Validate | `run` (full suite), `analyze` |

## Validation

- [ ] Original reproduction steps no longer trigger bug
- [ ] New regression test exists and passes
- [ ] All existing tests pass
- [ ] No new lint warnings
- [ ] Fix addresses root cause (not just symptom)

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| Can't reproduce | Bug doesn't appear locally | Get more details, check environment differences |
| Fix breaks other tests | Test suite fails | Revert, understand dependencies, fix properly |
| Fix only masks symptom | Bug reappears in variant | Dig deeper for root cause |
| Multiple bugs entangled | Fix one, another appears | Separate into multiple fixes |
| Performance regression | Benchmarks slower | Profile, optimize or accept tradeoff |

## Example Session

**Bug**: "moss view crashes on empty files"

```
Turn 1: Reproduce
  $(run echo "" > /tmp/empty.rs && ./target/debug/moss view /tmp/empty.rs)
  → thread 'main' panicked at 'index out of bounds'

Turn 2: Locate via stack trace
  $(text-search "index out of bounds" --only "*.rs")
  → No direct match, search for view command
  $(view crates/moss/src/commands/view/mod.rs --types-only)

Turn 3: Find the crash site
  $(text-search "lines\[" path:crates/moss/src/commands/view)
  → view/mod.rs:142: let first_line = lines[0];

Turn 4: Understand
  $(view crates/moss/src/commands/view/mod.rs:135-150)
  → No check for empty lines array before indexing

Turn 5: Fix
  $(edit crates/moss/src/commands/view/mod.rs)
  - let first_line = lines[0];
  + let first_line = lines.first().unwrap_or(&"");

Turn 6: Add test
  $(edit crates/moss/src/commands/view/tests.rs)
  + #[test]
  + fn test_view_empty_file() { ... }

Turn 7: Validate
  $(run cargo test -p moss view)
  → All tests pass
  $(run ./target/debug/moss view /tmp/empty.rs)
  → No crash, shows empty output
```

## Variations

### Intermittent/Flaky Bugs
Add logging, run multiple times, look for race conditions or timing issues.

### Performance Bugs ("X is slow")
Profile first (`analyze complexity`), then optimize hot paths.

### Security Bugs
Higher stakes: careful review, consider all attack vectors, may need coordinated disclosure.

### Regression Bugs ("X used to work")
Use `git bisect`, `view --history` to find when it broke.

## Anti-patterns

- **Shotgun debugging**: Making random changes hoping something works
- **Symptom fixing**: Catching exception instead of fixing cause
- **Scope creep**: Refactoring unrelated code while fixing bug
- **Missing test**: Fixing without adding regression test

## Metrics

- **Time to fix**: From report to merged fix
- **Fix quality**: Does it address root cause?
- **Regression rate**: Does the same bug come back?
- **Collateral damage**: Tests broken by fix
