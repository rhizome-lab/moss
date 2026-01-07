# Reverse Engineering Code Workflow

Understanding undocumented, legacy, or poorly-structured source code when there's no documentation, the original authors are gone, and tests are sparse or nonexistent.

## Trigger

- Inherited legacy codebase with no docs
- Need to modify code you don't understand
- Debugging without context
- Custom DSL/framework with unclear semantics
- "It works, but nobody knows how"

## Goal

- Understand what the code does (behavior)
- Understand why it does it (intent, if recoverable)
- Document enough to work with it safely
- Identify high-risk areas (untested, complex, magical)

## Prerequisites

- Access to source code
- Ability to run the code (ideally)
- Time and patience
- Some domain knowledge helps

## Why This Is Hard

1. **No Rosetta Stone**: Unlike binary RE, you can read the code - but that doesn't mean you understand it
2. **Implicit knowledge**: Original authors made decisions for reasons not documented
3. **Layers of changes**: Code evolved, original design obscured by patches
4. **Custom abstractions**: Internal frameworks, DSLs, patterns you've never seen
5. **Testing gaps**: Can't verify your understanding without tests
6. **"Clever" code**: Optimizations, shortcuts, hacks that obscure intent

## Types of Undocumented Code

| Type | Challenge | Approach |
|------|-----------|----------|
| **Old but structured** | Just needs docs | Read and document |
| **Organically grown** | No clear architecture | Map dependencies, find entry points |
| **Custom DSL/framework** | Unknown semantics | Find examples, reverse engineer runtime |
| **Magic/metaprogramming** | Code generates code | Trace execution, dump generated code |
| **Spaghetti** | Everything depends on everything | Identify modules, cut dependencies |

## Core Strategy: Execute → Trace → Understand → Document

```
┌─────────────────────────────────────────────────────────┐
│                      EXECUTE                             │
│  Run the code, observe behavior                         │
│  Dynamic understanding before static                     │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                       TRACE                              │
│  Follow specific paths through the code                 │
│  Debugger, logging, print statements                    │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                    UNDERSTAND                            │
│  Build mental model from observations                   │
│  Identify patterns, abstractions, data flow             │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                     DOCUMENT                             │
│  Write what you learned for future readers              │
│  Add tests that encode your understanding               │
└─────────────────────────────────────────────────────────┘
```

## Phase 1: Execute and Observe

### Get It Running First

```bash
# Can you build it?
make
npm install && npm run build
cargo build

# Can you run it?
./program --help  # Does it have help?
./program test_input.txt  # What does it do?

# What happens with different inputs?
./program minimal_input.txt
./program complex_input.txt
./program invalid_input.txt  # Error handling?
```

### Observe External Behavior

Before reading code, understand behavior:

```bash
# What files does it read/write?
strace -e open,read,write ./program input.txt 2>&1 | head -50

# What network calls does it make?
strace -e connect,sendto,recvfrom ./program

# What's the output structure?
./program input.txt | head -20
./program input.txt | jq .  # If JSON

# How does it fail?
./program nonexistent.txt 2>&1  # Error messages?
./program malformed.txt 2>&1
```

### Create Input/Output Corpus

```bash
# Build a test corpus from existing behavior
for input in test_inputs/*; do
    ./program "$input" > "outputs/$(basename "$input").out" 2>&1
done

# Now you have known-good outputs for regression testing
```

## Phase 2: Trace Execution Paths

### Add Tracing

```python
# Python: Add print statements at key points
def mysterious_function(x):
    print(f"DEBUG: mysterious_function called with {x}")
    # ... existing code
    print(f"DEBUG: mysterious_function returning {result}")
    return result

# Or use logging
import logging
logging.basicConfig(level=logging.DEBUG)
logger = logging.getLogger(__name__)

def mysterious_function(x):
    logger.debug(f"mysterious_function({x})")
    ...
```

```rust
// Rust: dbg! macro
fn mysterious_function(x: i32) -> i32 {
    dbg!(x);
    let result = /* ... */;
    dbg!(result)
}
```

### Use Debugger

```bash
# Python
python -m pdb script.py input.txt
# (Pdb) break mysterious_function
# (Pdb) continue
# (Pdb) step
# (Pdb) print(local_variable)

# GDB for C/Rust
gdb ./program
# (gdb) break main
# (gdb) run input.txt
# (gdb) step
# (gdb) print variable

# Node
node --inspect script.js
# Connect Chrome DevTools
```

### Trace Call Graphs

```python
# Python: trace all function calls
import sys

def trace_calls(frame, event, arg):
    if event == 'call':
        print(f"CALL: {frame.f_code.co_name} in {frame.f_code.co_filename}:{frame.f_lineno}")
    return trace_calls

sys.settrace(trace_calls)
# Run your code
```

