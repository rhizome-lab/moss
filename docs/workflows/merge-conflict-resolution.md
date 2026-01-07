# Merge Conflict Resolution Workflow

Resolving git merge conflicts by understanding both sides' intent and producing a correct combined result.

## Trigger

- `git merge` or `git rebase` reports conflicts
- PR can't be auto-merged
- Long-running branch needs to sync with main

## Goal

- Correct resolution that preserves intent of both changes
- No introduced bugs (semantic conflicts)
- Tests pass after resolution
- Understandable to future readers (not a confusing hybrid)

## Prerequisites

- Understanding of what both branches were trying to do
- Ability to run tests
- Access to commit history and PR descriptions

## Why Merge Conflicts Are Hard

1. **Textual vs semantic**: Git shows text conflicts, but semantic conflicts have no markers
2. **Intent is implicit**: Code shows WHAT changed, not WHY
3. **Context loss**: Long-running branches lose context of original decisions
4. **Combinatorial**: Both changes might be right individually, wrong together
5. **Testing burden**: Must verify the combined result, not just each side

## Types of Conflicts

| Type | Detection | Difficulty |
|------|-----------|------------|
| **Textual** | Git marks with `<<<<<<<` | Low - just needs correct merge |
| **Semantic** | Tests fail, behavior wrong | High - no markers, subtle bugs |
| **Structural** | File moved/renamed + edited | Medium - git can't track |
| **Dependency** | Both add same dependency differently | Medium - version conflicts |

## Core Strategy: Understand → Resolve → Verify

```
┌─────────────────────────────────────────────────────────┐
│                    UNDERSTAND                            │
│  Why did each side make this change?                    │
│  What was the intent? What problem was solved?          │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                     RESOLVE                              │
│  Combine changes preserving both intents                │
│  Or choose one if they're mutually exclusive            │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                     VERIFY                               │
│  Tests pass, behavior correct, no semantic conflicts    │
└─────────────────────────────────────────────────────────┘
```

## Phase 1: Understand the Conflict

### Gather Context

```bash
# What branches are involved?
git log --oneline HEAD...MERGE_HEAD | head -20

# What was the merge base? (common ancestor)
git merge-base HEAD MERGE_HEAD

# View the three-way diff
git diff :1:file.rs :2:file.rs  # base → ours
git diff :1:file.rs :3:file.rs  # base → theirs

# Or use merge tool
git mergetool
```

### Understand Each Side's Intent

```bash
# Ours: what were we trying to do?
git log --oneline main..HEAD -- conflicted_file.rs
git show <our-commit>  # Full commit message + diff

# Theirs: what were they trying to do?
git log --oneline main..MERGE_HEAD -- conflicted_file.rs
git show <their-commit>  # Full commit message + diff

# Check PR descriptions if available
gh pr view <pr-number>
```

### Categorize the Conflict

Ask:
- **Independent changes?** Both can coexist (add to different parts)
- **Overlapping changes?** Both modify same logic (need to combine)
- **Contradictory changes?** Mutually exclusive (must choose one)
- **Refactor collision?** One side refactored, other added feature

## Phase 2: Resolution Strategies

### Strategy 1: Combine Both (Independent)

```
Scenario: Both sides added new functions to the same file

<<<<<<< HEAD
fn feature_a() { ... }
=======
fn feature_b() { ... }
>>>>>>> feature-branch

Resolution: Keep both
fn feature_a() { ... }
fn feature_b() { ... }
```

### Strategy 2: Interleave Logic (Overlapping)

```
Scenario: Both sides modified the same function

Base:
fn process(x: i32) -> i32 {
    x + 1
}

Ours: Added validation
fn process(x: i32) -> i32 {
    if x < 0 { return 0; }
    x + 1
}

Theirs: Added logging
fn process(x: i32) -> i32 {
    log::debug!("processing {}", x);
    x + 1
}

Resolution: Combine both modifications
fn process(x: i32) -> i32 {
    log::debug!("processing {}", x);
    if x < 0 { return 0; }
    x + 1
}
```

### Strategy 3: Choose One (Contradictory)

```
Scenario: Both sides changed a constant to different values

Base:    const TIMEOUT: u64 = 30;
Ours:    const TIMEOUT: u64 = 60;  // Increased for slow networks
Theirs:  const TIMEOUT: u64 = 10;  // Decreased for faster feedback

Resolution: Understand WHY, then choose (or find third option)
- If both reasons valid, maybe make it configurable
- If one is wrong, choose the other
- Consult with authors if unclear
```

### Strategy 4: Reapply on Refactored Code

```
Scenario: One side refactored, other added feature to old code

Ours: Refactored module structure
  - Old: src/utils.rs with helper functions
  - New: src/utils/strings.rs, src/utils/numbers.rs

Theirs: Added new helper to old src/utils.rs
  fn new_helper() { ... }

Resolution: Apply their change to the new structure
  Add new_helper() to appropriate new file (src/utils/strings.rs?)
```

## Phase 3: Handling Specific Conflict Types

### Import/Use Statement Conflicts

