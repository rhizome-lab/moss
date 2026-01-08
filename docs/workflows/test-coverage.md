# Test Coverage Workflow

Improving test coverage: identifying gaps, adding meaningful tests.

## Trigger

- Coverage below threshold
- New feature needs tests
- Bug found in untested code
- Pre-release quality check

## Goal

- Increase meaningful test coverage
- Cover critical code paths
- Catch regressions
- Document expected behavior

## Prerequisites

- Coverage tooling configured
- Understanding of code behavior
- Time for writing tests

## Decomposition Strategy

**Measure → Prioritize → Write → Verify**

```
1. MEASURE: Understand current state
   - Overall coverage percentage
   - Per-file/module coverage
   - Covered vs. uncovered lines
   - Branch coverage

2. PRIORITIZE: Decide what to cover
   - Critical business logic
   - Error-prone code
   - Frequently changed code
   - Complex functions

3. WRITE: Add tests strategically
   - Test behavior, not implementation
   - Cover edge cases
   - Write readable tests
   - Avoid testing trivial code

4. VERIFY: Confirm improvement
   - Coverage increased
   - Tests are meaningful
   - No flaky tests introduced
   - CI still fast enough
```

## Tools Used

| Phase | Tools |
|-------|-------|
| Measure | `cargo tarpaulin`, `nyc`, coverage tools |
| Prioritize | `moss analyze complexity`, `view` |
| Write | Test framework, `edit` |
| Verify | Coverage report, CI |

## Coverage Types

### Line Coverage
Percentage of lines executed by tests.
```
Covered:   let x = calc();  ✓
Uncovered: return err;      ✗ (no test triggers this)
```

### Branch Coverage
Percentage of branches (if/else/match arms) taken.
```rust
if condition {  // branch 1
    do_a();     // covered
} else {        // branch 2
    do_b();     // uncovered - no test for this case
}
```

### Function Coverage
Percentage of functions called by tests.

### Path Coverage
Percentage of unique paths through code tested.

## Prioritization Criteria

### High Priority
- **Business critical**: Payment, auth, data integrity
- **Complex**: High cyclomatic complexity
- **Buggy**: History of bugs
- **Changed often**: Frequently modified code

### Low Priority
- **Trivial**: Getters, simple delegation
- **Generated**: Auto-generated code
- **Framework glue**: Boilerplate required by framework
- **Dead code**: Unused code (remove instead of test)

## Writing Effective Tests

### Test Behavior, Not Implementation
```rust
// Good: Tests what the function does
#[test]
fn parse_email_accepts_valid_emails() {
    assert!(parse_email("user@example.com").is_ok());
}

// Bad: Tests how it's implemented
#[test]
fn parse_email_calls_regex_match() { ... }
```

### Cover Edge Cases
```rust
#[test]
fn handles_empty_input() { ... }

#[test]
fn handles_max_size_input() { ... }

#[test]
fn handles_unicode() { ... }

#[test]
fn handles_concurrent_access() { ... }
```

### Test Failure Paths
```rust
#[test]
fn returns_error_for_invalid_input() {
    let result = parse_email("not-an-email");
    assert!(result.is_err());
}
```

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| False coverage | Tests run code but don't verify | Add assertions |
| Flaky tests | Random failures | Fix or quarantine |
| Slow tests | CI takes too long | Optimize or parallelize |
| Brittle tests | Break on unrelated changes | Test interfaces not impl |

## Example Session

**Goal**: Improve coverage for payment module

```
Turn 1: Measure current coverage
  $(cargo tarpaulin --packages payments)
  → Overall: 45%
  → src/charge.rs: 23%
  → src/refund.rs: 12%
  → src/validate.rs: 89%

Turn 2: Identify uncovered critical code
  $(view src/charge.rs --uncovered)
  → charge_card(): 8 of 25 lines uncovered
  → handle_decline(): completely uncovered

Turn 3: Understand the code
  $(view src/charge.rs/charge_card)
  → Handles success path, retry logic, error cases
  → Uncovered: retry logic, network errors

Turn 4: Write test for retry logic
  $(edit src/charge_tests.rs)
  + #[test]
  + fn retries_on_temporary_failure() {
  +     let mock = MockGateway::failing_then_succeeding(2);
  +     let result = charge_card(&mock, amount);
  +     assert!(result.is_ok());
  +     assert_eq!(mock.call_count(), 3);
  + }

Turn 5: Write test for network error
  $(edit src/charge_tests.rs)
  + #[test]
  + fn returns_error_on_network_timeout() {
  +     let mock = MockGateway::timing_out();
  +     let result = charge_card(&mock, amount);
  +     assert!(matches!(result, Err(ChargeError::NetworkTimeout)));
  + }

Turn 6: Verify coverage improvement
  $(cargo tarpaulin --packages payments)
  → src/charge.rs: 78% (was 23%)
  → Total: 62% (was 45%)
```

## Coverage Targets

| Type of Code | Target |
|--------------|--------|
| Business logic | 90%+ |
| Infrastructure | 70%+ |
| UI/Presentation | 50%+ |
| Generated code | 0% (exclude) |

Note: 100% coverage is rarely worth the effort. Diminishing returns after ~80%.

## Test Organization

```
tests/
├── unit/           # Fast, isolated tests
├── integration/    # Tests with real dependencies
├── e2e/           # End-to-end tests
└── fixtures/      # Shared test data
```

## Anti-patterns

- **Coverage theater**: High percentage, low value tests
- **Testing mocks**: Tests that only verify mock behavior
- **Snapshot everything**: Brittle tests that break on any change
- **Testing getters**: Trivial tests that add no value
- **100% or nothing**: Obsessing over percentage vs. value

## Metrics to Track

- Coverage percentage (with context)
- Tests added vs. code added
- Bug escape rate (bugs in new code)
- Test reliability (flaky test count)
- Test suite duration

## See Also

- [Quality Audit](quality-audit.md) - Finding coverage gaps
- [Bug Investigation](bug-investigation.md) - Adding regression tests
- [Tech Debt](tech-debt.md) - Test debt as part of overall debt
