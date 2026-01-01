# Shadow Git Design

Auto-track edits made via `moss edit` for undo/redo capability.

## Problem

When `moss edit` modifies files, there's no easy way to undo changes. Users must rely on git or manual backups.

## Solution

Maintain a hidden git repository (`.moss/shadow/`) that automatically commits after each `moss edit` operation, preserving full edit history as a tree.

## Why Shadow Git?

**Primary goal: prevent catastrophic loss.** LLM-driven edits can be unpredictable. A single bad edit or a chain of "fixes" can destroy working code. Shadow git provides a safety net that doesn't rely on user discipline.

Use cases:
- **Oops recovery**: "That delete was wrong, undo it"
- **Experiment freely**: Try aggressive refactors knowing you can always go back
- **Audit trail**: See exactly what moss changed, when, and why
- **Checkpoint comparison**: "What did moss do since my last git commit?"
- **Partial rollback**: Undo specific hunks while keeping others

Design principles (see `docs/philosophy.md` for full context):
- **Never destroy history** - even undo preserves the undone state as a branch
- **Shadow is invisible until needed** - zero friction for normal workflow
- **Real git is source of truth** - shadow serves the gap between edits and commits

## Core Features

### Automatic Tracking
- Every `moss edit` operation creates a shadow commit
- Workflow-driven edits (`moss @workflow`) also tracked, with workflow name as context
- Commit message includes: operation, target, timestamp, optional user message
- Only tracks files modified by moss, not external changes

### Edit Messages
```bash
moss edit src/foo.rs/bar delete --message "Removing deprecated function"
moss edit src/foo.rs/bar delete --reason "Removing deprecated function"  # alias
```

Optional `--message` (or `--reason`) flag attaches a description to the edit, displayed in history and undo output.

### Undo/Redo
```bash
moss edit --undo              # Revert last moss edit, prints what was undone
moss edit --undo 3            # Revert last 3 edits, prints summary of each
moss edit --undo --dry-run    # Preview what would be undone
moss edit --redo              # Re-apply last undone edit
moss edit --goto <ref>        # Jump to specific commit
```

### History (read-only)
```bash
moss history                  # Show recent moss edits
moss history src/foo.rs       # Show edits for specific file
moss history --all            # Show full tree structure
moss history --json           # Machine-readable output (for LLM/scripting)
moss history --status         # Uncommitted shadow edits since last git commit
moss history --diff <ref>     # Show what a commit changed
moss history --diff 2         # Diff for commit 2
```

JSON output example (`moss history --json`):
```json
{
  "head": 3,
  "checkpoint": "abc123",
  "edits": [
    {
      "id": 3,
      "operation": "insert",
      "target": "src/foo.rs/new_fn",
      "files": ["src/foo.rs"],
      "message": null,
      "workflow": null,
      "git_head": "abc123",
      "timestamp": "2025-01-15T10:30:00Z",
      "parent": 0,
      "children": []
    },
    {
      "id": 2,
      "operation": "rename",
      "target": "src/foo.rs/helper",
      "files": ["src/foo.rs"],
      "message": null,
      "workflow": null,
      "git_head": "def456",
      "timestamp": "2025-01-15T10:25:00Z",
      "parent": 1,
      "children": []
    }
  ]
}
```

Note: `moss history` is the primary interface for shadow git. Mutations (`--undo`, `--redo`, `--goto`) work on both `moss history` and `moss edit` for convenience.

Undo output includes:
- Files changed
- Edit descriptions (from `--message` if provided)
- Operation type and target

### Configuration
```toml
[shadow]
enabled = true                # Default: true
retention_days = 30           # Auto-cleanup old commits
warn_on_delete = true         # Confirm before deleting symbols
```

## Architecture

### Tree Structure (Not Linear)

Shadow history is a **tree**, not a linear history:
- Undo moves HEAD backward but doesn't destroy commits
- New edits after undo create a branch (fork in history)
- All edits preserved (can return to any previous state)
- Branches can be pruned for security (remove sensitive content from history)

```
         A -- B -- C -- D  (original history, still exists)
              \
               E -- F      (new branch: after undoing to B, made edits E, F)
                    ^
                   HEAD
```

Undo/redo mechanics:
- `--undo`: moves HEAD to parent commit, applies reverse patch to user files
- `--undo N`: undoes N commits in sequence
- `--redo`: moves HEAD to child commit, applies forward patch
  - If multiple children exist (branch point), prompts user or requires `--redo <ref>`
- After undo, new edits create a branch from current HEAD
- Original commits (like D above) still exist, reachable via `--history --all`

