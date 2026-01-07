# Flaky Test Debugging Workflow

Diagnosing and fixing tests that sometimes pass, sometimes fail - without clear reason.

## Trigger

- Test passes locally, fails in CI (or vice versa)
- Test fails intermittently with no code changes
- Test suite "goes red" randomly
- CI requires multiple retries to pass

## Goal

- Identify root cause of non-determinism
- Fix the flakiness (not just the symptom)
- Prevent recurrence (better test design)
- Documented understanding of what went wrong

## Prerequisites

- Ability to run tests repeatedly
- Access to CI logs/history
- Ideally: ability to run under stress conditions

## Why Flaky Tests Are Hard

1. **Non-reproducible on demand**: Works when you look at it
2. **Multiple interacting causes**: Timing AND state AND environment
3. **Heisenbug effect**: Adding debug output changes timing, "fixes" it
4. **Environment differences**: CI has different resources, parallelism, timing
5. **Intermittent = ignored**: "Just retry" culture masks the problem

## Categories of Flakiness

| Category | Symptoms | Common Causes |
|----------|----------|---------------|
| **Timing/Race** | Fails under load, parallel runs | Async without proper waits, shared resources |
| **State Pollution** | Fails when run after specific tests | Global state, database not cleaned, singletons |
| **External Dependencies** | Fails on specific machines/times | Network, filesystem, system time, locale |
| **Resource Exhaustion** | Fails in CI, works locally | File handles, ports, memory, connections |
| **Randomness** | Fails on specific seeds | RNG without fixed seed, hash iteration order |
| **Time-Dependent** | Fails near midnight/month-end | Hardcoded dates, timezone issues, expiry logic |

## Core Strategy: Reproduce → Isolate → Fix

```
┌─────────────────────────────────────────────────────────┐
│                    REPRODUCE                             │
│  Run until failure - establish that flakiness exists     │
│  Get failure rate baseline (1%, 10%, 50%?)              │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                     ISOLATE                              │
│  Narrow down: which category? which code path?          │
│  Binary search across dimensions                        │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                      FIX                                 │
│  Address root cause, not symptom                        │
│  Verify fix by running N times                          │
└─────────────────────────────────────────────────────────┘
```

## Phase 1: Reproduce

### Establish Flake Rate

```bash
# Run test N times, count failures
for i in {1..100}; do
  pytest tests/test_thing.py::test_flaky && echo PASS || echo FAIL
done | sort | uniq -c

# Or with test framework support
pytest --count=100 tests/test_thing.py::test_flaky
cargo test test_flaky -- --test-threads=1 --nocapture 2>&1 | tee /tmp/runs.log
```

**Interpret the rate:**
- 1-5%: Rare race condition, timing-sensitive
- 10-30%: Moderate flakiness, likely reproducible with stress
- 50%+: Severe, probably test logic issue or missing setup

### Reproduce CI Conditions

```bash
# Match CI parallelism
pytest -n 8 tests/  # parallel like CI

# Match CI resource constraints
# Docker with limited resources
docker run --cpus=2 --memory=2g ...

# Run full suite (not just the flaky test)
# Flakiness might depend on other tests running first
```

## Phase 2: Isolate

### Test Order Dependence

```bash
# Run flaky test in isolation
pytest tests/test_thing.py::test_flaky

# Run with specific tests before it
pytest tests/test_other.py tests/test_thing.py::test_flaky

# Randomize order to find dependencies
pytest --random-order tests/

# Bisect: which preceding test causes failure?
# Binary search through test list
```

**If order-dependent**: Shared state - database, global variables, singletons, env vars.

### Timing Sensitivity

```bash
# Add artificial delays/stress
# Run with high parallelism
pytest -n 16 tests/test_thing.py::test_flaky --count=50

# Run on loaded system
stress --cpu 8 &
pytest tests/test_thing.py::test_flaky --count=50
kill %1

# Run with slow I/O
# (filesystem, network simulation)
```

**If timing-sensitive**: Race conditions - missing synchronization, async without await, polling without timeout.

### Environment Variables

```bash
# Check what's different in CI
diff <(env | sort) <(ssh ci-runner env | sort)

# Key variables to check
TZ, LANG, LC_ALL, HOME, TMPDIR, PATH
DATABASE_URL, API_KEY, etc.

# Time zone issues
TZ=UTC pytest tests/test_thing.py::test_flaky
TZ=America/Los_Angeles pytest tests/test_thing.py::test_flaky
```

### Resource Exhaustion

```bash
# Check for leaked resources
lsof -p $(pgrep -f pytest) | wc -l  # file handles
netstat -tlnp | grep python          # ports

# Run tests that might leak, then the flaky one
pytest tests/test_resource_heavy.py tests/test_thing.py::test_flaky
```

## Phase 3: Diagnosis Techniques

### Add Strategic Logging

