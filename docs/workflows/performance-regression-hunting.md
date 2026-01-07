# Performance Regression Hunting Workflow

Finding what made the system slower - identifying the commit, code path, and root cause of performance degradation.

## Trigger

- Users report slowness
- Benchmark suite shows regression
- Monitoring alerts on latency/throughput
- "It used to be fast" complaints

## Goal

- Identify the regressing commit
- Understand the root cause (not just "this function is slow")
- Fix or document intentional tradeoff
- Prevent recurrence (benchmark coverage)

## Prerequisites

- Reproducible benchmark or workload
- Access to historical performance data (or ability to test old commits)
- Profiling tools for target language/platform
- Statistical rigor (performance is noisy)

## Why Performance Regression Hunting Is Hard

1. **Noise**: Performance varies run-to-run (CPU scheduling, cache state, GC)
2. **No clear "failure"**: 5% slower - is that a regression or noise?
3. **Compound causes**: Multiple small changes add up
4. **Environment sensitivity**: Works fine on your machine, slow in prod
5. **Profiling overhead**: Observing changes what you measure
6. **Dependency changes**: Regression might be in a library, not your code

## Core Strategy: Establish → Bisect → Profile → Fix

```
┌─────────────────────────────────────────────────────────┐
│                    ESTABLISH BASELINE                    │
│  What was the "fast" state? When? How fast exactly?     │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                      BISECT                              │
│  Binary search to find the regressing commit            │
│  Need statistical confidence at each step               │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                      PROFILE                             │
│  Where is time being spent? What changed?               │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                       FIX                                │
│  Address root cause, verify improvement                 │
└─────────────────────────────────────────────────────────┘
```

## Phase 1: Establish Baseline

### What "Fast" Means

Before hunting, define the target:

```bash
# Current performance
hyperfine './program input.dat'
# Benchmark results:
#   Time (mean ± σ):     2.341 s ±  0.045 s

# Historical performance (if you have it)
# From CI logs, monitoring, or memory:
# "Last month it was ~1.5s"
```

### Statistical Significance

Performance is noisy. Need enough runs to be confident:

```bash
# BAD: Single run comparison
./program  # 2.3s
git checkout old-version
./program  # 1.5s  # Is this real or noise?

# GOOD: Multiple runs with statistics
hyperfine --warmup 3 --runs 20 './program input.dat'
# Reports mean, stddev, min, max

# Compare two versions statistically
hyperfine --warmup 3 \
  './program-new input.dat' \
  './program-old input.dat'
# Reports whether difference is statistically significant
```

**Rule of thumb**: If difference is < 2× standard deviation, be skeptical.

### Find the "Good" Commit

```bash
# Check release tags
git checkout v1.0.0
hyperfine './program input.dat'  # 1.5s - good!

git checkout v1.1.0
hyperfine './program input.dat'  # 2.3s - bad!

# Regression is somewhere between v1.0.0 and v1.1.0
```

## Phase 2: Bisect

### Git Bisect with Performance Test

```bash
git bisect start
git bisect bad HEAD           # Current is slow
git bisect good v1.0.0        # This was fast

# Manual bisect (for statistical tests)
git bisect run bash -c '
  make -j8 &&
  result=$(hyperfine --warmup 2 --runs 10 --export-json /tmp/bench.json "./program input.dat" 2>/dev/null) &&
  mean=$(jq ".results[0].mean" /tmp/bench.json) &&
  # Threshold: 1.8s (between good 1.5s and bad 2.3s)
  python3 -c "exit(0 if $mean < 1.8 else 1)"
'
```

### Handling Build Failures During Bisect

Old commits might not build cleanly:

```bash
# Skip commits that don't build
git bisect run bash -c '
  make -j8 || exit 125  # 125 = skip this commit
  # ... rest of performance test
'
```

### Narrowing Down

Once you find the commit:

```bash
git bisect bad
# abc123 is the first bad commit

git show abc123
# Shows the diff - now you know WHAT changed
# But still need to understand WHY it's slower
```

## Phase 3: Profile

### CPU Profiling

