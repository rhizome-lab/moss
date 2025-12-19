# Async Task Management

Guide for managing background tasks: terminals, shells, agents running asynchronously.

## Task Types

| Type | Use Case | Lifecycle |
|------|----------|-----------|
| Terminal | Long-running commands (servers, watch mode) | Start → Monitor → Kill |
| Shell | One-off commands | Start → Wait → Collect output |
| Agent/Worker | Parallel subtasks | Spawn → Execute → Merge |

## Spawning Tasks

### Background Shells

```python
import asyncio
import subprocess

# Fire-and-forget
process = subprocess.Popen(
    ["npm", "run", "dev"],
    stdout=subprocess.PIPE,
    stderr=subprocess.STDOUT,
)

# Async subprocess
async def run_background():
    proc = await asyncio.create_subprocess_exec(
        "npm", "run", "dev",
        stdout=asyncio.subprocess.PIPE,
    )
    return proc
```

### Parallel Workers (agents.py)

```python
from moss.agents import Manager, Worker, Ticket
from moss.shadow_git import ShadowGit

# Create manager
shadow_git = ShadowGit.init(project_root)
manager = Manager(shadow_git)

# Spawn workers for parallel tasks
tickets = [
    manager.create_ticket("Refactor module A"),
    manager.create_ticket("Refactor module B"),
]

# Delegate to workers (each gets own git branch)
workers = [Worker(shadow_git) for _ in tickets]
for ticket, worker in zip(tickets, workers):
    await manager.delegate(ticket, worker)

# Workers run in parallel - don't need to join immediately
```

## Waiting Patterns

### Wait with Timeout

```python
async def wait_with_timeout(proc, timeout_sec=30):
    try:
        stdout, _ = await asyncio.wait_for(
            proc.communicate(),
            timeout=timeout_sec,
        )
        return stdout
    except asyncio.TimeoutError:
        proc.kill()
        raise
```

### Wait for Any

```python
async def wait_for_first(tasks):
    """Return when any task completes."""
    done, pending = await asyncio.wait(
        tasks,
        return_when=asyncio.FIRST_COMPLETED,
    )
    return done.pop().result()
```

### Poll without Blocking

```python
def check_done(process):
    """Non-blocking check."""
    return process.poll() is not None
```

## Handling Hangs

Tasks can hang. Detection strategies:

### 1. Output Timeout

```python
async def read_with_timeout(stream, timeout=30):
    """Detect hang via lack of output."""
    try:
        line = await asyncio.wait_for(
            stream.readline(),
            timeout=timeout,
        )
        return line
    except asyncio.TimeoutError:
        return None  # Likely hung
```

### 2. Progress Detection

```python
class ProgressMonitor:
    def __init__(self, timeout_sec=60):
        self.last_activity = time.time()
        self.timeout_sec = timeout_sec

    def record_activity(self):
        self.last_activity = time.time()

    def is_stalled(self):
        return time.time() - self.last_activity > self.timeout_sec
```

### 3. Graceful Cancellation

```python
async def graceful_kill(proc, grace_period=5):
    """SIGTERM, then SIGKILL after grace period."""
    proc.terminate()
    try:
        await asyncio.wait_for(proc.wait(), timeout=grace_period)
    except asyncio.TimeoutError:
        proc.kill()
        await proc.wait()
```

### Caveats

Not 100% reliable:
- Servers are long-running without output (intentional)
- Build tools may have long compilation phases
- Network operations can stall legitimately

Use domain knowledge:
```python
KNOWN_LONG_RUNNING = {"npm run dev", "python -m http.server", "uvicorn"}

def expected_timeout(command):
    if any(cmd in command for cmd in KNOWN_LONG_RUNNING):
        return None  # No timeout for servers
    return 120  # 2 min for normal commands
```

## Multiple Concurrent Agents

Agents don't need to join back to main stream immediately.

### Fire and Forget

```python
# Start agent on subtask
worker.spawn(ticket)

# Continue with main work
do_main_work()

# Check later (or never)
if worker.status == WorkerStatus.COMPLETE:
    result = worker.current_ticket
```

### Merge When Ready

```python
# Work continues while agents run
completed_results = []

for worker in workers:
    if worker.status == WorkerStatus.COMPLETE:
        result = await manager.merge(worker.current_ticket.result)
        completed_results.append(result)
    # Skip incomplete workers - check again later
```

### Don't Block on Completed Tasks

Main work may have moved on:

```python
# BAD: Blocking on all workers
results = await asyncio.gather(*[w.execute() for w in workers])

# GOOD: Non-blocking collection
async def collect_completed(workers):
    results = []
    for w in workers:
        if w.status == WorkerStatus.COMPLETE:
            results.append(w.result)
    return results

# Main thread continues
main_result = do_main_work()
agent_results = await collect_completed(workers)
# Use what's available, ignore stragglers
```

## When to Join vs Let Run

| Scenario | Strategy |
|----------|----------|
| Need result to continue | Wait with timeout |
| Result is nice-to-have | Fire and forget, check later |
| Multiple independent subtasks | Parallel, merge as completed |
| Long-running server | Start, don't join, kill on exit |
| One-off command | Wait for completion |

### Decision Tree

```
Need result now?
├─ Yes → Wait with timeout
│         └─ Hung? → Kill, retry or fail
└─ No → Fire and forget
         └─ Check periodically?
              ├─ Yes → Poll status
              └─ No → Ignore until cleanup
```

## Cleanup

Always clean up on exit:

```python
import atexit
import signal

active_processes = []

def cleanup():
    for proc in active_processes:
        if proc.poll() is None:
            proc.terminate()

atexit.register(cleanup)
signal.signal(signal.SIGTERM, lambda *_: cleanup())
```

## Integration with AgentLoop

```python
from moss.agent_loop import AgentLoopRunner, CompositeToolExecutor

# Run loop in background
async def run_loop_background(loop, input_data):
    runner = AgentLoopRunner(executor)
    task = asyncio.create_task(runner.run(loop, input_data))
    return task  # Don't await - caller decides when to wait

# Main code
task = await run_loop_background(analysis_loop, file_path)

# Do other work...
main_result = process_files()

# Now check if analysis done
if task.done():
    analysis = task.result()
else:
    task.cancel()  # Don't need it anymore
```

## Summary

1. **Spawn appropriately**: Use right abstraction (subprocess, asyncio, Worker)
2. **Set timeouts**: Prevent indefinite hangs
3. **Detect stalls**: Monitor output/progress
4. **Cancel gracefully**: SIGTERM before SIGKILL
5. **Don't over-join**: Let independent tasks run free
6. **Clean up**: Kill stragglers on exit