```python
# DON'T just add print() everywhere - changes timing!
# Instead, collect state at key points

import logging
logging.basicConfig(level=logging.DEBUG)

def test_flaky():
    state_before = capture_relevant_state()
    logging.debug(f"State before: {state_before}")

    result = do_thing()

    state_after = capture_relevant_state()
    logging.debug(f"State after: {state_after}")

    # Log on failure only
    if result != expected:
        logging.error(f"Mismatch! Before: {state_before}, After: {state_after}")
```

### Time Travel Debugging

For time-dependent tests:

```python
# Use freezegun or similar
from freezegun import freeze_time

@freeze_time("2024-01-15 23:59:59")
def test_near_midnight():
    ...

@freeze_time("2024-02-29")  # Leap year edge case
def test_leap_year():
    ...
```

### Seed Control for Randomness

```python
# Fix random seed for reproducibility
import random
random.seed(12345)

# Or capture seed on failure
seed = random.randint(0, 2**32)
random.seed(seed)
try:
    run_test()
except AssertionError:
    print(f"Failed with seed: {seed}")  # Can reproduce!
    raise
```

### Race Condition Detection

**Language-specific race detectors** - these are extremely powerful:

```bash
# C/C++: ThreadSanitizer (TSan)
clang -fsanitize=thread -g test.c -o test
./test  # Reports races with stack traces

# Go: Built-in race detector
go test -race ./...
go run -race main.go

# Rust: Miri (interpreter with UB detection)
cargo +nightly miri test

# Rust: Loom (exhaustive concurrency testing)
# Explores all possible thread interleavings
# https://github.com/tokio-rs/loom

# Java: Find thread-safety issues
# Use -XX:+UseThreadPriorities or tools like jcstress

# Python: No built-in, but can use manual detection
```

**Manual race detection** (when tools unavailable):

```python
import threading

class RaceDetector:
    def __init__(self):
        self.holder = None
        self.lock = threading.Lock()

    def acquire(self, name):
        with self.lock:
            if self.holder:
                raise RuntimeError(f"Race! {name} vs {self.holder}")
            self.holder = name

    def release(self):
        with self.lock:
            self.holder = None
```

