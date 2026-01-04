# Batch Edit Design

## Problem

Currently, editing N files requires N sequential `moss edit` calls:

```bash
moss edit src/main.py/foo replace "..."
moss edit src/utils.py/bar replace "..."
moss edit src/config.py/baz delete
```

Each call:
1. Parses the file
2. Finds the symbol
3. Applies the edit
4. Writes the file
5. Creates a shadow git snapshot

For LLM-driven refactoring, this creates:
- N shell invocations
- N shadow snapshots (cluttered history)
- N file parses (same file may be parsed multiple times)
- Sequential I/O

## Solution: Batch Edit

A batch edit API that:
1. Collects all edits before applying
2. Groups by file (single parse per file)
3. Applies all edits atomically
4. Creates single shadow snapshot

### CLI Interface

Option A: JSON file input
```bash
moss edit --batch edits.json
```

Where `edits.json`:
```json
[
  {"target": "src/main.py/foo", "action": "replace", "content": "def foo(): pass"},
  {"target": "src/utils.py/bar", "action": "delete"},
  {"target": "src/main.py/baz", "action": "insert", "position": "after", "relative_to": "foo", "content": "..."}
]
```

Option B: Stdin input
```bash
cat edits.json | moss edit --batch -
```

Option C: Multi-arg syntax
```bash
moss edit --batch \
  src/main.py/foo::replace::"new content" \
  src/utils.py/bar::delete
```

**Recommendation**: Option A with Option B support. JSON is explicit and handles multiline content well.

### Lua API

```lua
-- Collect edits
local batch = edit.batch()
batch:replace("src/main.py/foo", "def foo(): pass")
batch:delete("src/utils.py/bar")
batch:insert("src/main.py/baz", {after = "foo", content = "..."})

-- Apply atomically
batch:apply({message = "Refactor foo system"})
```

Alternative functional style:
```lua
edit.batch({
  {target = "src/main.py/foo", action = "replace", content = "..."},
  {target = "src/utils.py/bar", action = "delete"},
}, {message = "Refactor foo system"})
```

### Implementation

Core batch edit logic in `crates/moss/src/edit.rs`:

```rust
pub struct BatchEdit {
    edits: Vec<EditOp>,
}

pub struct EditOp {
    target: String,
    action: EditAction,
}

impl BatchEdit {
    pub fn new() -> Self { ... }

    pub fn add(&mut self, target: &str, action: EditAction) {
        self.edits.push(EditOp { target: target.to_string(), action });
    }

    pub fn apply(&self, root: &Path) -> Result<BatchEditResult, Error> {
        // Group edits by file
        let by_file = self.group_by_file(root)?;

        // For each file: parse once, apply all edits, write once
        let mut results = Vec::new();
        for (path, file_edits) in by_file {
            let content = std::fs::read_to_string(&path)?;
            let new_content = self.apply_file_edits(&path, &content, &file_edits)?;
            std::fs::write(&path, &new_content)?;
            results.push(FileEditResult { path, edits_applied: file_edits.len() });
        }

        // Single shadow snapshot
        if let Some(shadow) = ShadowGit::open(root) {
            shadow.snapshot(&SnapshotOptions { message: self.message.clone() })?;
        }

        Ok(BatchEditResult { files: results })
    }
}
```

### Edit Ordering Within File

When multiple edits target the same file, they must be applied in correct order:

1. **Sort by line number descending** - apply bottom-up so line numbers don't shift
2. **Validate no overlaps** - reject edits that would conflict
3. **Handle dependencies** - if edit B references symbol created by edit A, error

### Error Handling

- **Atomic failure**: If any edit fails validation, none are applied
- **Partial success option**: `--partial` flag to apply valid edits and skip failed ones
- **Dry run**: `--dry-run` shows what would change without applying

### Validation

Before applying:
1. Parse all target files
2. Resolve all symbol targets
3. Check for overlapping edits (same region)
4. Check for circular dependencies

### Shadow Git Integration

- Single snapshot for entire batch
- Message includes summary: "Batch edit: 5 files, 12 edits"
- Individual edit details in commit metadata

## Non-Goals

- Real-time collaborative editing (not a code editor)
- Conflict resolution (fail fast, let user fix)

Note: Partial undo (undo individual edits within batch) is trivial once hunk-level undo exists.

## Migration Path

1. Add `BatchEdit` struct to `edit.rs`
2. Add `--batch` flag to CLI
3. Expose in Lua as `edit.batch()`
4. Document in CLI help

## Success Criteria

- 10+ edits across 5+ files completes in <1s
- Single shadow snapshot for batch
- Clear error messages for conflicts
- Works with existing `--dry-run` flag
