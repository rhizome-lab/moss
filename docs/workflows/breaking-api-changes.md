# Breaking API Changes Workflow

Handling upstream dependency changes that break your code - library updates, framework upgrades, API deprecations.

## Trigger

- Dependency update breaks build
- Deprecation warnings appearing
- Security advisory requires upgrade
- New features need newer dependency version
- End-of-life for current dependency version

## Goal

- Update to new version successfully
- Code works correctly with new API
- No functionality regression
- Understand what changed and why
- Avoid similar breakage in future

## Prerequisites

- Changelog/release notes for dependency
- Migration guide (if available)
- Working test suite
- Ability to run both versions (for comparison)

## Why Breaking Changes Are Hard

1. **Cascading effects**: One change may require many call site updates
2. **Semantic changes**: Same API name, different behavior
3. **Missing documentation**: Not all changes are documented
4. **Version constraints**: Other dependencies may conflict
5. **Testing gaps**: Your tests may not cover all affected code
6. **Time pressure**: Security updates can't wait

## Types of Breaking Changes

| Type | Difficulty | Example |
|------|------------|---------|
| **Renamed** | Low | `foo()` → `bar()` |
| **Signature change** | Medium | Added required parameter |
| **Removed** | Medium | Function no longer exists |
| **Semantic change** | High | Same name, different behavior |
| **Type change** | Medium-High | Return type changed |
| **Structural** | High | Entire module reorganized |

## Core Strategy: Assess → Plan → Migrate → Verify

```
┌─────────────────────────────────────────────────────────┐
│                      ASSESS                              │
│  What changed? How much of your code is affected?       │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                       PLAN                               │
│  Strategy: all-at-once vs incremental?                  │
│  Compatibility shim needed?                             │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                     MIGRATE                              │
│  Update code, following migration guide                 │
│  Address each breaking change systematically            │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                      VERIFY                              │
│  Tests pass, behavior unchanged                         │
│  Check for semantic changes especially                  │
└─────────────────────────────────────────────────────────┘
```

## Phase 1: Assess the Damage

### Read the Changelog

```bash
# Find the changelog
cat CHANGELOG.md
cat HISTORY.rst
cat NEWS

# Or on GitHub
gh release view v2.0.0 --repo some/dependency

# Look for:
# - "BREAKING" or "Breaking Changes" sections
# - Migration guides
# - Deprecation notices from previous versions
```

### Try the Upgrade

```bash
# Update dependency
cargo update -p some-crate
npm update some-package
pip install --upgrade some-lib

# Try to build
cargo build 2>&1 | tee build-errors.txt
npm run build 2>&1 | tee build-errors.txt

# Count errors
grep -c "error\[" build-errors.txt
```

### Map Affected Code

```bash
# Find all uses of changed API
grep -r "old_function_name" --include="*.rs" src/
grep -r "from old_module import" --include="*.py" .

# Count affected files
grep -r "some_crate::" --include="*.rs" -l src/ | wc -l
```

### Categorize Changes

```markdown
## Breaking Changes Assessment

### Renamed (easy)
- `foo()` → `bar()`: 15 call sites
- `Baz` type → `Qux` type: 8 uses

### Signature Changes (medium)
- `process(x)` → `process(x, config)`: 23 call sites
  - Need to determine correct config for each

### Removed (need alternative)
- `deprecated_helper()`: 5 uses
  - Replacement: implement ourselves or use `new_helper()`

### Semantic Changes (careful!)
- `parse()` now returns Result instead of panicking
  - All 12 call sites need error handling
```

## Phase 2: Plan the Migration

### Choose a Strategy

**Big Bang**: Update everything at once
```
Pros: Clean cut, no compatibility layers
Cons: Large diff, harder to review, risky

Best for: Small changes, few affected files
```

**Incremental**: Migrate piece by piece
```
Pros: Smaller diffs, easier review, rollback possible
Cons: Temporary compatibility code, longer timeline

Best for: Large changes, many affected files
```

**Compatibility Shim**: Wrap old API over new
```rust
// Shim: old interface calling new implementation
#[deprecated(note = "Use new_api instead")]
pub fn old_api(x: i32) -> i32 {
    new_api(x, Config::default())
}
```