**Linux (perf)**:
```bash
# Record profile
perf record -g ./program input.dat

# View results
perf report
# Shows functions sorted by CPU time

# Generate flame graph
perf script | stackcollapse-perf.pl | flamegraph.pl > flame.svg
```

**macOS (Instruments/samply)**:
```bash
# samply - lightweight sampling profiler
samply record ./program input.dat
# Opens web UI with flame graph
```

**Language-specific profilers**:
```bash
# Python
py-spy record -o profile.svg -- python program.py

# Go
go tool pprof -http=:8080 cpu.pprof

# Rust
cargo flamegraph -- input.dat

# Node.js
node --prof program.js
node --prof-process isolate-*.log > profile.txt
```

### Differential Profiling

Compare profiles between good and bad versions:

```bash
# Profile both versions
git checkout good-commit
perf record -o good.perf.data ./program input.dat

git checkout bad-commit
perf record -o bad.perf.data ./program input.dat

# Compare (perf diff)
perf diff good.perf.data bad.perf.data

# Or generate both flame graphs and compare visually
# Look for: new hot spots, expanded existing hot spots
```

### Memory Profiling

Sometimes "slow" is actually memory-related:

```bash
# Check memory usage
/usr/bin/time -v ./program input.dat
# Look at "Maximum resident set size"

# Heap profiling
valgrind --tool=massif ./program input.dat
ms_print massif.out.*

# Check for excessive allocation
valgrind --tool=callgrind ./program input.dat
# Look at malloc/free call counts
```

### I/O Profiling

```bash
# strace for syscalls
strace -c ./program input.dat
# Shows syscall counts and time

# Detailed I/O
strace -e read,write -T ./program input.dat

# Linux: iotop, blktrace for disk I/O
```

## Phase 4: Root Cause Analysis

### Common Regression Patterns

| Pattern | Profile Signature | Root Cause |
|---------|-------------------|------------|
| Algorithm change | New function dominates | O(n²) replaced O(n), etc. |
| Removed optimization | Existing function slower | Cache, SIMD, inlining removed |
| Data structure change | More allocations | Vec → HashMap, etc. |
| Added feature | New code path | Validation, logging, hooks |
| Dependency update | Time in library code | Library regression |
| Reduced parallelism | Single thread hot | Removed threading, added lock |

### Micro vs Macro

The profiler shows WHERE time is spent, not necessarily WHAT to fix:

```
Hot function: parse_json() - 60% of time

Questions:
- Was parse_json() always slow, or did it get slower?
- Is parse_json() being called more times than before?
- Is the input to parse_json() larger than before?
- Did the implementation of parse_json() change?
```

### Counting Matters

Sometimes regression is "called more" not "slower per call":

```bash
# Instrument call counts
# In code: add counter
static CALL_COUNT: AtomicUsize = AtomicUsize::new(0);
fn parse_json(...) {
    CALL_COUNT.fetch_add(1, Ordering::Relaxed);
    // ...
}

# Or use perf stat
perf stat -e instructions,cycles,cache-misses ./program input.dat
```

## Common Fixes

### Algorithm Regression

```rust
// BAD: O(n²) crept in
for item in items {
    if other_items.contains(&item) {  // O(n) lookup
        // ...
    }
}

// GOOD: O(n) with set
let other_set: HashSet<_> = other_items.iter().collect();
for item in items {
    if other_set.contains(&item) {  // O(1) lookup
        // ...
    }
}
```

### Allocation Regression

```rust
// BAD: Allocating in hot loop
for i in 0..1000000 {
    let s = format!("item_{}", i);  // Allocates each iteration
    process(&s);
}

// GOOD: Reuse allocation
let mut buf = String::new();
for i in 0..1000000 {
    buf.clear();
    write!(&mut buf, "item_{}", i).unwrap();
    process(&buf);
}
```

### Caching Regression

```rust
// Regression: removed cache
fn expensive_lookup(key: &str) -> Value {
    database.query(key)  // Always hits DB
}

// Fix: restore caching
fn expensive_lookup(key: &str) -> Value {
    if let Some(cached) = cache.get(key) {
        return cached.clone();
    }
    let value = database.query(key);
    cache.insert(key.to_string(), value.clone());
    value
}
```