**Conflict handling**: If reverse patch doesn't apply (file was modified externally):
- Abort undo and report conflict
- User must resolve manually (e.g., discard external changes or use `moss edit --force-undo`)
- `--force-undo` overwrites file with shadow's known state (destructive)

**Branch navigation**: To restore a different branch:
```bash
moss edit --goto <ref>        # Move HEAD to ref, restore file to that state
moss edit --goto 2            # Go to commit 2 (by number from --history)
```

### Directory Structure
```
.moss/
  shadow/
    .git/                     # Shadow repository
      refs/
        heads/
          main                # Current position in edit tree
    worktree/                 # Working copy of tracked files
```

The shadow repo tracks files in a separate worktree, not the user's actual files. On each `moss edit`:
1. Copy current file state to worktree (captures "before")
2. Apply edit to user's file
3. Copy new file state to worktree (captures "after")
4. Commit the change

**Initialization**: Shadow git is created by `moss init` (if `[shadow] enabled = true`, which is the default). The init output explicitly states that shadow git is now active. The "initial state" commit (commit 0) is created on first `moss edit`, containing the file's state before that edit.

### Shadow Commit Format

Commit message (structured for parsing):
```
moss edit: delete src/foo.rs/deprecated_fn

Message: Removing deprecated function
Operation: delete
Target: src/foo.rs/deprecated_fn
Files: src/foo.rs
Git-HEAD: abc123
```

For workflow-driven edits:
```
moss edit: insert src/foo.rs/new_handler

Workflow: @api-scaffold
Operation: insert
Target: src/foo.rs/new_handler
Files: src/foo.rs
Git-HEAD: abc123
```

Git stores the diff separately. Timestamp comes from git commit metadata. `Git-HEAD` records the real git commit at time of edit (for checkpoint detection).

### Undo Granularity

Git's patch APIs enable fine-grained undo:
- `--undo` reverts entire commit (all files, all changes)
- `--undo --file src/foo.rs` reverts only that file
- `--undo --hunk` interactive hunk selection (like `git checkout -p`)
- `--undo --lines 10-25 src/foo.rs` non-interactive, reverts changes in line range

Non-interactive hunk selection (for LLM/scripted usage):
```bash
moss edit --show-hunks <ref>           # List hunks with IDs
moss edit --undo --hunk-id h1,h3       # Undo specific hunks by ID
moss edit --undo --lines 10-25 foo.rs  # Undo changes touching these lines
```

Each partial undo creates a new shadow commit with just those reversals.

### Multi-File Edits

Some operations may touch multiple files (future: cross-file refactors like `moss move`):
- Shadow commit is atomic: all files in one commit
- Partial undo (file or hunk level) available via flags above
- `--history src/foo.rs` filters to show only commits affecting that file

### Branch Pruning (Security)

If sensitive content was accidentally committed:
```bash
moss edit --prune <commit-range>  # Remove commits from shadow history
moss edit --prune-file src/secrets.rs  # Remove all history for a file
```

Uses `git filter-branch` or similar under the hood. Important for:
- Removing accidentally committed secrets
- Cleaning up after experiments
- Reducing repo size

## Design Decisions

### D1: Tree structure over linear
- **Decision**: Preserve all history as tree
- **Rationale**: Undo shouldn't destroy information; users might want to return to undone state
- **Trade-off**: More disk usage, but git handles this well

### D2: Per-file filtering (not branches)
- **Decision**: Single unified timeline, with per-file filtering via `--history <file>`
- **Rationale**: Per-file branches create confusing parallel timelines. One chronological history is simpler.
- **Implementation**: `--history src/foo.rs` filters to commits affecting that file, but all commits share one timeline

### D3: Storage format
- **Decision**: Use git
- **Rationale**: Delta compression, familiar tooling, handles trees naturally

### D4: External changes
- **Decision**: Re-sync by reading current file state before commit
- **Rationale**: Shadow tracks moss edits, not manual edits; patch may fail if file diverged

### D5: Relationship to real git
- **Decision**: Real git is source of truth; shadow tracks uncommitted moss edits
- **Rationale**: Once user commits in real git, they've accepted those changes. Shadow serves the gap between edits and commits.
- **Mechanics**:
  - Shadow records real git HEAD at each shadow commit (for context)
  - On each `moss edit`, check if real git HEAD changed since last shadow commit → checkpoint
  - `--undo` by default won't cross checkpoint boundaries (user explicitly committed)
  - `--undo --cross-checkpoint` allows undoing past a real commit (with warning)
  - `moss edit --status` shows: shadow edits since last real commit
- **Git operations that change files** (`git reset`, `git checkout`, `git stash pop`):
  - Detected on next `moss edit` via HEAD or file content mismatch
  - Shadow re-syncs: records new file state as baseline, creates checkpoint
  - Old shadow history preserved but marked as pre-divergence