```bash
# Record call graph to file
python -c "
import sys
calls = []
def tracer(frame, event, arg):
    if event == 'call':
        calls.append(f'{frame.f_code.co_filename}:{frame.f_code.co_name}')
    return tracer
sys.settrace(tracer)
import your_module
your_module.main()
print('\n'.join(calls))
" > call_graph.txt
```

## Phase 3: Understand the Structure

### Map the Architecture

```
1. Find entry points
   - main(), if __name__ == "__main__", exported functions

2. Identify major modules
   - What does each directory contain?
   - What does each file do?

3. Trace data flow
   - Where does input come in?
   - How is it transformed?
   - Where does output go?

4. Find the "core"
   - What's the essential logic?
   - What's plumbing/boilerplate?
```

### Identify Patterns

```markdown
## Patterns Found

### Custom ORM
Files: db/*.py
Pattern: Objects with `save()`, `load()`, `find()` methods
Maps to: database tables named after class names

### Plugin System
Files: plugins/*.py
Pattern: Classes inheriting from `Plugin` base
Discovered via: `__subclasses__()` call in loader.py

### Event System
Files: events.py, handlers/*.py
Pattern: Functions registered with `@event_handler("event_name")`
Dispatch: events.emit("event_name", data)
```

### Understand Custom DSLs

If the code has its own language/configuration format:

```
1. Find examples of the DSL in use
   - Config files, test files, documentation

2. Find the parser/interpreter
   - How is the DSL read?
   - What data structure does it produce?

3. Find the runtime
   - How is the parsed DSL executed?
   - What are the primitives?

4. Map DSL constructs to code
   - DSL keyword X → calls function Y
   - DSL block type Z → creates object W
```

### Handle Metaprogramming

When code generates code:

```python
# Python: Classes created dynamically
def make_model(name, fields):
    return type(name, (Model,), fields)

# To understand: print what's generated
UserModel = make_model("User", {"name": str, "email": str})
print(UserModel.__dict__)  # See the actual class

# Or: set breakpoint in type() call
import pdb; pdb.set_trace()
```

```ruby
# Ruby: method_missing magic
class Api
  def method_missing(method, *args)
    http_request(method.to_s, args)
  end
end

# Understanding: search for method_missing, respond_to_missing?
# Trace what methods are actually called
```

## Phase 4: Document Your Understanding

### Add Comments to Code

```python
# BEFORE: No context
def process(data):
    x = data.get('x', 0)
    if x > THRESHOLD:
        return handle_high(data)
    return handle_low(data)

# AFTER: Document your understanding
def process(data):
    """
    Route data processing based on x value.

    Background: The high/low split exists because high-x values
    require more memory and are processed differently.
    (Discovered via git blame - commit abc123 from 2018)

    Args:
        data: Dict with at least 'x' key (int)

    Returns:
        Processed result dict
    """
    x = data.get('x', 0)  # Default to 0 if missing (found in test_data.json)
    if x > THRESHOLD:  # THRESHOLD = 1000, set in config.py
        return handle_high(data)
    return handle_low(data)
```

### Create Architecture Doc

```markdown
# System Architecture

## Overview
This system processes X and produces Y. It was originally built for Z.

## Entry Points
- `main.py`: CLI entry point
- `server.py`: Web server entry point
- `worker.py`: Background job processor

## Core Components

### Parser (`parser/`)
Reads input format and produces AST.
Key file: parser/grammar.py defines the syntax.

### Processor (`core/`)
Transforms AST into output.
Key file: core/transform.py has the main logic.

### Output (`output/`)
Formats results for different targets.
Key file: output/formatter.py

## Data Flow
```
Input → Parser → AST → Processor → Result → Formatter → Output
```

## Gotchas
- The `cache` module is load-bearing - don't disable
- `legacy/` is still used for edge cases (see LEGACY.md)
- Tests in `tests/integration/` are the real spec
```

### Write Characterization Tests

