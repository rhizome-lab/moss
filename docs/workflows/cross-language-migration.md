# Cross-Language Migration Workflow

Porting code from one programming language to another - Python to Rust, JavaScript to TypeScript, Java to Kotlin, etc.

## Trigger

- Performance requirements demand different language
- Type safety requirements (JS → TS)
- Platform constraints (need native code, WASM, mobile)
- Team expertise shift
- Ecosystem/library availability
- End-of-life for current language/runtime

## Goal

- Functionally equivalent code in new language
- Idiomatic to the target language (not just "X in Y's clothing")
- Passes existing tests (if portable) or equivalent new tests
- Maintains or improves performance characteristics
- Preserves important behaviors (error handling, edge cases)

## Prerequisites

- Clear understanding of original code behavior
- Test suite (the most important prerequisite)
- Knowledge of both source and target languages
- Understanding of semantic differences between languages

## Why Cross-Language Migration Is Hard

1. **Languages aren't isomorphic**: Not every concept translates cleanly
2. **Idiom differences**: What's natural in Python may be unnatural in Rust
3. **Type system gaps**: Dynamic to static typing requires decisions
4. **Runtime differences**: GC vs ownership, exceptions vs Result types
5. **Ecosystem differences**: Libraries, patterns, conventions
6. **Subtle semantics**: Integer overflow, floating point, string encoding

## Types of Migration

| Migration | Challenge Level | Key Concerns |
|-----------|-----------------|--------------|
| JS → TS | Low | Add types to existing code, few semantic changes |
| Python → Typed Python | Low | Type annotations, gradual |
| Java → Kotlin | Medium | Null safety, idiom translation |
| Python → Go | Medium | Error handling, types, concurrency model |
| Python → Rust | High | Ownership, lifetimes, error handling |
| C → Rust | High | Unsafe blocks, memory model |
| Dynamic → Static (any) | Medium-High | Type decisions pervade everything |

## Core Strategy: Understand → Map → Translate → Verify

```
┌─────────────────────────────────────────────────────────┐
│                    UNDERSTAND                            │
│  What does the code actually do? All behaviors.         │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                       MAP                                │
│  How do source concepts map to target concepts?         │
│  Where are the semantic gaps?                           │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                    TRANSLATE                             │
│  Convert code, making idiomatic choices                 │
│  Address gaps with target-appropriate solutions         │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                      VERIFY                              │
│  Test against original behavior                         │
│  Property testing to find edge case divergence          │
└─────────────────────────────────────────────────────────┘
```

## Phase 1: Understand the Source

### Document Behavior (Not Just Code)

```python
# Source code tells you WHAT, not WHY or WHAT-IF

def process_user(user_data):
    """
    Questions to answer:
    - What types can user_data be? (dict? object? None?)
    - What happens if required fields are missing?
    - What happens with invalid data types?
    - What are the side effects? (logging? database?)
    - What exceptions can be raised?
    - What are the performance characteristics?
    """
    if not user_data:
        return None  # None input → None output
    name = user_data.get("name", "Anonymous")  # Missing name → default
    # ...
```

### Extract Test Suite as Specification

```bash
# Tests are the best specification
# Enumerate all test cases

pytest tests/ --collect-only
# List all tests for the module being ported

# Run with coverage to see what's exercised
pytest tests/test_user.py --cov=src/user --cov-report=term-missing
```

### Identify Edge Cases

```python
# What happens with:
process_user(None)           # Null/None
process_user({})             # Empty
process_user({"name": ""})   # Empty string
process_user({"name": 123})  # Wrong type
process_user({"extra": 1})   # Unknown fields
```

Document every edge case - these are where translations diverge.

## Phase 2: Map Concepts

### Create Translation Table

```markdown
## Python → Rust Translation Map

| Python | Rust | Notes |
|--------|------|-------|
| `None` | `Option<T>` | Must choose T |
| `dict` | `HashMap<K, V>` or struct | Struct preferred for known keys |
| `list` | `Vec<T>` | Must choose T |
| `try/except` | `Result<T, E>` | Different control flow |
| `class` | `struct` + `impl` | No inheritance |
| `self.x = y` | `self.x = y` (similar) | But ownership matters |
| Dynamic typing | Static typing | Every variable needs type |
| `**kwargs` | No direct equivalent | Use builder pattern or struct |
| `duck typing` | Traits | Must define interfaces explicitly |
```

### Identify Semantic Gaps

