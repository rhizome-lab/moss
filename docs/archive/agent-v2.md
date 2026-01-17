# Agent V2 Design

Goal state for `moss @agent` - autonomous coding agent.

## Core Principles

1. **Do what's requested** - no artificial limits on autonomy or scope
2. **Universal** - CLI, IDE, CI all use the same agent
3. **Configurable** - project-specific behavior via `[agent]` config
4. **Recoverable** - retry → rollback → ask user escalation

## Session Continuity

Agent can run indefinitely. When context fills or session ends:
1. Summarize progress and open questions
2. Write checkpoint to `.moss/agent/session-<id>.json`
3. New session picks up from checkpoint

Stops ONLY when:
- Blocked on design decisions (needs user input)
- Task is complete
- User explicitly stops

## Validation Pipeline

Configurable in `.moss/config.toml` - fully arbitrary schema:

```toml
[agent]
# User-defined validation - agent is just a Lua script, fully extensible
validate = ["typecheck", "test", "lint"]
validate_commands = [
  { name = "typecheck", run = "cargo check" },
  { name = "test", run = "cargo test" },
  { name = "lint", run = "cargo clippy" },
]
validate_on = "edit"

# Users can add any fields they want - agent.lua reads config freely
my_custom_option = "whatever"
```

No predefined schema - agent is just Lua, users write their own if defaults don't fit.

## Context Management

Commands the agent can use:

| Command | Effect |
|---------|--------|
| `$(keep N)` | Retain output N in working memory |
| `$(note <fact>)` | Record synthesized insight |
| `$(drop N)` | Remove output N from context (NEW) |
| `$(forget <pattern>)` | Remove notes matching pattern (NEW) |

Context is ephemeral by default - outputs shown once then gone unless kept.

## Error Recovery

Escalation chain:
1. **Retry** - same approach, maybe different parameters
2. **Rollback** - undo to last known good state (shadow git)
3. **Ask user** - blocked, need guidance

```
[error] cargo test failed
[retry 1/3] Running tests again...
[retry 2/3] Adjusting approach...
[retry 3/3] Final attempt...
[rollback] Reverting to pre-edit state
[blocked] Need help: tests still failing after rollback. Options:
  1. Skip this test for now
  2. Show me the test code
  3. Explain what you were trying to do
```

## User Input During Session

Problem: Claude Code often ignores user messages mid-task.

Solution:
1. **Acknowledge immediately** - "Got it, will address after current step"
2. **Queue if unrelated** - don't let unrelated input pollute task context
3. **Interrupt if urgent** - keywords like "stop", "abort", "wait"
4. **Dispose after handling** - unrelated queries don't persist in task context

Prompt pattern:
```
If user sends a message while you're working:
- If related to current task: incorporate immediately
- If unrelated: handle briefly, then return to task (don't add to task context)
- If "stop"/"abort"/"wait": pause and await further instruction
```

## Integration Points

| Surface | How |
|---------|-----|
| CLI | `moss @agent "task"` |
| IDE | Extension calls CLI, streams output |
| CI | `moss @agent --non-interactive --task-file tasks.md` |
| MCP | Agent as MCP server for other tools |

## Multi-Agent Coordination

Agents can spawn sub-agents for subtasks. Coordination required:

```lua
-- Parent spawns child for subtask
local child = agent.spawn("refactor auth module")
child:wait()  -- blocks until child done

-- Or async with locks
local lock = agent.lock("src/auth/")  -- claims ownership of path
-- ... do work ...
lock:release()
```

Conflict avoidance:
- **Locks** - agent claims paths it's editing, siblings wait
- **Pause** - parent can pause children, siblings can request pause
- **Queue** - edits to locked paths queue until lock released

Lock granularity: configurable per-lock (agent decides based on task)
- `agent.lock("src/auth/")` - directory
- `agent.lock("src/auth/jwt.rs")` - file
- `agent.lock("src/auth/jwt.rs/verify")` - symbol

## Long-Term Memory

Short-term: `$(keep)`, `$(note)`, `$(drop)`, `$(forget)` - within session

Long-term: `memorize` tool for cross-session persistence

```lua
memorize("auth module uses JWT tokens, see src/auth/jwt.rs")
memorize("user prefers functional style over OOP")

-- Stored in .moss/memory/ (version controlled)
-- Accessible via recall()
```

Memory storage: `.moss/memory/` directory, checked into git
- `facts.md` - general project knowledge
- `preferences.md` - user/team preferences
- `decisions.md` - past design decisions and rationale

Note: `moss init` gitignore must NOT exclude `.moss/memory/` (unlike other `.moss/` contents)

Organization: both manual and LLM-assisted
- User can edit `.moss/memory/*.md` directly
- Agent can reorganize/consolidate via LLM when prompted
- `moss memory organize` - LLM pass to dedupe/categorize

## Implemented

- [x] Context commands: $(keep), $(note), $(drop), $(forget), $(memorize)
- [x] Session checkpoints: $(checkpoint), --resume, --list-sessions
- [x] Error escalation: retry → rollback → ask user (automatic after 3 failures)
- [x] Working memory IDs: random 4-char strings to avoid LLM autocomplete
- [x] Batch edit: $(batch-edit target1 action content | target2 action content)
- [x] Session logging: JSONL logs in .moss/agent/logs/, --list-logs to view
- [x] Non-interactive mode: --non-interactive / -n for CI usage

## Open Questions

1. **Parallel validation** - shadow worktree configurable, default OFF (complexity vs benefit)
2. **Memory heuristics** - what to auto-memorize vs explicit `memorize` only?
3. **Sub-agent error attribution** - when validation fails, which sub-agent caused it? Need tracing/blame mechanism

## Success Criteria

- Agent completes multi-file refactors without intervention
- Graceful degradation when stuck (clear error, options presented)
- Session continuity works (can resume after context fills)
- User input acknowledged and handled appropriately
- Works in CI (non-interactive mode)