## Verification

After fixing:

```bash
# Verify fix with statistical comparison
hyperfine --warmup 3 \
  './program-fixed input.dat' \
  './program-broken input.dat'

# Should show statistically significant improvement
# Ideally back to baseline performance
```

## Production Profiling

Local profiling may not reflect production reality. Continuous profiling captures what's actually happening:

### Continuous Profiling Services

```
Always-on, low-overhead sampling in production:

- Pyroscope (open source) - flamegraphs over time, diff between deploys
- Datadog Continuous Profiler - integrated with APM
- Google Cloud Profiler - very low overhead
- AWS CodeGuru Profiler - ML-based recommendations
- Polar Signals (Parca) - open source, eBPF-based

Workflow:
1. Deploy new version
2. Compare production profiles: before vs after
3. Spot regressions with real traffic patterns
```

### eBPF-Based Profiling

Low-overhead, no instrumentation needed:

```bash
# bpftrace - one-liners for quick investigation
bpftrace -e 'profile:hz:99 { @[ustack] = count(); }'

# bcc tools
/usr/share/bcc/tools/profile -p $(pgrep myapp) -F 99 30 > profile.txt

# Converts to flame graph
/usr/share/bcc/tools/profile -p $(pgrep myapp) -F 99 -f 30 | flamegraph.pl > prod.svg
```

### When Local Doesn't Reproduce

Production regressions that don't appear locally:

| Factor | Why It Matters |
|--------|----------------|
| Data distribution | Production data has different characteristics |
| Concurrency | Real load patterns vs synthetic |
| Memory pressure | GC behaves differently under pressure |
| Cache state | Cold start vs warmed up |
| Network latency | Real latency vs localhost |

**Approach**: Capture production profile, compare to local profile - look for structural differences, not just timing.

## Distributed Tracing

For services, regression might be between components, not within:

### Tracing Tools

```
- Jaeger - open source, CNCF project
- Zipkin - open source, Twitter origin
- Datadog APM - commercial, integrated
- Honeycomb - observability focused
- AWS X-Ray - AWS integrated

All follow OpenTelemetry standard (mostly)
```

### Identifying Distributed Regressions

```
Trace shows:
  Service A (50ms) → Service B (200ms) → Database (50ms)
  Total: 300ms

After regression:
  Service A (50ms) → Service B (500ms) → Database (50ms)
  Total: 600ms

Service B is the culprit - but why?
- B's code got slower?
- B is making more calls to something?
- B is waiting on a lock/resource?
```

### Span-Level Analysis

```
Look at span breakdown within slow service:

Service B spans:
  - parse_request: 10ms (unchanged)
  - validate: 20ms (unchanged)
  - process: 450ms (was 170ms) ← regression here
  - serialize: 20ms (unchanged)

Now profile Service B's process() function specifically
```

### Cross-Service Bisect

When regression is in service interaction:

```bash
# 1. Identify which service pair has the regression
#    Trace data shows A→B latency increased

# 2. Check if A or B changed
git log --since="regression date" service-a/
git log --since="regression date" service-b/

# 3. Bisect the service that changed
#    Deploy old B with new A, measure
#    Deploy new B with old A, measure

# 4. Once isolated, profile that service
```

## CI Integration

### Continuous Benchmarking

Catch regressions before merge:

```yaml
# GitHub Actions example
- name: Run benchmarks
  run: cargo bench -- --save-baseline pr

- name: Compare to main
  run: |
    git fetch origin main
    git checkout origin/main
    cargo bench -- --save-baseline main
    git checkout -
    critcmp main pr --threshold 5  # Fail if >5% regression
```

### Benchmark Best Practices

```rust
// Use proper benchmark framework
#[bench]
fn bench_parse(b: &mut Bencher) {
    let input = include_str!("../fixtures/large.json");
    b.iter(|| {
        // Prevent optimizer from removing work
        black_box(parse(black_box(input)))
    });
}
```