```
Python:
    x = some_dict.get("key", default)  # Returns default if missing

Rust:
    let x = some_map.get("key").unwrap_or(&default);  # Similar
    // But: get returns Option<&V>, not Option<V>
    // And: what if we need owned value, not reference?

Gap: Ownership semantics require different approach
```

### Plan Type Decisions

Moving from dynamic to static typing requires decisions:

```python
# Python: types are implicit/dynamic
def process(data):
    return data["value"] * 2

# Questions for translation:
# - What type is data? Dict? Custom class?
# - What type is data["value"]? int? float? str?
# - What should happen if key is missing?
# - What should happen if value isn't numeric?
```

```rust
// Rust: must decide explicitly
fn process(data: &HashMap<String, i64>) -> i64 {
    data.get("value").copied().unwrap_or(0) * 2
}
// Or with custom struct:
fn process(data: &Data) -> i64 {
    data.value * 2
}
```

## Phase 3: Translation Strategies

### Strategy 1: Direct Translation (Simple Cases)

```python
# Python
def add(a, b):
    return a + b
```

```rust
// Rust - direct translation
fn add(a: i64, b: i64) -> i64 {
    a + b
}
```

Works when concepts map 1:1.

### Strategy 2: Idiomatic Rewrite

```python
# Python - dictionary access with default
def get_name(user):
    return user.get("name", "Anonymous")
```

```rust
// Rust - don't just translate, use idiomatic patterns
fn get_name(user: &User) -> &str {
    user.name.as_deref().unwrap_or("Anonymous")
}

// Or with typed struct:
struct User {
    name: Option<String>,
}

impl User {
    fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or("Anonymous")
    }
}
```

### Strategy 3: Pattern Transformation

```python
# Python - exception-based error handling
def divide(a, b):
    try:
        return a / b
    except ZeroDivisionError:
        return None
```

```rust
// Rust - Result-based error handling (different pattern)
fn divide(a: f64, b: f64) -> Option<f64> {
    if b == 0.0 {
        None
    } else {
        Some(a / b)
    }
}
```

### Strategy 4: Architecture Change

Sometimes direct translation isn't possible:

```python
# Python - inheritance hierarchy
class Animal:
    def speak(self): pass

class Dog(Animal):
    def speak(self): return "woof"

class Cat(Animal):
    def speak(self): return "meow"
```

```rust
// Rust - traits instead of inheritance
trait Animal {
    fn speak(&self) -> &str;
}

struct Dog;
impl Animal for Dog {
    fn speak(&self) -> &str { "woof" }
}

struct Cat;
impl Animal for Cat {
    fn speak(&self) -> &str { "meow" }
}
```

## Phase 4: Common Migration Patterns

### Null Handling (Dynamic → Static)

```python
# Python: None is everywhere
def process(x):
    if x is None:
        return "default"
    return x.upper()
```

```rust
// Rust: Option makes null explicit
fn process(x: Option<&str>) -> String {
    match x {
        Some(s) => s.to_uppercase(),
        None => "default".to_string(),
    }
}
// Or:
fn process(x: Option<&str>) -> String {
    x.map(|s| s.to_uppercase())
     .unwrap_or_else(|| "default".to_string())
}
```

### Error Handling

```python
# Python: exceptions
def read_file(path):
    try:
        with open(path) as f:
            return f.read()
    except FileNotFoundError:
        return None
    except PermissionError:
        raise  # Re-raise
```

```rust
// Rust: Result types
fn read_file(path: &str) -> Result<String, io::Error> {
    fs::read_to_string(path)
}

// Or with custom error enum for fine-grained handling:
enum ReadError {
    NotFound,
    PermissionDenied,
    Other(io::Error),
}
```

### Collections

```python
# Python: list comprehension
squares = [x**2 for x in range(10) if x % 2 == 0]
```

```rust
// Rust: iterator chains
let squares: Vec<i64> = (0..10)
    .filter(|x| x % 2 == 0)
    .map(|x| x * x)
    .collect();
```

### Classes to Structs

```python
# Python class
class User:
    def __init__(self, name, email):
        self.name = name
        self.email = email

    def display(self):
        return f"{self.name} <{self.email}>"
```

```rust
// Rust struct + impl
struct User {
    name: String,
    email: String,
}

impl User {
    fn new(name: String, email: String) -> Self {
        Self { name, email }
    }

    fn display(&self) -> String {
        format!("{} <{}>", self.name, self.email)
    }
}
```

## Phase 5: Verification

### Port Tests First

