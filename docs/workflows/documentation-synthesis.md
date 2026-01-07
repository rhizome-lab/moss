# Documentation Synthesis Workflow

Generating documentation from code - the inverse of code synthesis. Extracting human-readable explanations from implementations.

## Trigger

- Undocumented codebase needs docs
- API documentation out of sync with code
- Need to onboard new team members
- Compliance requires documentation
- Open-sourcing internal code

## Goal

- Accurate documentation that reflects actual code
- Multiple levels: API reference, tutorials, architecture
- Documentation that stays in sync with code
- Useful to target audience (developers, users, operators)

## Prerequisites

- Working code to document
- Understanding of target audience
- Documentation format/tooling decided
- Examples of good documentation in the domain

## Why This Is Hard

1. **Intent is lost**: Code shows what, rarely why
2. **Multiple audiences**: Users vs developers vs operators
3. **Abstraction level**: Too detailed is useless, too high-level is vague
4. **Freshness**: Docs rot faster than code
5. **Examples**: Readers need examples, code doesn't provide them
6. **Edge cases**: What to document vs what to skip

## Types of Documentation

| Type | Audience | Content | Source |
|------|----------|---------|--------|
| **API Reference** | Developers | Signatures, types, returns | Code + docstrings |
| **Tutorials** | New users | Step-by-step guides | Examples + narration |
| **How-to Guides** | Users | Task-focused recipes | Common use cases |
| **Architecture** | Maintainers | Design decisions, structure | Code + history |
| **Operations** | Operators | Deploy, configure, monitor | Config + procedures |

## Core Strategy: Extract → Organize → Generate → Validate

```
┌─────────────────────────────────────────────────────────┐
│                      EXTRACT                             │
│  Pull information from code: signatures, types, comments│
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                     ORGANIZE                             │
│  Structure for audience: group, order, hierarchy        │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                     GENERATE                             │
│  Write prose, add examples, explain context             │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                     VALIDATE                             │
│  Technical accuracy, completeness, usability            │
└─────────────────────────────────────────────────────────┘
```

## Phase 1: Extract Information

### API Signatures

```rust
// Rust: extract from source
pub fn connect(host: &str, port: u16, config: Config) -> Result<Connection, Error>

// Extract:
// - Function name: connect
// - Parameters: host (string), port (u16), config (Config)
// - Return: Result<Connection, Error>
// - Visibility: public
```

```python
# Python: use introspection
import inspect

def extract_signature(func):
    sig = inspect.signature(func)
    doc = inspect.getdoc(func)
    source = inspect.getsource(func)
    return {
        'name': func.__name__,
        'signature': str(sig),
        'docstring': doc,
        'source': source,
    }
```

### Existing Documentation

```bash
# Find existing docs
grep -r "///" --include="*.rs" src/       # Rust doc comments
grep -r '"""' --include="*.py" src/       # Python docstrings
grep -r "/\*\*" --include="*.js" src/     # JSDoc

# Extract TODO/FIXME/HACK comments
grep -rn "TODO\|FIXME\|HACK\|XXX" src/
```

### Type Information

```typescript
// TypeScript: rich type info
interface Config {
  /** Maximum retry attempts */
  maxRetries: number;
  /** Timeout in milliseconds */
  timeout: number;
  /** TLS configuration, if enabled */
  tls?: TlsConfig;
}

// Extract: field names, types, optionality, doc comments
```

### Code Structure

```bash
# Module/file organization
tree src/ -I '__pycache__|*.pyc'

# Public exports
grep -r "^pub " --include="*.rs" src/ | grep -v "pub(crate)"
grep -r "^export " --include="*.ts" src/

# Dependencies between modules
# (use moss analyze or similar tools)
```

### Git History for Context

```bash
# Why was this added?
git log -p --follow -- src/auth.rs | head -100

# Who knows about this code?
git shortlog -sn -- src/auth.rs

# Recent changes
git log --oneline -20 -- src/
```

## Phase 2: Organize for Audience

### API Reference Structure

```markdown
# API Reference

## Core Types
- Connection - Main client connection
- Config - Configuration options
- Error - Error types

## Functions

### Connection Management
- connect() - Establish connection
- disconnect() - Close connection
- reconnect() - Reconnect after failure

### Data Operations
- send() - Send data
- receive() - Receive data
- query() - Execute query
```

### Tutorial Structure

```markdown
# Getting Started

## Installation
...

## Quick Start
...

## Basic Concepts
### Connections
### Queries
### Results

## Your First Application
### Step 1: Connect
### Step 2: Query
### Step 3: Process Results

## Next Steps
```

### Architecture Document Structure

