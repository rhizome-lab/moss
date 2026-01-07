# Builtin Syntax Rules

Design and guidance for built-in syntax linting rules shipped with moss.

## Rule Loading Order

Rules are loaded in this order (later overrides earlier by `id`):

1. **Embedded builtins** - compiled into the moss binary
2. **User global** - `~/.config/moss/rules/*.scm`
3. **Project** - `.moss/rules/*.scm`

To disable a builtin, add to `.moss/config.toml`:

```toml
[analyze.rules."rust/println-debug"]
enabled = false
```

Or create a rule file with same `id` and `enabled = false`.

## Project Type Guidance

Not all rules are appropriate for all project types:

| Rule | Library | CLI Tool | Web App | Notes |
|------|---------|----------|---------|-------|
| `rust/println-debug` | ✓ Use | ❌ Disable | ✓ Use | CLI tools use println for output |
| `rust/dbg-macro` | ✓ Use | ✓ Use | ✓ Use | Never commit dbg! |
| `rust/unwrap-in-impl` | ✓ Use | ⚠️ Noisy | ⚠️ Noisy | Many in test code |
| `js/console-log` | ✓ Use | N/A | ✓ Use | Production should use logging |

**Library code** should avoid stdout/stderr side effects - use logging crates instead.

**CLI tools** legitimately use `println!` for output - disable `rust/println-debug`:

```toml
[analyze.rules."rust/println-debug"]
enabled = false
```

## Severity Recommendations

| Severity | Rules | When to use |
|----------|-------|-------------|
| `error` | `hardcoded-secret` | Must fix before commit |
| `warning` | `rust/dbg-macro`, `rust/todo-macro`, `no-fixme-comment` | Fix before merge |
| `info` | `rust/unnecessary-let`, `rust/println-debug`, `rust/unwrap-in-impl` | Consider fixing |

## Builtin Rules Reference

### Rust Rules

#### `rust/println-debug`
**Severity:** info | **Languages:** rust

Flags `println!`, `print!`, `eprintln!`, `eprint!` macros.

**Default allow:** `**/tests/**`, `**/examples/**`, `**/bin/**`, `**/main.rs`

**When to use:** Library code where stdout/stderr side effects are bugs.
**When to disable:** CLI tools, where println is the correct output mechanism.

#### `rust/dbg-macro`
**Severity:** warning | **Languages:** rust

Flags `dbg!()` macro calls - these should never be committed.

**Default allow:** `**/tests/**`

**Always use.** The dbg! macro is for temporary debugging only.

#### `rust/todo-macro`
**Severity:** warning | **Languages:** rust

Flags `todo!()` macro calls - unfinished code paths.

**Always use.** Helps track incomplete implementations.

#### `rust/unwrap-in-impl`
**Severity:** info | **Languages:** rust

Flags `.unwrap()` calls - suggests using `?` or `.expect()` with context.

**Default allow:** `**/tests/**`, `**/test_*.rs`, `**/*_test.rs`, `**/*_tests.rs`, `**/examples/**`, `**/benches/**`

**Legitimate unwrap uses:**
- Lock poisoning: `mutex.lock().unwrap()` - panic is correct if lock poisoned
- Known-safe conversions: after validation that guarantees success
- Test code: tests should panic on unexpected failures

```rust
// moss-allow: rust/unwrap-in-impl - panic correct if lock poisoned
let guard = CACHE.lock().unwrap();
```

#### `rust/expect-empty`
**Severity:** warning | **Languages:** rust

Flags `.expect("")` with empty string - provide meaningful context.

**Always use.** Empty expect messages waste the opportunity to explain failures.

#### `rust/unnecessary-let`
**Severity:** info | **Languages:** rust

Flags `let x = y;` where both are simple identifiers - the binding adds no value.

**Exclusions:** `let mut`, underscore-prefixed names, `None` value.

#### `rust/unnecessary-type-alias`
**Severity:** info | **Languages:** rust

Flags `type Foo = Bar;` where Bar is a simple type - adds indirection without value.

**Legitimate uses:** Generic bounds, documentation, API stability.

### JavaScript/TypeScript Rules

#### `js/console-log`
**Severity:** info | **Languages:** javascript, typescript, tsx, jsx

Flags `console.log`, `console.debug`, `console.info` calls.