```bash
# Ideal workflow:
# 1. Port test suite to target language
# 2. Run tests (all fail - no implementation yet)
# 3. Port implementation
# 4. Run tests (should pass)
```

### Cross-Language Testing

If you can run both versions:

```python
# Generate test cases
import json

test_cases = [
    {"input": {"name": "Alice"}, "expected": "ALICE"},
    {"input": {}, "expected": "ANONYMOUS"},
    {"input": None, "expected": None},
]

with open("test_cases.json", "w") as f:
    json.dump(test_cases, f)
```

```rust
// Run against same test cases
#[test]
fn test_from_shared_cases() {
    let cases: Vec<TestCase> = serde_json::from_str(
        include_str!("test_cases.json")
    ).unwrap();

    for case in cases {
        assert_eq!(process(case.input), case.expected);
    }
}
```

### Property-Based Testing

Find edge cases where implementations diverge:

```python
# Python: hypothesis
from hypothesis import given, strategies as st

@given(st.text())
def test_process_any_string(s):
    result = process(s)
    assert isinstance(result, str)
    # Save inputs for Rust to test
```

```rust
// Rust: proptest
proptest! {
    #[test]
    fn test_process_any_string(s in ".*") {
        let result = process(&s);
        // Same assertions
    }
}
```

### Differential Testing

Run both implementations, compare outputs:

```bash
# Generate inputs
python generate_test_inputs.py > inputs.json

# Run Python version
python run_original.py < inputs.json > python_outputs.json

# Run Rust version
./run_ported < inputs.json > rust_outputs.json

# Compare
diff python_outputs.json rust_outputs.json
```

## Incremental Migration

For large codebases, migrate incrementally:

### FFI Bridge

```python
# Python calling Rust via FFI (PyO3)
import my_rust_lib

# Old Python implementation
def old_process(x):
    return x.upper()

# New Rust implementation called from Python
def new_process(x):
    return my_rust_lib.process(x)

# Gradual migration: replace calls one by one
```

### Strangler Fig Pattern

```
1. New code written in target language
2. Old code remains in source language
3. Calls gradually redirect to new code
4. Old code eventually unused, deleted

[Client] → [Router] → [Old Python Service]
                   ↘ [New Rust Service]
```

### Feature Flag Migration

```python
if feature_flags.use_rust_processor:
    result = rust_lib.process(data)
else:
    result = python_process(data)
```

## LLM-Specific Techniques

### Translation Assistance

```
Given Python function:
```python
def process_user(user_data):
    if not user_data:
        return None
    name = user_data.get("name", "Anonymous")
    return {"greeting": f"Hello, {name}!"}
```

Translate to idiomatic Rust, noting:
1. Type decisions made
2. Semantic differences from Python
3. Edge cases that behave differently
```

### Edge Case Discovery

```
Compare these implementations:

Python:
```python
def divide(a, b):
    return a / b if b != 0 else None
```

Rust:
```rust
fn divide(a: f64, b: f64) -> Option<f64> {
    if b != 0.0 { Some(a / b) } else { None }
}
```

Find inputs where they behave differently.
(Hint: floating point edge cases, integer vs float division, etc.)
```

## Common Mistakes

| Mistake | Why It's Bad | Prevention |
|---------|--------------|------------|
| Literal translation | Unidiomatic, hard to maintain | Learn target idioms |
| Ignoring type decisions | Implicit assumptions become bugs | Document every type choice |
| Not porting tests | No verification | Port tests first |
| Big bang migration | Hard to debug failures | Incremental with FFI |
| Same architecture | May not fit target language | Redesign where needed |

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| Semantic divergence | Tests fail, behavior differs | Add test for specific case |
| Type mismatch | Compile error or wrong behavior | Review type mapping |
| Performance regression | Benchmarks slower | Profile, optimize |
| Missing edge case | Bug in production | Add property testing |

## Open Questions

### When to Redesign vs Translate

Direct translation preserves familiarity but may not be idiomatic.
Complete redesign is idiomatic but loses connection to original.
Where's the balance?

### Automated Translation

Can LLMs reliably translate code between languages?
- Simple functions: yes
- Complex systems: partially
- Verification: still needed

### Type Inference Assistance

When migrating from dynamic to static:
- Can types be inferred from tests?
- Can LLMs suggest appropriate types?
- What about generic/parameterized types?

## See Also

- [Code Synthesis](code-synthesis.md) - D×C verification applies to translation too
- [Codebase Onboarding](codebase-onboarding.md) - Understand before translating