```markdown
# Architecture

## Overview
High-level description and diagram

## Components
### Component A
- Purpose
- Key types/functions
- Dependencies

### Component B
...

## Data Flow
How data moves through the system

## Design Decisions
### Why X instead of Y
Reasoning, tradeoffs, alternatives considered

## Extension Points
How to add new functionality
```

## Phase 3: Generate Documentation

### API Reference from Code

```python
def generate_api_docs(module):
    """Generate markdown API docs from Python module."""
    docs = []

    for name, obj in inspect.getmembers(module):
        if inspect.isfunction(obj) and not name.startswith('_'):
            sig = inspect.signature(obj)
            docstring = inspect.getdoc(obj) or "No description"

            docs.append(f"""
### {name}

```python
{name}{sig}
```

{docstring}

**Parameters:**
{format_params(sig.parameters)}

**Returns:** {format_return(sig.return_annotation)}
""")

    return '\n'.join(docs)
```

### Examples from Tests

```python
def extract_examples_from_tests(test_file):
    """Extract usage examples from test code."""
    # Tests often show real usage patterns
    # Extract the "arrange" and "act" parts

    # Look for patterns like:
    # def test_basic_usage():
    #     client = Client()           # <- setup
    #     result = client.query(...)  # <- usage
    #     assert result == expected   # <- (skip)
```

### Prose Generation (LLM-assisted)

```
Given this function signature and implementation:

```rust
pub fn parse_config(path: &Path) -> Result<Config, ConfigError> {
    let content = fs::read_to_string(path)?;
    let config: Config = toml::from_str(&content)?;
    validate_config(&config)?;
    Ok(config)
}
```

Generate documentation that explains:
1. What the function does (1-2 sentences)
2. Parameters and their meaning
3. Return value and possible errors
4. Example usage
```

### Architecture from Code Analysis

```bash
# Generate dependency graph
cargo depgraph | dot -Tpng > deps.png

# Analyze module coupling
moss analyze --complexity src/

# Identify entry points
grep -r "fn main" --include="*.rs" src/
```

## Phase 4: Validation

### Technical Accuracy

```python
# Doctest: examples in docs are tested
def add(a: int, b: int) -> int:
    """Add two numbers.

    >>> add(1, 2)
    3
    >>> add(-1, 1)
    0
    """
    return a + b
```

```bash
# Run doctests
python -m doctest module.py
cargo test --doc
```

### Completeness Check

```python
def check_documented(module):
    """Check all public items are documented."""
    missing = []

    for name, obj in inspect.getmembers(module):
        if not name.startswith('_'):
            if inspect.isfunction(obj) or inspect.isclass(obj):
                if not inspect.getdoc(obj):
                    missing.append(name)

    return missing
```

```bash
# Rust: warn on missing docs
#![warn(missing_docs)]
cargo doc

# Python: pydocstyle
pydocstyle src/

# JavaScript: eslint-plugin-jsdoc
eslint --rule "jsdoc/require-jsdoc: error" src/
```

### Freshness Check

```bash
# Find docs older than code
for doc in docs/*.md; do
    code_file=$(basename "$doc" .md)
    if [ "src/$code_file.rs" -nt "$doc" ]; then
        echo "STALE: $doc"
    fi
done

# Better: CI check that docs regenerate cleanly
cargo doc
git diff --exit-code docs/
```

### User Testing

```markdown
## Documentation Review Checklist

### New User Test
- [ ] Can follow tutorial without prior knowledge?
- [ ] Are prerequisites clearly stated?
- [ ] Do examples actually work when copy-pasted?

### Expert Review
- [ ] Technically accurate?
- [ ] Missing important details?
- [ ] Misleading simplifications?

### Task-Based Test
- [ ] Can complete common tasks using only docs?
- [ ] Error messages lead to relevant doc sections?
```

## Documentation Tools

| Tool | Language | Output |
|------|----------|--------|
| rustdoc | Rust | HTML, tests examples |
| Sphinx | Python | HTML, PDF, many formats |
| JSDoc | JavaScript | HTML |
| TypeDoc | TypeScript | HTML, markdown |
| godoc | Go | HTML |
| Doxygen | C/C++ | HTML, PDF, LaTeX |
| mdBook | Any | Book-style HTML |
| MkDocs | Any | Static site |

## LLM-Specific Techniques

### Code-to-Prose

```
Given this implementation:

```python
def retry(func, max_attempts=3, delay=1.0, backoff=2.0):
    attempt = 0
    while attempt < max_attempts:
        try:
            return func()
        except Exception as e:
            attempt += 1
            if attempt >= max_attempts:
                raise
            time.sleep(delay * (backoff ** attempt))
```

Write a docstring that explains:
- What the function does
- Each parameter's purpose and default
- Retry behavior (exponential backoff)
- When it raises vs returns
```