### Version Pinning Strategy

```toml
# Cargo.toml: Allow compatible updates
[dependencies]
some-crate = "1.5"  # Accepts 1.5.x

# Or: Pin exactly during migration
some-crate = "=1.5.3"  # Exact version

# Or: Allow range
some-crate = ">=1.5, <2.0"
```

### Dependency Conflict Resolution

```bash
# Check for conflicts
cargo tree -d  # Duplicate dependencies
npm ls         # Dependency tree

# If conflict:
# - Can you update other deps too?
# - Can you use resolutions/overrides?
# - Do you need to wait for ecosystem?
```

## Phase 3: Migration Techniques

### Simple Renames

```bash
# Find and replace
sed -i 's/old_name/new_name/g' src/**/*.rs

# Or more carefully with codemod tools
fastmod --accept-all 'old_name' 'new_name' src/

# Or with IDE refactoring
# VSCode: Right-click → Rename Symbol (F2)
```

### Signature Changes

```rust
// Old
let result = process(data);

// New: added required parameter
let result = process(data, Options::default());

// Find all call sites, update each
```

For complex signature changes:
```rust
// Create helper to ease migration
fn process_compat(data: &Data) -> Result {
    process(data, Options::default())
}

// Replace calls with helper first
// Then gradually migrate to full API
// Finally remove helper
```

### Type Changes

```rust
// Old: returned Option
let value = get_thing(id);
if let Some(v) = value { ... }

// New: returns Result
let value = get_thing(id)?;
// Now need to handle error case everywhere
```

### Removed APIs

```python
# Old: library provided helper
from lib import deprecated_helper
result = deprecated_helper(x)

# New: removed, implement yourself
def deprecated_helper(x):
    """Compat shim - remove when migration complete"""
    # Implementation copied from old library
    # or reimplemented
    return x.something()

result = deprecated_helper(x)
```

### Semantic Changes (Most Dangerous)

```python
# Old behavior: returns -1 on failure
result = parse(text)
if result == -1:
    handle_error()

# New behavior: raises exception on failure
try:
    result = parse(text)
except ParseError:
    handle_error()

# These are easy to miss - tests are critical
```

## Phase 4: Verification

### Run Tests

```bash
# Full test suite
cargo test
pytest
npm test

# Focus on affected areas
cargo test --package affected-module
pytest tests/test_integration.py -k "uses_updated_lib"
```

### Check for Semantic Regressions

```python
# Before migration: capture current behavior
def test_parse_returns_minus_one_on_empty():
    assert parse("") == -1  # Old behavior

# After migration: update expectation
def test_parse_raises_on_empty():
    with pytest.raises(ParseError):
        parse("")  # New behavior
```

### Differential Testing

If you can run both versions:

```bash
# Generate test inputs
python generate_inputs.py > inputs.json

# Run with old version
git stash  # Save migration work
OLD_OUTPUT=$(python run.py < inputs.json)
git stash pop

# Run with new version
NEW_OUTPUT=$(python run.py < inputs.json)

# Compare
diff <(echo "$OLD_OUTPUT") <(echo "$NEW_OUTPUT")
```

### Review Deprecation Warnings

```bash
# Python: Enable deprecation warnings
python -W default::DeprecationWarning -c "import your_code"

# Rust: Enable all warnings
RUSTFLAGS="-W warnings" cargo build

# Node: look for deprecation notices
npm ls 2>&1 | grep -i deprec
```

## Common Scenarios

### Scenario: Major Framework Upgrade (React, Django, Rails)

```
1. Read upgrade guide (usually excellent for major frameworks)
2. Create new branch
3. Update version
4. Fix build errors (usually many)
5. Run tests, fix failures
6. Manual testing of key flows
7. Deploy to staging, soak test
8. Gradual production rollout
```

### Scenario: Security Patch (Urgent)

```
1. Read advisory - what's the vulnerability?
2. Update immediately (accept some breakage)
3. Fix critical build errors
4. Run smoke tests
5. Deploy with monitoring
6. Fix remaining issues in follow-up
```