### Tracking Historical Performance

```bash
# Store benchmark results in CI
# Graph over time to spot gradual regressions

# Tools:
# - Bencher (bencher.dev)
# - GitHub Action: benchmark-action
# - Custom: store JSON results, graph with matplotlib/gnuplot
```

## LLM-Specific Techniques

1. **Parse profiler output** - Extract hot functions from `perf report` or flame graphs
2. **Diff analysis** - Compare the regressing commit's changes to profile hot spots
3. **Pattern recognition** - Identify common regression patterns in code changes
4. **Suggest fixes** - Generate optimization suggestions based on profile

```bash
# Get structured profiler output
perf report --stdio --no-children > profile.txt

# Find the regressing commit's changes
git show <bad-commit> --stat
git diff <good-commit>..<bad-commit> -- <hot-file>
```

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| Can't reproduce | No regression in benchmark | Use production data/workload, check environment |
| Noise overwhelms signal | High variance | More runs, isolated environment, warmup |
| Wrong baseline | "Good" was also slow | Check earlier history, talk to users |
| Profiler overhead | Adds too much noise | Use sampling profiler, reduce overhead |
| Regression in dependency | Hot code is in library | Profile library, check library changelog |

## Anti-patterns

- **Premature optimization**: Fix the actual regression, don't optimize random things
- **Micro-benchmark tunnel vision**: Real workload might differ from synthetic benchmark
- **Ignoring statistical significance**: "It feels faster" isn't evidence
- **Fixing symptoms**: Adding cache without understanding why it got slower
- **Blame shifting**: "It's the GC/OS/hardware" without evidence

## Prevention

1. **Continuous benchmarking** in CI
2. **Benchmark critical paths** before merging
3. **Profile-guided code review** for performance-sensitive changes
4. **Performance budgets** - fail CI if latency exceeds threshold
5. **Document performance expectations** - "This should be O(n)"

## Tools Reference

| Category | Tools |
|----------|-------|
| Benchmarking | hyperfine, criterion, pytest-benchmark, go test -bench |
| CPU Profiling | perf, Instruments, samply, py-spy, pprof |
| Flame Graphs | flamegraph.pl, speedscope, Firefox Profiler |
| Memory | valgrind (massif), heaptrack, Instruments Allocations |
| I/O | strace, dtrace, iotop |
| CI Integration | Bencher, benchmark-action, custom dashboards |

## Open Questions

### Gradual Regressions

When performance degrades 1% per commit over 50 commits:
- Bisect finds nothing (each step is within noise)
- Total regression is 50%+ but no single commit is "the cause"
- Death by a thousand cuts

**Possible approaches**:

1. **Per-commit benchmark archive** - Store benchmark results for every trunk commit, forever. Enables:
   - Trend analysis over time
   - Detect when slope changed
   - Retroactive investigation
   - But: requires discipline, storage, consistent benchmark environment

2. **Statistical trend detection** - Fit regression line to benchmark history:
   ```
   If slope > threshold for N commits, alert
   Even if no single commit exceeds noise threshold
   ```

3. **Periodic baseline comparison** - Weekly/monthly comparison to fixed baseline:
   ```
   Compare HEAD to v1.0.0 (or "last known good")
   If >10% regression, investigate the range
   ```

4. **Cumulative diff analysis** - Look at cumulative changes, not individual:
   ```
   git diff v1.0.0..HEAD --stat
   # What areas grew the most?
   # Correlate with profile hot spots
   ```

**Open**: How to attribute gradual regression to specific changes when each is within noise? Is it even possible, or must you accept "the codebase got slower" and optimize holistically?

### Multi-Dimensional Performance

Trade-offs between latency, throughput, memory:
- Faster but uses more memory - is it a regression?
- Need multi-objective tracking

### Production vs Benchmark Divergence

Benchmarks might not reflect production:
- Different data distributions
- Different concurrency patterns
- How to keep benchmarks representative?

## See Also

- [Bug Investigation](bug-investigation.md) - General debugging
- [Flaky Test Debugging](flaky-test-debugging.md) - Statistical aspects similar