**Deterministic simulation testing** - [Antithesis](https://antithesis.com/) takes a different approach: run your system in a deterministic simulator that explores all possible interleavings. When it finds a bug, it's reproducible by replaying the same execution. This sidesteps the "can't reproduce" problem entirely. Worth considering for critical systems.

### Git Bisect for Regression

```bash
# Find which commit introduced the flakiness
git bisect start
git bisect bad HEAD
git bisect good v1.0.0

# Automated bisect with test script
git bisect run bash -c 'for i in {1..20}; do pytest tests/test_flaky.py || exit 1; done'
```

## Common Fixes by Category

### Timing/Race Conditions

```python
# BAD: Polling without proper wait
while not condition:
    pass  # Spin forever or miss it

# GOOD: Explicit wait with timeout
import time
deadline = time.time() + 10.0  # 10 second timeout
while not condition:
    if time.time() > deadline:
        raise TimeoutError("Condition not met")
    time.sleep(0.1)

# BETTER: Event-based waiting
event.wait(timeout=10.0)
```

### State Pollution

```python
# BAD: Global state modified by tests
cache = {}

def test_a():
    cache['key'] = 'value_a'

def test_b():
    assert 'key' not in cache  # Fails if test_a runs first!

# GOOD: Reset state in fixtures
@pytest.fixture(autouse=True)
def reset_cache():
    cache.clear()
    yield
    cache.clear()
```

### External Dependencies

```python
# BAD: Direct network call
def test_api():
    response = requests.get("https://api.example.com/data")
    assert response.json()['status'] == 'ok'

# GOOD: Mock external dependencies
@mock.patch('requests.get')
def test_api(mock_get):
    mock_get.return_value.json.return_value = {'status': 'ok'}
    response = requests.get("https://api.example.com/data")
    assert response.json()['status'] == 'ok'
```

### Resource Exhaustion

```python
# BAD: Resource leak
def test_file():
    f = open('/tmp/test.txt', 'w')
    f.write('data')
    # f never closed!

# GOOD: Context manager ensures cleanup
def test_file():
    with open('/tmp/test.txt', 'w') as f:
        f.write('data')
```

## Verification

After fixing:

```bash
# Run many times to verify fix
pytest tests/test_thing.py::test_flaky --count=500

# Run under stress
pytest -n 16 tests/test_thing.py::test_flaky --count=100

# Run full suite with flaky test multiple times
for i in {1..10}; do pytest tests/; done
```

**Flake rate should go from N% to 0%**, not just "lower".

## CI Considerations

### Diagnosis in CI

CI often surfaces flakiness that doesn't appear locally. Use CI for diagnosis:

```yaml
# GitHub Actions: Matrix to stress test
jobs:
  flaky-diagnosis:
    strategy:
      matrix:
        run: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
    steps:
      - run: pytest tests/test_flaky.py -v
```

```yaml
# Capture detailed logs on failure
- run: pytest tests/ -v --tb=long 2>&1 | tee test-output.log
- uses: actions/upload-artifact@v3
  if: failure()
  with:
    name: test-logs-${{ matrix.run }}
    path: test-output.log
```

### What CI Can Tell You

- **Fails only in CI**: Environment difference (parallelism, resources, timing)
- **Fails on specific runner OS**: Platform-specific behavior
- **Fails at specific time**: Time-dependent code, expiring credentials
- **Fails after other jobs**: Shared state (database, cache, artifacts)

### Bandaids vs Fixes

These are **bandaids** - they hide the problem, don't fix it:

| Bandaid | Why It's Bad |
|---------|--------------|
| Auto-retry failed tests | Masks flakiness, wastes CI time, gives false confidence |
| Run flaky tests N times | Same - if it passes once, you call it good |
| Quarantine/skip flaky tests | Technical debt, test coverage gap |
| Reduce parallelism | Bug still exists, will surface elsewhere |
| Add generous sleep() | Slows tests, race still exists at different timing |

These are **actual fixes**:
- Find and fix the race condition
- Properly isolate test state
- Mock external dependencies
- Use deterministic simulation (Antithesis)
- Delete the test if it's not testing anything meaningful

### Flaky Test Tracking

If you have many flaky tests, track them:

```
# Simple: grep CI logs
grep -l "FLAKY\|RETRY\|flaky" ci-logs/*.log | wc -l

# Better: structured tracking
# - Test name
# - Flake rate (failures / total runs)
# - When it started (git bisect)
# - Category (timing, state, external, etc.)
# - Owner / last touched by
```

Dashboards help, but the goal is **zero flaky tests**, not "managed flaky tests".

## LLM-Specific Techniques

For LLM-driven debugging:

1. **Collect failure logs** - Parse CI logs for patterns:
   ```bash
   grep -A 20 "FAILED test_flaky" ci-logs/*.log > failures.txt
   ```

2. **Diff passing vs failing runs** - What's different?
   ```bash
   diff passing_run.log failing_run.log
   ```

3. **Search for anti-patterns** - Known flaky patterns:
   ```bash
   grep -r "time.sleep" tests/       # Hardcoded waits
   grep -r "global " tests/          # Global state
   grep -r "random\." tests/         # Unseeded randomness
   ```

4. **Static analysis** - Find shared state:
   ```bash
   # Module-level variables that tests might share
   grep -r "^[a-z_]* = " tests/ | grep -v "def \|#"
   ```

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| Can't reproduce | 0 failures in 1000 runs | Get exact CI environment, check for CI-specific code paths |
| Fixed wrong thing | Flakiness returns | Dig deeper, was masking another issue |
| Fix introduces new flakiness | Different test now flaky | Check for cascading dependencies |
| "Fixed" by adding sleep | Still flaky under load | Remove sleep, fix actual race |

## Anti-patterns

- **"Just retry"**: Masks the problem, wastes CI time, erodes trust
- **Skip flaky tests**: Technical debt accumulation
- **Add sleep()**: Doesn't fix races, just makes them rarer
- **Blame the framework**: Usually your code (but check framework issues)
- **Fix by reducing parallelism**: Hides the bug, will resurface

## Prevention

1. **Run tests in random order** from day one
2. **Use fixtures for setup/teardown**, not test code
3. **Mock external dependencies** by default
4. **Set explicit timeouts** on all async operations
5. **Seed randomness** in tests
6. **Run CI with high parallelism** to surface races early

## Open Questions

### Heisenbug Handling

Some bugs disappear when you try to observe them - adding logging changes timing enough to "fix" the race. Techniques like strategic logging help, but:

- Are Heisenbugs fundamentally different from regular race conditions?
- Can they be debugged systematically, or only via deterministic replay (Antithesis)?
- What real-world Heisenbugs exist that couldn't be diagnosed with current techniques?

Needs research: collect real-world Heisenbug case studies to see if patterns emerge.

### Flakiness in Distributed Systems

Multi-node tests add complexity:
- Network partitions (real or simulated)
- Clock skew between nodes
- Non-deterministic leader election
- Message ordering

Tools like Jepsen test for these, but debugging failures is harder than single-process.

### Property-Based Testing Integration

Property-based testing (Hypothesis, QuickCheck) can surface edge cases, but:
- Failures may not reproduce without the same seed
- Shrinking can change timing characteristics
- How to integrate with flaky test debugging?

## See Also

- [Bug Investigation](bug-investigation.md) - General debugging workflow
- [Bug Fix](bug-fix.md) - Once cause is found