### Scenario: Gradual Deprecation

```
1. Notice deprecation warnings in logs
2. Check timeline - when removed?
3. Plan migration before deadline
4. Update incrementally
5. Remove deprecated usage
6. Verify no warnings remain
```

### Scenario: Library Abandoned

```
1. Assess: is a fork maintained?
2. Options:
   a. Fork and maintain yourself
   b. Find alternative library
   c. Inline the code you need
   d. Accept the risk (not recommended)
3. If migrating to alternative:
   - Treat as API translation problem
   - May need significant refactoring
```

## Compatibility Patterns

### Adapter Pattern

```rust
// Wrap new API to match old interface
struct OldStyleAdapter {
    new_impl: NewImplementation,
}

impl OldInterface for OldStyleAdapter {
    fn old_method(&self, x: i32) -> i32 {
        self.new_impl.new_method(x, Default::default())
    }
}
```

### Feature Flags

```rust
#[cfg(feature = "new-api")]
use new_crate::Widget;

#[cfg(not(feature = "new-api"))]
use old_crate::Widget;
```

### Polyfill / Shim

```python
# Provide missing functionality for old version
try:
    from new_module import new_function
except ImportError:
    def new_function(x):
        # Fallback implementation
        return old_function(x)
```

## LLM-Specific Techniques

### Changelog Analysis

```
Given this changelog:

## v2.0.0 Breaking Changes
- `process()` now requires a Config parameter
- `parse()` returns Result instead of panicking
- Removed `deprecated_helper()`

And this code:
```rust
fn main() {
    let result = process(data);
    let parsed = parse(text);
    let helper = deprecated_helper(x);
}
```

Generate the updated code and explain each change.
```

### Migration Pattern Recognition

```
This code uses the old API:
```python
response = client.get(url, verify=False)
data = response.json
```

The new API:
- `get()` is now async
- `.json` is now `.json()`

Generate migration and note any semantic differences.
```

### Bulk Update Generation

```
Given 50 call sites of `old_function(x)` that need to become
`new_function(x, Config::default())`, generate a script or
codemod to update them all.
```

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| Missed semantic change | Tests pass but behavior wrong | Add targeted tests |
| Version conflict | Build fails with dep resolution error | Check dep tree, update others |
| Incomplete migration | Deprecation warnings remain | Grep for old API uses |
| Performance regression | Benchmarks slower | Profile, may need API changes |

## Anti-patterns

- **Ignoring deprecation warnings**: They become errors eventually
- **Upgrading without reading changelog**: Miss semantic changes
- **No tests before migrating**: Can't verify correctness
- **Big bang for large changes**: Hard to debug failures
- **Not pinning during migration**: Version can shift under you

## Prevention

1. **Stay current**: Small updates easier than big jumps
2. **Read release notes**: Subscribe to dependency updates
3. **Test coverage**: Can't verify what you don't test
4. **Dependency audit**: Regularly check for outdated deps
5. **Lock files**: Pin versions explicitly

## Tools

```bash
# Find outdated dependencies
cargo outdated           # Rust
npm outdated            # JavaScript
pip list --outdated     # Python
bundle outdated         # Ruby

# Check for security issues
cargo audit
npm audit
pip-audit
bundle audit

# Dependency update tools
dependabot              # GitHub
renovate                # GitLab/GitHub
cargo update            # Update within constraints
```

## Open Questions

### Semantic Versioning Trust

Can you trust that "minor" updates won't break you?
- In theory: semver guarantees
- In practice: bugs happen, edge cases differ
- Strategy: Test even minor updates

### Automated Migration

Can codemods/LLMs reliably update API usages?
- Simple renames: yes
- Signature changes: mostly
- Semantic changes: need verification

### Dependency Minimization

Should you minimize dependencies to reduce breaking change surface?
- Fewer deps = fewer breaking changes
- But: reinventing wheels has costs too
- Balance: depend on stable, well-maintained libs

## See Also

- [Cross-Language Migration](cross-language-migration.md) - Related, but between languages
- [Refactoring](refactoring.md) - Internal API changes
- [Bug Fix](bug-fix.md) - When migration reveals bugs