**Default allow:** `**/tests/**`, `**/*.test.*`, `**/*.spec.*`

#### `js/unnecessary-const`
**Severity:** info | **Languages:** javascript, typescript, tsx, jsx

Flags `const x = y;` where both are simple identifiers.

**Exclusions:** `undefined`, `Infinity`, `NaN` (global constants).

### Cross-Language Rules

#### `hardcoded-secret`
**Severity:** error | **Languages:** all

Flags potential hardcoded secrets (API keys, passwords, tokens).

**Default allow:** `**/tests/**`, `**/*.test.*`

#### `no-todo-comment`
**Severity:** info | **Languages:** all with line comments

Flags `// TODO` comments in code.

#### `no-fixme-comment`
**Severity:** warning | **Languages:** all with line comments

Flags `// FIXME` comments - known bugs that need fixing.

## Configuration Examples

### Library Project
```toml
# Default rules are appropriate
# Optionally upgrade severity
[analyze.rules."rust/unwrap-in-impl"]
severity = "warning"
```

### CLI Tool Project
```toml
# Disable println-debug
[analyze.rules."rust/println-debug"]
enabled = false
```

### Per-Directory Exclusions
```toml
[analyze.rules."rust/unwrap-in-impl"]
allow = ["**/generated/**", "**/proto/**"]
```

## Known Limitations

### In-File Test Detection (`#[cfg(test)]`)

Rust commonly places tests in `#[cfg(test)]` modules within source files:

```rust
// src/lib.rs
pub fn add(a: i32, b: i32) -> i32 { a + b }

#[cfg(test)]
mod tests {
    #[test]
    fn test_add() {
        assert_eq!(add(1, 2).unwrap(), 3);  // Flagged but legitimate
    }
}
```

**Current state:** Glob patterns cannot detect `#[cfg(test)]` structure. These are flagged.

**Workarounds:**
1. Use inline `// moss-allow: rule-id` comments
2. Move tests to separate `tests/` directory or `*_test.rs` files
3. Accept some false positives for rules like `rust/unwrap-in-impl`

**Future work:** See "Design: `#[cfg(test)]` Detection" below.

### Query Predicate Limitations

Tree-sitter predicates have limitations:
- No access to type information (can't distinguish `Result::unwrap` from `Option::unwrap`)
- No cross-file analysis
- Limited to syntactic patterns

## Design: `#[cfg(test)]` Detection

**Problem:** Many false positives come from test code in `#[cfg(test)]` modules within source files.

**Options considered:**

1. **Global config flag:** `skip_test_code = true`
   - Pros: Simple
   - Cons: All-or-nothing, affects all rules

2. **Per-rule option:** `skip_cfg_test = true` in rule frontmatter
   - Pros: Fine-grained control
   - Cons: Config complexity, must add to each rule

3. **Query-level predicate:** `(#not-in-cfg-test?)` implemented in evaluator
   - Pros: Opt-in per query, reusable
   - Cons: Rust-specific, implementation complexity

4. **Automatic by category:** Rules tagged "code-quality" skip test code by default
   - Pros: Sensible defaults
   - Cons: Hidden behavior, may surprise users

**Proposed design:** Option 3 with category defaults from option 4.

Add a `(#not-in-test?)` predicate that:
- For Rust: walks up tree looking for `#[cfg(test)]` attribute on ancestor module/item
- For JS/TS: checks if inside `describe()`, `it()`, `test()` calls
- Returns true if NOT in test context

Rules like `rust/unwrap-in-impl` would use:
```scheme
((call_expression
  function: (field_expression field: (field_identifier) @_method)
  (#eq? @_method "unwrap")
  (#not-in-test?)) @match)
```

**Status:** Not implemented. Use inline allows or accept false positives for now.

## Implementation Notes

### Embedding Rules

Rules are in `crates/moss/src/commands/analyze/builtin_rules/`:

```rust
pub const BUILTIN_RULES: &[BuiltinRule] = &[
    BuiltinRule {
        id: "rust/todo-macro",
        content: include_str!("rust_todo_macro.scm"),
    },
    // ...
];
```

### Testing Rules

To test a rule against the moss codebase:

```bash
moss analyze rules --rule "rust/unnecessary-let"
```

To get SARIF output for IDE integration:

```bash
moss analyze rules --sarif > results.sarif
```
