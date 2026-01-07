# Debugging Production Issues Workflow

Diagnosing problems in production when you can't reproduce locally - working from logs, traces, and metrics without direct debugging access.

## Trigger

- Alert fires: errors, latency, resource exhaustion
- Users report issues you can't reproduce
- Monitoring shows anomalies
- On-call incident

## Goal

- Identify root cause from available observability data
- Fix or mitigate without causing additional production impact
- Build reproducer for thorough testing
- Prevent recurrence (better monitoring, tests)

## Prerequisites

- Access to logs, metrics, traces
- Understanding of system architecture
- Deployment/change history
- Ability to deploy fixes (ideally with canary/rollback)

## Why Production Debugging Is Hard

1. **No debugger**: Can't set breakpoints in production
2. **State is gone**: By the time you investigate, the problematic state has changed
3. **Sampling**: Logs/traces may not capture the exact failing request
4. **Noise**: Thousands of normal events obscure the anomaly
5. **Pressure**: Users are affected, need to fix fast
6. **Fear of making it worse**: Debug code might cause more problems

## Observability Stack

What you typically have to work with:

| Source | What It Shows | Limitations |
|--------|---------------|-------------|
| **Logs** | Event messages, errors, context | Incomplete, may miss root cause |
| **Metrics** | Aggregated numbers (latency, errors, throughput) | No individual request detail |
| **Traces** | Request flow across services | Sampled, may miss the bad request |
| **APM** | Code-level profiling | Overhead limits detail |
| **Error tracking** | Stack traces, frequency | After-the-fact, state lost |
| **Alerts** | Something is wrong | No why |

## Core Strategy: Scope → Correlate → Hypothesize → Verify

```
┌─────────────────────────────────────────────────────────┐
│                      SCOPE                               │
│  When did it start? What's affected? What changed?      │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                    CORRELATE                             │
│  What else changed at the same time? Deploys?           │
│  Traffic patterns? Dependency issues?                   │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                   HYPOTHESIZE                            │
│  Based on evidence, what could cause this?              │
│  Rank by likelihood and testability                     │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                     VERIFY                               │
│  Test hypothesis without breaking prod further          │
│  Build reproducer, fix, validate                        │
└─────────────────────────────────────────────────────────┘
```

## Phase 1: Scope the Problem

### Define the Boundaries

```
Questions:
- When did it start? (exact timestamp)
- What's the impact? (error rate, latency, affected users)
- What's affected? (one endpoint, one service, everything?)
- Is it intermittent or constant?
- Is it getting worse?
```

### Timeline Construction

```bash
# Build a timeline
| Time     | Event                              |
|----------|-------------------------------------|
| 14:00:00 | Deploy v1.2.3 to production         |
| 14:05:23 | First error in logs                 |
| 14:07:00 | Error rate crosses alert threshold  |
| 14:07:15 | PagerDuty alert fires               |
| 14:10:00 | You start investigating             |

# The 5-minute gap between deploy and first error is suspicious
```

### Check What Changed

```bash
# Recent deploys
kubectl rollout history deployment/myapp
git log --oneline --since="2 hours ago"

# Config changes
kubectl get configmap myapp-config -o yaml
git log --oneline -- config/

# Infrastructure changes
# Check Terraform/Pulumi state, cloud console, etc.

# Dependency status
# Check status pages for your dependencies (AWS, Stripe, etc.)
```

## Phase 2: Correlate

### Cross-Reference Data Sources

```
If error rate spiked at 14:05:
- What do logs show at 14:05?
- What do traces show for requests at 14:05?
- What do metrics show? (CPU, memory, connections, queues)
- What deploys happened just before?
- Any dependency outages at that time?
```

### Common Correlations

| Correlation | Likely Cause |
|-------------|--------------|
| Error spike after deploy | Bug in new code |
| Error spike + no deploy | Dependency issue, traffic spike, data issue |
| Latency spike + CPU spike | Inefficient code path activated |
| Errors only for some users | Data-dependent bug, feature flag |
| Errors only from one region | Infrastructure issue, config drift |

### Query Patterns

```bash
# Logs: Find errors in time window
grep -E "ERROR|Exception|FATAL" app.log | grep "14:0[5-9]"

# Or with structured logging (JSON)
jq 'select(.level == "error" and .timestamp >= "14:05:00")' logs.jsonl

# Metrics: Compare before/after
# In Grafana, Datadog, etc.: query rate(errors) at T-10m vs T

# Traces: Find slow/failed requests
# In Jaeger/Zipkin: filter by error=true, timestamp in range
```

## Phase 3: Hypothesize

### Generate Hypotheses

Based on evidence, list possible causes:

```
Evidence:
- Errors started 5 min after deploy
- Errors are "connection refused" to database
- Database metrics look normal
- Only affecting write operations

Hypotheses (ranked by likelihood):
1. New code has connection pool misconfiguration (high - correlates with deploy)
2. Database connection limit reached (medium - but DB metrics look ok)
3. Network issue between app and DB (low - would affect reads too)
```

### Check Common Causes First

