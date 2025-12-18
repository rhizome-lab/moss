# Quickstart

This guide walks you through your first code synthesis with Moss.

## Basic Synthesis

The simplest synthesis command takes a description:

```bash
moss synthesize "Create a function that adds two numbers"
```

Output:
```python
def generated_function(*args, **kwargs):
    """Auto-generated function.

    Original request: Create a function that adds two numbers
    """
    # TODO: Implement this function
    raise NotImplementedError("generated_function not yet implemented")
```

!!! note
    The default generator produces placeholder stubs. Configure an LLM provider for actual implementations.

## Adding Type Information

Type signatures help the synthesis framework understand the problem better:

```bash
moss synthesize "Create a function that adds two numbers" \
    --type "(a: int, b: int) -> int"
```

## Dry Run Mode

See how Moss would decompose a problem without executing:

```bash
moss synthesize "Build a REST API with CRUD for users" --dry-run
```

Output:
```
Specification
-------------
[*] Description: Build a REST API with CRUD for users

[>] Analyzing decomposition...
[*] Best strategy: pattern_based (score: 0.40)

[>] Decomposition (5 subproblems):
  0. Implement Create operation (POST /users)
  1. Implement Read operation (GET /users/:id) [deps: (0,)]
  2. Implement List operation (GET /users) [deps: (1,)]
  3. Implement Update operation (PUT /users/:id) [deps: (2,)]
  4. Implement Delete operation (DELETE /users/:id) [deps: (3,)]

[*] (dry-run mode, stopping before synthesis)
```

## Pattern Recognition

Moss recognizes common patterns and decomposes them automatically:

| Pattern | Keywords | Example |
|---------|----------|---------|
| CRUD API | crud, rest, api | "Build a REST API for products" |
| Authentication | auth, login | "Implement user authentication" |
| Validation | validate, check | "Create input validation" |
| Search | search, filter | "Build a search feature" |
| Caching | cache, memoize | "Add caching layer" |

## Adding Examples

Provide input/output examples for better synthesis:

```bash
moss synthesize "Create a function that reverses a string" \
    --example "hello" "olleh" \
    --example "world" "dlrow"
```

## JSON Output

For programmatic use:

```bash
moss synthesize "Add two numbers" --json
```

```json
{
  "success": true,
  "code": "def generated_function...",
  "iterations": 1,
  "strategy": "pattern_based",
  "metadata": {}
}
```

## Next Steps

- [Synthesis Overview](../synthesis/overview.md) - How decomposition works
- [Strategies](../synthesis/strategies.md) - Available decomposition strategies
- [Generators](../synthesis/generators.md) - Code generation approaches
