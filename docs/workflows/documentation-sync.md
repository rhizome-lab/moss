# Documentation Sync Workflow

Keeping documentation in sync with code: preventing stale docs, broken examples.

## Trigger

- Code change that affects documented behavior
- PR review notices doc staleness
- Regular doc audit schedule
- User reports incorrect documentation

## Goal

- Documentation accurately reflects code
- Examples compile and run
- Links work
- No stale references

## Prerequisites

- Codebase with documentation
- Understanding of doc locations
- Ability to test examples

## Decomposition Strategy

**Detect → Locate → Update → Verify**

```
1. DETECT: Find stale documentation
   - Check references to renamed/removed symbols
   - Validate example code
   - Check internal links
   - Compare doc statements with code behavior

2. LOCATE: Find what needs updating
   - Map doc sections to code sections
   - Identify scope of staleness
   - Prioritize by impact

3. UPDATE: Fix the documentation
   - Update text to match code
   - Fix or remove broken examples
   - Update links
   - Add missing documentation

4. VERIFY: Ensure accuracy
   - Run example code
   - Check links
   - Review against code
   - Get feedback from users
```

## Tools Used

| Phase | Tools |
|-------|-------|
| Detect | `moss analyze check-refs`, `moss analyze stale-docs` |
| Locate | `text-search`, `view` |
| Update | `edit`, write tools |
| Verify | `moss analyze check-examples`, manual testing |

## Detection Methods

### Automated Checks
```bash
# Check for broken doc references
moss analyze check-refs

# Find docs referencing renamed/removed code
moss analyze stale-docs

# Verify example code still works
moss analyze check-examples
```

### Manual Checks
- Read docs, try following them
- Run documented commands
- Compare screenshots with current UI
- Check version numbers

## Common Staleness Patterns

### Renamed Symbols
```
Problem: Doc references `old_function_name`
Detection: moss analyze stale-docs
Fix: Update to `new_function_name`
```

### Removed Features
```
Problem: Doc describes feature that no longer exists
Detection: Feature search returns no results
Fix: Remove or archive documentation
```

### Changed Behavior
```
Problem: Doc says X happens, but Y happens now
Detection: Manual testing, user reports
Fix: Update description
```

### Broken Examples
```
Problem: Example code doesn't compile/run
Detection: moss analyze check-examples
Fix: Update example or fix code
```

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| Incomplete update | Some docs still stale | Full audit of changed area |
| Wrong interpretation | Fix doesn't match code | Have code author review |
| Breaking examples | Examples no longer work | Test examples in CI |
| Missing context | Fix is correct but unclear | Add more explanation |

## Example Session

**Trigger**: Function renamed, docs need update

```
Turn 1: Find stale references
  $(moss analyze stale-docs)
  → docs/api.md references `get_user` (now `fetch_user`)
  → docs/examples.md uses old function name

Turn 2: Find all occurrences
  $(text-search "get_user" path:docs)
  → docs/api.md:45
  → docs/api.md:102
  → docs/examples.md:23
  → docs/migration.md:15 (intentional - shows old API)

Turn 3: Update api.md
  $(edit docs/api.md)
  → Replace get_user with fetch_user
  → Update parameter documentation (also changed)

Turn 4: Update examples.md
  $(edit docs/examples.md)
  → Update function name
  → Verify example still makes sense

Turn 5: Keep migration.md
  → Intentionally shows old API for migration purposes
  → Add note: "In versions < 2.0, this was called `get_user`"

Turn 6: Verify
  $(moss analyze check-refs)
  → No broken references
  $(cargo test --doc)
  → Doc tests pass
```

## Prevention Strategies

### Doctest / Example Testing
```rust
/// Fetch a user by ID.
///
/// ```
/// let user = fetch_user(123)?;
/// ```
pub fn fetch_user(id: u64) -> Result<User> { ... }
```

### CI Checks
```yaml
- name: Check documentation
  run: |
    moss analyze check-refs
    moss analyze stale-docs
    cargo test --doc
```

### Doc-Code Proximity
Keep docs close to code (doc comments, adjacent README) rather than separate docs directory.

### Review Process
- Require doc review for API changes
- Link docs to code in PRs
- Flag docs-only vs code-only changes

## Anti-patterns

- **Orphan docs**: Documentation far from code it describes
- **Aspirational docs**: Documenting planned features as if they exist
- **Copy-paste docs**: Duplicating docs, one copy gets stale
- **No examples**: Docs without runnable examples can't be verified

## See Also

- [Documentation Synthesis](documentation-synthesis.md) - Generating docs from code
- [Code Review](code-review.md) - Catching doc staleness in review