### Architecture Narration

```
Given this module structure:

src/
├── client/
│   ├── connection.rs
│   ├── pool.rs
│   └── retry.rs
├── protocol/
│   ├── parser.rs
│   ├── serializer.rs
│   └── types.rs
└── server/
    ├── handler.rs
    └── router.rs

And these dependencies:
- client -> protocol
- server -> protocol

Generate an architecture overview explaining:
- What each module does
- How they relate
- Data flow through the system
```

### Example Generation

```
Given this API:

```rust
pub fn query(conn: &Connection, sql: &str, params: &[Value]) -> Result<Rows, Error>
```

Generate 3 usage examples:
1. Simple query without parameters
2. Query with parameters (preventing SQL injection)
3. Handling errors properly
```

### Gap Detection

```
Here is the current documentation for module X:
[current docs]

Here is the actual code:
[implementation]

Identify:
1. Documented features that don't exist in code
2. Code features not mentioned in docs
3. Documented behavior that differs from implementation
```

## Keeping Docs Fresh

### Doc Generation in CI

```yaml
# GitHub Actions
- name: Generate docs
  run: cargo doc --no-deps

- name: Check for staleness
  run: |
    git diff --exit-code target/doc/
    if [ $? -ne 0 ]; then
      echo "Documentation is stale. Regenerate with 'cargo doc'"
      exit 1
    fi
```

### Living Documentation

```rust
// Docs that test themselves
/// ```
/// let config = Config::default();
/// assert_eq!(config.timeout, Duration::from_secs(30));
/// ```
pub struct Config {
    pub timeout: Duration,
}
```

### Doc-Code Proximity

```rust
// Keep docs close to code (same file)
// Not: separate docs/api.md that drifts

/// Connects to the server.
///
/// # Arguments
/// * `host` - Server hostname
/// * `port` - Server port (default: 8080)
///
/// # Errors
/// Returns `ConnectionError` if connection fails.
pub fn connect(host: &str, port: u16) -> Result<Connection, Error> {
    // ...
}
```

## Common Patterns

### README Template

```markdown
# Project Name

One-line description.

## Installation

```bash
cargo install project-name
```

## Quick Start

```rust
use project_name::Client;

let client = Client::new("localhost:8080");
let result = client.query("SELECT 1")?;
```

## Documentation

- [API Reference](https://docs.rs/project-name)
- [User Guide](docs/guide.md)
- [Examples](examples/)

## License

MIT
```

### Function Documentation Template

```rust
/// Brief one-line description.
///
/// Longer description if needed. Explain the purpose,
/// not just what the code does.
///
/// # Arguments
///
/// * `param1` - Description of first parameter
/// * `param2` - Description of second parameter
///
/// # Returns
///
/// Description of return value.
///
/// # Errors
///
/// * `ErrorType1` - When this happens
/// * `ErrorType2` - When that happens
///
/// # Examples
///
/// ```
/// let result = function(arg1, arg2)?;
/// assert_eq!(result, expected);
/// ```
///
/// # Panics
///
/// Panics if invariant is violated (if applicable).
///
/// # Safety
///
/// Describe safety requirements (if unsafe).
pub fn function(param1: Type1, param2: Type2) -> Result<Output, Error> {
    // ...
}
```

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| Stale docs | CI diff check, user complaints | Regenerate, add freshness checks |
| Missing coverage | Lint warnings, coverage tool | Add missing docs |
| Wrong abstraction | User confusion, questions | Rewrite for audience |
| Broken examples | Doctest failures | Fix examples, test in CI |

## Anti-patterns

- **Writing docs once**: Docs need maintenance like code
- **Documenting implementation**: Document interface and behavior, not how
- **Copy-paste from code**: Docs should add value, not repeat
- **One size fits all**: Different audiences need different docs
- **Hiding complexity**: Acknowledge limitations and edge cases

## Open Questions

### Automation Limits

How much documentation can be auto-generated?
- Signatures, types: yes
- Behavior description: partially
- Intent and rationale: rarely (unless in commit messages)
- Good examples: needs curation

### Documentation as Code

Should docs be:
- Separate files (easier to edit, can drift)
- In code comments (close to source, limited formatting)
- Generated from tests (always accurate, limited prose)

### Multi-Language Projects

How to unify docs across:
- Rust backend + TypeScript frontend
- Python library with C extension
- Polyglot microservices

## See Also

- [Code Synthesis](code-synthesis.md) - Inverse problem: code from docs
- [Reverse Engineering Code](reverse-engineering-code.md) - Understanding undocumented code
- [Codebase Onboarding](codebase-onboarding.md) - Using docs for onboarding

