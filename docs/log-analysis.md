# Claude Code Session Log Analysis

Analysis of session logs from `~/.claude/projects/-home-me-git-moss/`.

## Session Overview

Main session: `6cb78e3b-873f-4a1f-aebe-c98d701052e3.jsonl` (59MB)
Subagents: 15+ agent files (200KB-500KB each)

## Message Types

| Type | Count |
|------|-------|
| assistant | 8512 |
| user | 3918 |
| file-history-snapshot | 319 |
| queue-operation | 110 |
| system | 41 |
| summary | 1 |

## Tool Usage

| Tool | Calls | Errors | Success Rate |
|------|-------|--------|--------------|
| Bash | 1,372 | 234 | 83% |
| Edit | 990 | 7 | 99% |
| Read | 644 | 4 | 99% |
| TodoWrite | 286 | 0 | 100% |
| Write | 253 | 9 | 96% |
| Grep | 221 | 0 | 100% |
| Glob | 40 | 0 | 100% |
| Task | 12 | 0 | 100% |
| Other | 35 | 3 | 91% |

**Total**: 3,853 tool calls, 257 errors (93% success rate)

*Note: "File not read yet" errors are attributed to Write (the tool that checks), not Edit.*

## Bash Command Patterns

Most common command prefixes:
- `uv` (550): Running tests, python scripts, package management
- `ruff` (331): Linting and formatting
- `git` (314): Version control
- `grep` (41): Text search (despite Grep tool!)
- `nix` (12): Development environment

## Error Analysis

### Bash Errors (234 total)

Exit codes:
- Exit code 1: 222 (general failure)
- Exit code 2: 7 (misuse)
- Exit code 124: 4 (timeout)
- Exit code 127: 1 (command not found)

Common error categories:
1. **Test failures** (pytest): Test assertions failing during development
2. **Lint errors** (ruff): Code style violations caught during check
3. **Build errors**: Missing dependencies, import errors
4. **LSP server tests**: Specific test file with persistent failures

### File Operation Errors (20 total)

| Error | Count | Cause |
|-------|-------|-------|
| File not read yet | 9 | Write/Edit before Read |
| File does not exist | 4 | Read/Write to missing file |
| Found N matches | 3 | Edit string not unique |
| String not found | 2 | Edit target changed |
| File modified since read | 2 | Linter changed file |

### Other Errors (3 total)

- 2: User plan refinements (interactive editing, not rejections)
- 1: KillShell on already-completed shell

### User Rejections

The 2 "user doesn't want to proceed" errors were actually **plan refinements** -
the user was editing/improving proposed plans via Claude Code's interactive mode,
not rejecting work outright.

## Observations

### Patterns

1. **High Bash usage for testing**: Running `uv run pytest` frequently, which is expected during development
2. **Linting integrated into workflow**: `ruff check` and `ruff format` called 331 times
3. **Git operations frequent**: 314 git commands for commits, status checks
4. **TodoWrite heavily used**: 285 calls, showing good task tracking discipline

### Potential Inefficiencies

1. **Using grep in Bash** (41 calls) when Grep tool exists and is optimized
2. **File read-before-write errors** (9 cases): Could be addressed with better tool sequencing
3. **Non-unique Edit strings** (3 cases): Need more context in edit operations
4. **Low parallel tool usage**: 99.95% of turns have only 1 content block - could parallelize more

### Turn Patterns

| Content Blocks | Count |
|----------------|-------|
| 1 | 8,562 |
| 2 | 2 |
| 3 | 2 |

Almost all turns are sequential (1 tool call at a time). There's significant opportunity
to parallelize independent operations like:
- Reading multiple files at once
- Running git status + git diff in parallel
- Multiple Grep searches

### What's Working Well

1. **Edit success rate 99%**: Very reliable file editing
2. **Read success rate 99%**: File reading rarely fails
3. **TodoWrite 100% success**: Task tracking working flawlessly
4. **Subagent usage**: Only 12 Task calls, showing restraint in spawning subagents
5. **Subagents are focused**: Mostly use Read/Grep/Glob for exploration, minimal Bash

### Subagent Patterns

15 subagents spawned, tool usage shows they're used for research:
- Read: Primary tool (11-27 calls per agent)
- Grep: Secondary (1-7 calls)
- Glob: File finding (1-4 calls)
- Bash: Minimal (0-10 calls, usually for running code)

Subagents don't edit files - they gather information for the main session.

## Recommendations

### For Claude Code Users

1. **Prefer Grep tool over bash grep**: More integrated, better error handling
2. **Always Read before Write/Edit**: Avoid the 9 "not read yet" errors
3. **Provide more context in Edit**: Avoid non-unique string matches
4. **Parallelize independent operations**: Read multiple files, run multiple commands in one turn

### For This Project

1. **LSP tests need attention**: Multiple failures in test_lsp_server.py
2. **Consider log analysis tooling**: This manual analysis could be automated as a `moss` command

## Future Work

Could build a `moss analyze-logs` command that:
- Parses Claude Code session logs
- Computes tool success rates
- Identifies error patterns
- Suggests workflow improvements
- Estimates token costs

This would help users understand their agent interaction patterns.

## Token Usage

**Note**: Log entries are duplicated per streaming chunk. Numbers below use unique requestId grouping.

| Metric | Count |
|--------|-------|
| Unique API Calls | 3,716 |
| New Input Tokens | 33K |
| Cache Creation | 7.4M |
| Cache Read | 362M |
| Output Tokens | 1.45M |
| Total Context Processed | 372M |

### Context Size Distribution

| Metric | Tokens |
|--------|--------|
| Minimum context | 19K |
| Maximum context | 156K |
| Average context | 100K |

Context grew from ~19K to ~156K tokens as the session progressed over 31 hours.

### Cost Estimate (Opus 4.5 API rates)

| Component | Tokens | Rate | Cost |
|-----------|--------|------|------|
| Cache read | 362M | $1.50/M | $543 |
| Cache create | 7.4M | $18.75/M | $139 |
| New input | 33K | $15/M | $0.50 |
| Output | 1.45M | $75/M | $109 |
| **Total** | | | **~$791** |

*Note: Claude Code Pro subscription ($100-200/mo) provides unlimited usage, so actual user cost
depends on subscription tier, not raw API rates.*

### Data Deduplication Note

The raw log has 8,500+ assistant entries because each streaming chunk is logged separately.
Grouping by `requestId` and taking max values gives the correct per-call totals.

---

*Analysis performed on session from Dec 17-18, 2025*