Tests that capture current behavior (even if you don't fully understand it):

```python
def test_process_returns_expected_output():
    """
    Characterization test: captures current behavior.
    Generated by running: ./program test_input.txt > expected_output.txt

    If this test fails, either:
    1. You broke something (revert!)
    2. You intentionally changed behavior (update this test)
    """
    input_data = load_fixture("test_input.txt")
    expected = load_fixture("expected_output.txt")

    result = process(input_data)

    assert result == expected
```

```python
# Generate characterization tests from corpus
for input_file in glob("test_inputs/*"):
    output = run_program(input_file)
    test_name = f"test_{Path(input_file).stem}"
    generate_test(test_name, input_file, output)
```

## Techniques for Specific Challenges

### Legacy Spaghetti Code

```
1. Identify the boundaries (entry/exit points)
2. Create integration tests at boundaries
3. Identify clusters of coupled code
4. Extract one cluster at a time into module
5. Add interface between modules
6. Iterate until manageable
```

### Custom Framework/DSL (e.g., viwo-style)

```
When dealing with a custom framework with insufficient testing:

1. FIND THE RUNTIME
   - What interprets/executes the DSL?
   - Where are the primitives implemented?

2. BUILD A MENTAL MODEL OF PRIMITIVES
   - List all DSL keywords/constructs
   - For each: what code does it execute?

3. CREATE MINIMAL EXAMPLES
   - Smallest possible use of each primitive
   - Run and observe behavior

4. BUILD UP COMPLEXITY
   - Combine primitives
   - Where do interactions cause bugs?

5. ADD TESTS AT PRIMITIVE LEVEL
   - Each primitive should have unit tests
   - Integration tests for combinations

6. DOCUMENT THE DSL
   - Grammar/syntax (formal or informal)
   - Semantics (what each construct does)
   - Common patterns/idioms
```

### Magic Numbers and Hardcoded Values

```python
# Found in code:
if x > 1073741824:
    handle_large(x)

# Investigation:
# 1073741824 = 2^30 = 1 GB in bytes
# This is a memory threshold

# Document it:
ONE_GB = 1073741824  # 2^30 bytes
if x > ONE_GB:
    handle_large(x)
```

### Implicit Dependencies

```python
# Code calls undefined function
result = do_magic(data)

# Where is do_magic?
# 1. Check imports (including star imports!)
# 2. Check __builtins__ modifications
# 3. Check module-level exec() or eval()
# 4. Check if injected at runtime
# 5. Search entire codebase: grep -r "def do_magic"

# Document when found:
# do_magic is defined in utils/magic.py, imported via
# "from utils import *" in __init__.py
```

## LLM-Specific Techniques

### Code Explanation

```
Given this function:
```python
def f(x):
    t = []
    for i in range(len(x)):
        if i == 0 or x[i] != x[i-1]:
            t.append([x[i], 1])
        else:
            t[-1][1] += 1
    return t
```

Explain what it does, give it a better name, and document it.
```

### Pattern Recognition

```
This codebase uses a custom pattern I don't recognize:

```python
class Handler(metaclass=RegisterMeta):
    __trigger__ = "event_name"

    def handle(self, data):
        ...
```

What pattern is this? How does it work? Where should I look for
the registration/dispatch logic?
```

### Dependency Mapping

```
Given these imports and function calls:

```python
from core import process
from utils.helpers import transform
from .local import validate

def main(data):
    v = validate(data)
    t = transform(v)
    return process(t)
```

Describe the data flow and what each module likely does.
```

## Common Mistakes

| Mistake | Why It's Bad | Prevention |
|---------|--------------|------------|
| Reading without running | Miss dynamic behavior | Run first, read second |
| Guessing semantics | Build wrong mental model | Verify assumptions with traces |
| Changing before understanding | Break unknown invariants | Add tests first |
| Local understanding only | Miss system interactions | Trace end-to-end |
| Not documenting | Waste effort, forget | Document as you learn |

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| Built wrong mental model | Tests fail, behavior unexpected | Add more traces, check assumptions |
| Missed dynamic behavior | Works differently at runtime | Add runtime tracing |
| Documentation already wrong | Docs contradict behavior | Trust behavior, update docs |
| Too much complexity | Overwhelmed | Focus on one path |

## Anti-patterns

- **Reading top to bottom**: Code isn't a novel - trace execution paths
- **Assuming comments are correct**: Trust code, verify comments
- **"I'll remember this"**: Write it down immediately
- **Perfect understanding before acting**: Iterate - understand enough to make progress
- **Ignoring tests (even if sparse)**: Tests often contain hidden specs

## Open Questions

### Automated Understanding

Can LLMs reliably build mental models of code?
- Simple code: yes
- Complex, stateful, side-effecting code: partial
- Need human verification

### Documentation Synthesis

Can we auto-generate accurate documentation from code?
- API docs: yes
- Behavioral docs: partially
- Intent/why docs: no (unless recovered from history)

### "Sufficient" Understanding

When do you understand code "enough"?
- Enough to make your specific change?
- Enough to refactor safely?
- Enough to extend significantly?

Different goals require different depth.

## See Also

- [Codebase Onboarding](codebase-onboarding.md) - Similar but for documented code
- [Reverse Engineering Binary](reverse-engineering-binary.md) - Binary format version
- [Bug Investigation](bug-investigation.md) - Often requires understanding unfamiliar code