```rust
// Common: both sides added imports

<<<<<<< HEAD
use std::collections::HashMap;
use crate::feature_a::Helper;
=======
use std::collections::{HashMap, HashSet};
use crate::feature_b::Processor;
>>>>>>> feature-branch

// Resolution: Combine all imports
use std::collections::{HashMap, HashSet};
use crate::feature_a::Helper;
use crate::feature_b::Processor;
```

### Lock File Conflicts (package-lock.json, Cargo.lock)

```bash
# Don't manually resolve - regenerate

# For npm
git checkout --theirs package-lock.json  # or --ours
npm install  # Regenerates lock file

# For Cargo
git checkout --theirs Cargo.lock
cargo update  # Or cargo build to regenerate

# For yarn
yarn install --force
```

### Configuration File Conflicts

```yaml
# Often: both sides added different config entries

<<<<<<< HEAD
features:
  - feature_a
=======
features:
  - feature_b
>>>>>>> feature-branch

# Resolution: Usually combine (order might matter)
features:
  - feature_a
  - feature_b
```

### Migration/Schema Conflicts

```
Scenario: Both branches added database migrations

Branch A: migrations/003_add_users_table.sql
Branch B: migrations/003_add_orders_table.sql

Resolution:
1. Renumber one migration (003 → 004)
2. Ensure they can run in either order
3. Or combine if they must be atomic
```

## Phase 4: Detecting Semantic Conflicts

Semantic conflicts have NO textual markers - both sides' changes merge cleanly but the result is broken.

### Signs of Semantic Conflict

```
Scenario:

Base:
  fn get_user(id: i32) -> User { ... }

Branch A: Changed signature
  fn get_user(id: i32, include_deleted: bool) -> User { ... }

Branch B: Added new caller
  let user = get_user(42);  // Uses old signature

Git merge: SUCCESS (no textual conflict!)
Reality: Compile error - wrong number of arguments
```

### Detection Methods

```bash
# 1. Always compile after merge
cargo build  # or npm run build, go build, etc.

# 2. Always run tests after merge
cargo test

# 3. Check for new callers of changed functions
git diff main..MERGE_HEAD --name-only  # Files they changed
# Look for new calls to functions we modified

# 4. Type checker is your friend
# TypeScript, Rust, etc. catch many semantic conflicts at compile time
```

### LLM-Assisted Semantic Analysis

```
For each function with signature change in our branch:
  1. Find all callers in their branch
  2. Check if callers use old signature
  3. Flag for manual review

For each removed/renamed symbol in our branch:
  1. Check if their branch references it
  2. Flag if so
```

## Phase 5: Verification

### Test the Merge

```bash
# Run full test suite
cargo test

# Run integration tests
cargo test --test integration

# If tests existed before conflict, they should still pass
```

### Manual Verification Checklist

- [ ] Both features work as intended
- [ ] No duplicate code (both sides added similar things)
- [ ] No dead code (removed by one side, referenced by other)
- [ ] Imports are clean (no unused, no missing)
- [ ] Documentation is consistent

### Review the Resolution

```bash
# View what you're about to commit
git diff --cached

# Make sure resolution makes sense
# Ask: "Would someone reading this understand it?"
```

## LLM-Specific Techniques

LLMs can help with conflict resolution by understanding intent:

### Gather Context for LLM

```bash
# Extract all relevant context
echo "=== CONFLICT FILE ===" > context.txt
cat conflicted_file.rs >> context.txt

echo "=== OUR CHANGES ===" >> context.txt
git log -p main..HEAD -- conflicted_file.rs >> context.txt

echo "=== THEIR CHANGES ===" >> context.txt
git log -p main..MERGE_HEAD -- conflicted_file.rs >> context.txt

echo "=== BASE VERSION ===" >> context.txt
git show :1:conflicted_file.rs >> context.txt
```

### LLM Prompt Structure

```
Given:
- Base version (common ancestor)
- Our changes with commit messages explaining why
- Their changes with commit messages explaining why

Task:
1. Explain what each side was trying to accomplish
2. Identify if changes are independent, overlapping, or contradictory
3. Propose a resolution that preserves both intents
4. Flag any potential semantic conflicts
```

### Automated Conflict Detection

```bash
# For each conflicted file, extract:
# - The conflict markers
# - Surrounding context
# - Commit messages from both sides

git diff --name-only --diff-filter=U  # List conflicted files

for file in $(git diff --name-only --diff-filter=U); do
  echo "=== $file ==="
  git show :1:"$file" > /tmp/base.txt
  git show :2:"$file" > /tmp/ours.txt
  git show :3:"$file" > /tmp/theirs.txt
  # Now have three versions to analyze
done
```

## Common Mistakes

| Mistake | Why It's Bad | Prevention |
|---------|--------------|------------|
| Always picking "ours" or "theirs" | Loses one side's changes | Understand both sides first |
| Not running tests | Semantic conflicts go undetected | Always test after merge |
| Resolving without understanding | Creates confused hybrid | Read commit messages, PRs |
| Rushing large conflicts | Introduces subtle bugs | Take time, break into pieces |
| Not committing promptly | Lose resolution work | Commit resolution immediately |

## Prevention

