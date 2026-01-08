# Bug Investigation Workflow

Understanding why something is broken: "Why is X happening?", "What's causing Y?"

## Trigger

- Bug report with unclear cause
- Unexpected behavior observed
- Error that needs root cause analysis
- Regression with unknown origin

## Goal

- Identify the root cause of the issue
- Understand the failure mechanism
- Document findings for the [bug fix](bug-fix.md) phase
- Avoid fixing symptoms instead of causes

## Prerequisites

- Reproducible issue (or logs/traces)
- Access to relevant code
- Understanding of expected behavior

## Decomposition Strategy

**Reproduce → Isolate → Trace → Understand**

```
1. REPRODUCE: Confirm the bug exists
   - Run reproduction steps
   - Capture actual vs expected behavior
   - Note environmental factors

2. ISOLATE: Narrow the scope
   - Binary search through code/commits
   - Minimize reproduction case
   - Identify affected components

3. TRACE: Follow the data flow
   - Trace from symptom to cause
   - Log intermediate values
   - Identify where behavior diverges

4. UNDERSTAND: Determine root cause
   - Why does this code behave this way?
   - What assumption was violated?
   - Are there related issues?
```

## Tools Used

| Phase | Tools |
|-------|-------|
| Reproduce | Test runners, manual execution |
| Isolate | `git bisect`, `text-search`, `view` |
| Trace | `analyze trace`, `analyze callers/callees` |
| Understand | `view`, `text-search` for patterns |

## Investigation Techniques

### Stack Trace Analysis
```
1. Read error message carefully
2. Find the crash/error location
3. Walk up the stack to find root cause
4. Check if error handling masked the real issue
```

### Binary Search (git bisect)
```
1. Find a known-good commit
2. Find the first bad commit
3. git bisect to find the culprit
4. Analyze the breaking change
```

### Logging/Tracing
```
1. Add strategic log points
2. Trace variable values
3. Identify where values diverge from expected
4. Remove logs after investigation
```

### Delta Debugging
```
1. Create minimal reproduction case
2. Remove code/data until bug disappears
3. Add back to find minimal trigger
4. Minimal case reveals root cause
```

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| Can't reproduce | Bug doesn't appear | Get more details, check environment |
| Too many variables | Can't isolate | Divide and conquer, test hypotheses |
| Red herring | Following wrong trail | Step back, verify assumptions |
| Analysis paralysis | Not converging | Time-box, try simpler hypothesis |

## Example Session

**Bug**: "Users sometimes see wrong profile data"

```
Turn 1: Understand the report
  → "Sometimes" suggests race condition or caching
  → "Wrong profile" could be stale or different user

Turn 2: Search for profile loading
  $(text-search "load.*profile" --only "*.rs")
  → src/users/profile.rs:load_profile()
  → src/cache/user_cache.rs:get_cached_profile()

Turn 3: Check caching logic
  $(view src/cache/user_cache.rs/get_cached_profile)
  → Cache key is just user_id
  → No invalidation on profile update

Turn 4: Check cache invalidation
  $(text-search "invalidate" path:src/cache)
  → No invalidation for user_cache

Turn 5: Trace update flow
  $(analyze callers update_profile)
  → API handler calls update_profile
  → update_profile doesn't touch cache

Turn 6: Root cause identified
  → Profile updates don't invalidate cache
  → Users see stale data until cache expires
  → Fix: Add cache invalidation to update_profile
```

## Variations

### Performance Investigation
Profile first, identify hotspots, measure before/after.

### Intermittent Bug
Add logging, increase test iterations, look for timing dependencies.

### Data Corruption
Check input validation, look for buffer overflows, verify serialization.

### Memory Leak
Use memory profiler, track allocations, check for reference cycles.

## Documentation

After investigation, document:
1. **Symptom**: What was observed
2. **Root cause**: Why it happened
3. **Mechanism**: How it happened
4. **Fix approach**: How to resolve (input for bug-fix workflow)
5. **Prevention**: How to avoid similar issues

## See Also

- [Bug Fix](bug-fix.md) - Next step after investigation
- [Debugging Practices](debugging-practices.md) - Cross-cutting debugging techniques