```
Most production issues are caused by:
1. Recent deploys (code bugs, config bugs)
2. Traffic changes (overload, unusual patterns)
3. Dependency issues (databases, APIs, cloud services)
4. Resource exhaustion (memory, connections, disk)
5. Data issues (bad input, migration problems)
6. Certificate/credential expiry

Check these before exotic theories.
```

### The "What Changed?" Principle

> If it was working and now it's not, something changed.
> Find the change.

```
Change categories:
- Your code (deploys, config)
- Your infrastructure (scaling events, restarts)
- Your dependencies (their deploys, their outages)
- Traffic (volume, patterns, sources)
- Time (cron jobs, expiry, timezone edge cases)
- External (attacks, compliance blocks, network changes)
```

## Phase 4: Verify

### Test Hypotheses Safely

```bash
# DON'T: Make changes to prod to "see what happens"
# DO: Test hypothesis with minimal impact

# Option 1: Read-only investigation
# Check connection pool metrics without changing anything
kubectl exec -it pod -- curl localhost:8080/metrics | grep pool

# Option 2: Canary test
# Deploy hypothesis fix to single instance, observe

# Option 3: Shadow traffic
# Replay traffic to test environment with fix

# Option 4: Local reproduction
# Build reproducer from logs, test locally
```

### Build a Reproducer

The gold standard: reproduce the issue locally.

```python
# From logs, extract:
# - The request that failed
# - The state at the time
# - The error that occurred

# Reproduce:
def test_reproduces_production_bug():
    # Setup: same state as production
    db.execute("INSERT INTO users VALUES (1, 'test', NULL)")  # NULL caused the bug

    # Action: same request
    response = client.post("/api/process", json={"user_id": 1})

    # Assert: same error
    assert response.status_code == 500
    assert "NullPointerException" in response.text
```

### Fix and Validate

```bash
# 1. Fix in code
git commit -m "Handle NULL user email in processor

Root cause: production user had NULL email (data migration issue)
Symptom: NullPointerException in email formatter
Fix: Check for NULL, use default placeholder"

# 2. Add test that catches this
# The reproducer becomes a regression test

# 3. Deploy with monitoring
# Watch error rate, ready to rollback

# 4. Post-incident review
# How did NULL email get there? Fix the data, fix the migration
```

## Common Production Issue Patterns

### Pattern: Works Locally, Fails in Prod

```
Causes:
- Environment differences (env vars, config)
- Data differences (prod has edge cases)
- Scale differences (works for 10 requests, not 10000)
- Dependency version differences
- Permission differences

Debug approach:
1. List all env vars in prod vs local
2. Get sample of actual prod data
3. Load test locally with prod-like volume
4. Check dependency versions exactly
```

### Pattern: Intermittent Failures

```
Causes:
- Race conditions (timing-dependent)
- Resource contention (only fails under load)
- External dependency flakiness
- Garbage collection pauses
- Connection pool exhaustion

Debug approach:
1. Correlate failures with load/timing
2. Check for patterns (every N minutes? only during peak?)
3. Look at resource metrics at failure times
4. Enable more detailed logging temporarily
```

### Pattern: Gradual Degradation

```
Causes:
- Memory leak (slow growth until OOM)
- Connection leak (pool slowly exhausts)
- Log/data growth (disk fills up)
- Cache pollution (hit rate declining)

Debug approach:
1. Graph metrics over time (not just instant)
2. Look for monotonic increases
3. Check when last restart was (did restart "fix" it?)
4. Profile memory/connections in staging under load
```

### Pattern: Specific Users Affected

```
Causes:
- Data-dependent bug (their data triggers it)
- Feature flag (they're in a bad cohort)
- Geographic (their region has issues)
- Account state (subscription, permissions)

Debug approach:
1. Get affected user IDs
2. Compare their data/state to working users
3. Check feature flag assignments
4. Check request routing (which servers, regions)
```

## Tools and Techniques

### Log Analysis

```bash
# Find error patterns
grep -h ERROR *.log | sort | uniq -c | sort -rn | head -20

# Find requests that errored
grep -B 10 "ERROR.*NullPointer" app.log

# JSON logs with jq
cat logs.jsonl | jq -s 'group_by(.error_type) | map({error: .[0].error_type, count: length})'

# Aggregate by time bucket
cat logs.jsonl | jq -r '.timestamp[:16]' | sort | uniq -c
# Shows errors per minute
```

### Metrics Queries

```promql
# Error rate spike
rate(http_requests_total{status=~"5.."}[5m])

# Latency percentiles
histogram_quantile(0.99, rate(http_request_duration_seconds_bucket[5m]))

# Compare to yesterday
rate(http_requests_total{status=~"5.."}[5m])
  / rate(http_requests_total{status=~"5.."}[5m] offset 1d)
```

### Distributed Tracing

```
In Jaeger/Zipkin/Datadog:

1. Filter: error=true, service=myapp, time=14:00-14:30
2. Find traces with errors
3. Look at span details:
   - Which service/operation failed?
   - What was the error message?
   - What were the request parameters?
   - What upstream services were called?
4. Compare to successful trace for same operation
```