- **Decision**: Keep old shadow history after checkpoint (archaeology). Disk is cheap, lost work is expensive. Manual `--prune` available if needed.

### D6: Multiple worktrees
- **Problem**: User may have multiple git worktrees of the same repo. Each worktree has its own file state.
- **Decision**: Each worktree gets its own shadow repo, sharing nothing
- **Rationale**: Shadow tracks file state, which differs per worktree. Config consistency with current worktree state is cleaner.
- **Implementation**: Shadow repo at `.moss/shadow/` within each worktree's directory
- **Pruning across worktrees**:
  - `--prune` detects if same file has shadow history in other worktrees
  - Interactive: prompts "Also prune in N other worktrees? [Y/n]" (default: yes)
  - `--prune --all-worktrees` non-interactive, prunes everywhere
  - `--prune --local` non-interactive, prunes only current worktree

## Implementation Plan

### Phase 1: Basic Infrastructure
- [ ] Create `.moss/shadow/` git repo on first `moss edit`
- [ ] Commit file state before each edit
- [ ] `--message`/`--reason` flag for edit descriptions
- [ ] `moss history` command (list recent edits)
- [ ] `moss history --json` for machine-readable output
- [ ] `moss history --diff <ref>` to view changes

### Phase 2: Undo/Redo + Git Integration
- [ ] `moss edit --undo` applies reverse patch, moves HEAD backward
- [ ] `moss edit --undo N` reverts N edits in sequence
- [ ] `moss edit --undo --dry-run` preview without applying
- [ ] `moss edit --undo --file` partial undo for specific file
- [ ] `moss edit --undo --hunk` interactive hunk-level undo
- [ ] `moss history --show-hunks` and `moss edit --undo --hunk-id` for non-interactive
- [ ] `moss edit --undo --lines` for line-range based undo
- [ ] `moss edit --redo` moves HEAD forward
- [ ] `moss edit --goto <ref>` jumps to arbitrary commit
- [ ] Conflict detection and `--force-undo` for external modifications
- [ ] `moss history --all` shows full tree structure
- [ ] `moss history <file>` filters to commits affecting that file
- [ ] `moss history --status` shows uncommitted shadow edits
- [ ] Checkpoint integration: record real git HEAD, respect commit boundaries

### Phase 3: Security + Polish
- [ ] `--prune` for removing commits/branches
- [ ] Retention policy / auto-cleanup (only prunes merged branches)
- [ ] `warn_on_delete` confirmation

## Risks

1. **Disk usage**: Tree structure preserves everything
   - Mitigation: Retention policy prunes old merged branches, `--prune` for manual cleanup, git gc

2. **Performance**: Git operations add latency
   - Mitigation: Commits are small; consider async commits for non-blocking edits

3. **Complexity**: Tree navigation
   - Mitigation: Simple undo/redo for common case; tree visible only via `--history --all`

## Example Session

### Basic undo/redo

```bash
$ moss edit src/foo.rs/old_fn delete --message "Cleanup"
delete: old_fn in src/foo.rs

$ moss edit src/foo.rs/helper rename new_helper
rename: helper -> new_helper in src/foo.rs

$ moss history
  2. [HEAD] rename: helper -> new_helper in src/foo.rs
  1. delete: old_fn in src/foo.rs "Cleanup"

$ moss edit --undo 2
Undoing 2 edits:
  [2] rename: helper -> new_helper
  [1] delete: old_fn "Cleanup"
Files restored: src/foo.rs
HEAD now at: (initial state)

$ moss edit src/foo.rs/new_fn insert "fn new_fn() {}"
insert: new_fn in src/foo.rs
(created branch from initial state)

$ moss history --all
  * 3. [HEAD] insert: new_fn in src/foo.rs
  |
  | 2. rename: helper -> new_helper in src/foo.rs
  | 1. delete: old_fn in src/foo.rs "Cleanup"
  |/
  0. (initial state)
```

### Checkpoint behavior (real git integration)

```bash
$ moss edit src/foo.rs/bar delete
delete: bar in src/foo.rs

$ git add -A && git commit -m "Remove bar"
[main abc123] Remove bar

$ moss edit src/foo.rs/baz delete
delete: baz in src/foo.rs
(checkpoint: git commit abc123)

$ moss edit --undo
Undoing: delete baz in src/foo.rs
Files restored: src/foo.rs

$ moss edit --undo
error: Cannot undo past checkpoint (git commit abc123).
hint: Use --undo --cross-checkpoint to undo past real git commits.

$ moss history --status
Shadow edits since last commit: 0
Last checkpoint: abc123 "Remove bar"
```