1. **Merge frequently** - Small conflicts are easier than large ones
2. **Communicate** - If you're changing something others might touch, mention it
3. **Feature flags** - Allow parallel development without conflict
4. **Clear ownership** - Reduce concurrent changes to same files
5. **Atomic commits** - Easier to understand intent when resolving

## Tools

```bash
# Built-in
git mergetool          # Opens configured merge tool
git checkout --ours    # Take our version entirely
git checkout --theirs  # Take their version entirely

# Visual merge tools
# - VS Code (built-in)
# - IntelliJ IDEA (built-in)
# - Meld (standalone)
# - Beyond Compare (commercial)
# - Kaleidoscope (macOS)

# Advanced
git rerere            # "Reuse recorded resolution" - remembers past resolutions
git imerge            # Incremental merge for complex cases
```

### Semantic Merge Tools

Traditional merge tools treat code as text. Semantic tools understand structure:

**SemanticMerge / Plastic SCM**
- Parses code as AST
- Understands "function moved" vs "function changed"
- Can auto-merge structural changes that confuse text-based merge
- Commercial, supports C#, Java, C/C++, others

**IntelliJ IDEA**
- Has some semantic awareness for refactorings
- Detects "renamed method" and can merge intelligently
- Works well for Java/Kotlin especially

**GumTree**
- Academic tool for AST differencing
- Not a merge tool itself, but basis for semantic diff
- https://github.com/GumTreeDiff/gumtree

**Mergiraf** (experimental)
- Structured merge for multiple languages
- https://mergiraf.org/

**Why this matters**: Text-based three-way merge fails when:
- Function was moved to different file on one branch, modified on other
- Variable renamed on one branch, new usage added on other
- Reordering of declarations (text differs, semantics identical)

Semantic merge handles these automatically, but tooling is less mature than text-based.

### Git Rerere (Reuse Recorded Resolution)

```bash
# Enable rerere
git config --global rerere.enabled true

# How it works:
# 1. You resolve a conflict
# 2. Git remembers: "conflict X resolves to Y"
# 3. Next time same conflict appears, auto-applies resolution
```

**Use cases** (somewhat obscure, but valuable when applicable):
- **Repeated rebases**: Rebasing a branch multiple times against moving target
- **Cherry-picking across branches**: Same conflict appears in multiple branches
- **Redo after reset**: You resolved, then reset, now redoing
- **Long-running branches**: Same file keeps conflicting on every sync

**Limitations**:
- Only helps with identical conflicts (same text)
- Doesn't transfer between machines (unless you share .git/rr-cache)
- Still need to verify resolution is correct in new context

```bash
# View recorded resolutions
ls .git/rr-cache/

# Forget a resolution (if it was wrong)
git rerere forget path/to/file
```

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| Lost changes | Feature doesn't work after merge | Check both branches, re-apply lost changes |
| Semantic conflict | Tests fail, runtime error | Review combined code logic |
| Duplicate code | Same feature implemented twice | Delete one, keep better version |
| Broken references | Compile errors after clean merge | Check for renamed/moved symbols |

## Open Questions

### Automated Resolution

Can LLMs reliably auto-resolve conflicts?
- Simple cases (both added imports): probably yes
- Complex cases (interleaved logic): risky without verification
- Need confidence estimation - when to auto-resolve vs ask human

### Resolution Reasoning Logs

With LLMs, we can cheaply log reasoning for every resolution:

```
# .git/conflict-resolutions/abc123.md (or similar)
## File: src/users.rs
## Conflict at lines 45-67

### Our change (commit def456):
Added validation to reject negative user IDs

### Their change (commit 789abc):
Added logging of user lookup attempts

### Resolution:
Combined both changes - validation first, then logging.
Order matters: we want to log the attempt before potentially rejecting.

### Confidence: High
Both changes are independent, no interaction concerns.
```

Benefits:
- Future reference when similar conflicts arise
- Code review of the merge itself
- Training data for better automated resolution
- Understanding past decisions when bugs surface later
- Git rerere remembers WHAT, this remembers WHY

**Open**: What's the right format/tooling for this? Inline in commit message? Separate files? Database?

### Large Rebases

When rebasing a long-lived branch with many commits against a changed mainline:

**Ideas**:
- **`git imerge`**: Incremental merge - merges pairwise, easier conflicts
- **Squash then rebase**: Reduce commit count first, fewer conflicts
- **Rebase in chunks**: Stop at known-stable points, verify, continue
- **Reverse approach**: Merge main into branch repeatedly (accumulate, then squash)

**What actually happens in practice?** Need more real-world case studies:
- How often do teams face this?
- What strategies work?
- When is it better to just merge (not rebase)?

### Conflict Prevention

Better ways to prevent conflicts in the first place?
- Dependency injection reducing coupling
- Finer-grained files
- Better tooling for "who's working on what"

### Three-Way Merge Alternatives

Are there better merge algorithms than three-way diff?
- Semantic merging (understanding AST, not just text)
- Intent-based merging (from commit messages)
- Patch theory (Darcs-style)

## See Also

- [Code Review](code-review.md) - Review merged result
- [Bug Fix](bug-fix.md) - If semantic conflict introduced bug