### Quick Diagnostics

```bash
# Check process health
kubectl top pods
kubectl describe pod <pod> | grep -A 10 "Conditions"

# Check recent events
kubectl get events --sort-by=.lastTimestamp | tail -20

# Check resource limits
kubectl describe pod <pod> | grep -A 5 "Limits"

# Get a shell for investigation
kubectl exec -it <pod> -- /bin/sh
# Then: check /tmp, /var/log, running processes, open connections
```

## Emergency Mitigations

When you need to stop the bleeding before you understand the root cause:

| Mitigation | When to Use | Trade-offs |
|------------|-------------|------------|
| Rollback | Recent deploy caused it | Lose new features |
| Scale up | Resource exhaustion | Costs money, may not help |
| Feature flag off | New feature causing issues | Feature unavailable |
| Rate limit | Traffic spike | Some users affected |
| Restart | Unknown, "have you tried..." | May recur, loses state |
| Failover | Region/instance specific | Reduced capacity |

```bash
# Rollback deploy
kubectl rollout undo deployment/myapp

# Scale up
kubectl scale deployment/myapp --replicas=10

# Emergency feature flag (if you have one)
curl -X POST https://api.launchdarkly.com/flags/new-feature/disable
```

## LLM-Specific Techniques

LLMs can help analyze logs and correlate data:

### Log Summarization

```
Given 1000 lines of logs around the incident:
1. Extract unique error messages
2. Count frequency of each
3. Identify the first occurrence
4. Find patterns (same user? same endpoint? same data?)
```

### Hypothesis Generation

```
Given:
- Error message: "Connection refused to 10.0.1.5:5432"
- Timing: Started at 14:05, 5 min after deploy
- Context: Database connection, write operations only

Generate ranked hypotheses:
1. ...
2. ...

For each, suggest verification steps that don't impact production.
```

### Code Analysis

```
Given the stack trace and relevant code:
1. What code path was executing?
2. What could cause this exception?
3. What state would trigger this?
4. What changed in the recent deploy that touches this code?
```

## Post-Incident

After fixing:

### Document the Incident

```markdown
## Incident: Database Connection Errors
**Date**: 2024-03-15 14:05-14:45 UTC
**Duration**: 40 minutes
**Impact**: 15% of write requests failed

### Timeline
- 14:00: Deploy v1.2.3
- 14:05: First errors
- 14:07: Alert fired
- 14:15: Root cause identified
- 14:25: Fix deployed
- 14:45: Error rate returned to normal

### Root Cause
New code reduced connection pool size from 20 to 5 (typo in config).
Under load, pool exhausted, writes failed.

### Resolution
Reverted config change, added validation for pool size.

### Follow-up
- [ ] Add integration test for connection pool under load
- [ ] Add alert for connection pool utilization
- [ ] Review config change process
```

### Improve Detection

```yaml
# Add alert for the symptom
- alert: ConnectionPoolExhaustion
  expr: db_pool_available_connections < 2
  for: 1m
  labels:
    severity: warning
```

### Add Tests

```python
# The reproducer becomes a regression test
def test_handles_high_write_load():
    """Regression test for incident 2024-03-15"""
    with connection_pool(size=5):
        # Simulate concurrent writes
        with ThreadPoolExecutor(max_workers=20) as executor:
            futures = [executor.submit(write_to_db) for _ in range(100)]
            # Should not raise, should queue gracefully
            results = [f.result() for f in futures]
```

## Open Questions

### Observability Gaps

What should you instrument that often isn't?
- Business metrics (not just technical)
- Feature flag evaluations
- Cache hit rates
- Queue depths
- External dependency latency

### Log vs Trace vs Metric

When to use which?
- Logs: Debugging, after-the-fact investigation
- Traces: Understanding request flow, latency breakdown
- Metrics: Alerting, dashboards, trends

All three are needed; the question is sampling and retention trade-offs.

### Chaos Engineering

Should you deliberately cause production issues to test detection?
- Netflix Chaos Monkey approach
- Requires mature observability first
- Risk vs value calculation

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| Made prod worse while debugging | Metrics spike | Roll back debug changes |
| Misidentified root cause | Fix didn't help | Re-examine evidence |
| Fixed symptom, not cause | Issue recurs | Deeper investigation |
| Insufficient logging | Can't find evidence | Add logging, wait for next occurrence |

## Anti-patterns

- **Changing production to debug**: Add logging = deploy = risk
- **Tunnel vision**: Assuming you know the cause before looking at evidence
- **Blame-first**: "It's always the database" without checking
- **Cowboy fixes**: Pushing untested fixes under pressure
- **Incomplete post-mortem**: Fixing issue but not preventing recurrence

## See Also

- [Bug Investigation](bug-investigation.md) - General debugging, can reproduce locally
- [Flaky Test Debugging](flaky-test-debugging.md) - Similar statistical thinking
- [Performance Regression Hunting](performance-regression-hunting.md) - Production perf issues
