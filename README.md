# Moss

Headless agent orchestration layer for AI engineering.

Moss implements a "Compiled Context" approach that prioritizes architectural awareness (AST-based understanding) over raw text processing, with verification loops ensuring correctness before output.

## Features

- **Event-Driven Architecture**: Async communication via EventBus (`UserMessage`, `PlanGenerated`, `ToolCall`, `ValidationFailed`, `ShadowCommit`)
- **Shadow Git**: Atomic commits per tool call with rollback via git reset
- **AST-Aware Editing**: Structural editing with fuzzy anchor matching
- **Validation Loop**: Domain-specific verification (syntax, linter, tests) with automatic retry
- **Policy Engine**: Safety enforcement (velocity checks, quarantine, rate limiting, path blocking)
- **Memory System**: Episodic and semantic stores for learning from past actions
- **Multi-Agent Support**: Ticket-based coordination with isolated workers
- **Configuration DSL**: Distro-based configuration with inheritance
- **Code Synthesis**: Plugin-based code generation with decomposition strategies and LLM integration

## Architecture

```
User Request
     │
     ▼
┌─────────────┐
│ Config DSL  │  ← MossConfig, Distros
└─────────────┘
     │
     ▼
┌─────────────┐
│  Event Bus  │  ← Async message passing
└─────────────┘
     │
     ├───────────────┬────────────────┐
     ▼               ▼                ▼
┌─────────┐   ┌───────────┐   ┌────────────┐
│ Manager │   │  Context  │   │   Policy   │
│ (Agents)│   │   Host    │   │   Engine   │
└─────────┘   └───────────┘   └────────────┘
     │               │                │
     ▼               ▼                │
┌─────────┐   ┌───────────┐          │
│ Workers │   │   Views   │          │
│ (Tasks) │   │ (Skeleton,│          │
└─────────┘   │  Deps)    │          │
     │        └───────────┘          │
     │               │                │
     └───────┬───────┘                │
             ▼                        │
       ┌───────────┐                  │
       │  Patches  │  ← AST-aware edits
       └───────────┘
             │
             ▼
       ┌───────────┐
       │Shadow Git │  ← Atomic commits
       └───────────┘
             │
             ▼
       ┌───────────┐
       │ Validator │◄─────────────────┘
       │   Chain   │
       └───────────┘
             │
             ▼ (retry loop if errors)
       ┌───────────┐
       │  Commit   │
       │  Handle   │
       └───────────┘
```

## Installation

```bash
# Using pip
pip install moss

# Using uv
uv add moss
```

## Quick Start

### Initialize a Project

```bash
# Initialize in current directory
moss init

# Initialize with a specific distro
moss init --distro strict
```

This creates:
- `moss_config.py` - Project configuration
- `.moss/` - Runtime data directory

### Run a Task

```bash
# Submit a task
moss run "Add input validation to the login form"

# With priority
moss run "Fix critical security bug" --priority critical

# With constraints
moss run "Refactor auth module" -c "no-tests" -c "preserve-api"
```

### Check Status

```bash
# Show current status
moss status

# Verbose output
moss status -v
```

### Code Synthesis

```bash
# Synthesize code from a specification
moss synthesize "Create a function that validates email addresses"

# Show the decomposition strategy without generating code
moss synthesize "Build a REST API for user management" --dry-run

# Show detailed decomposition tree
moss synthesize "Implement a binary search tree" --show-decomposition

# Use a specific code generator
moss synthesize "Parse JSON config file" --generator llm  # LLM-based
moss synthesize "CRUD operations for users" --generator template  # Template-based
```

### Configuration

```bash
# Show current configuration
moss config

# Validate configuration
moss config --validate

# List available distros
moss distros
```

## Configuration

Moss uses a Python-based configuration DSL:

```python
# moss_config.py
from pathlib import Path
from moss.config import MossConfig, get_distro

# Start from a base distro
base = get_distro("python")
config = base.create_config()

# Customize
config = (
    config
    .with_project(Path(__file__).parent, "my-project")
    .with_validators(syntax=True, ruff=True, pytest=True)
    .with_policies(velocity=True, quarantine=True, path=True)
    .with_loop(max_iterations=10, auto_commit=True)
)
```

### Built-in Distros

| Distro | Description |
|--------|-------------|
| `python` | Python projects with syntax + ruff validation |
| `strict` | Strict mode with pytest and lower iteration limit |
| `lenient` | Relaxed settings, higher iteration limit |
| `fast` | Quick iterations with tight timeout |

## Programmatic Usage

```python
import asyncio
from pathlib import Path

from moss import (
    EventBus,
    ShadowGit,
    create_manager,
    create_api_handler,
    TaskRequest,
)

async def main():
    # Set up components
    event_bus = EventBus()
    shadow_git = ShadowGit(Path("."))
    manager = create_manager(shadow_git, event_bus)
    handler = create_api_handler(manager, event_bus)

    # Create a task
    request = TaskRequest(
        task="Implement user authentication",
        priority="high",
    )
    response = await handler.create_task(request)
    print(f"Task created: {response.request_id}")

    # Check status
    status = await handler.get_task_status(response.request_id)
    print(f"Status: {status.status}")

asyncio.run(main())
```

## Core Components

### Event Bus

Central async communication hub:

```python
from moss import EventBus, Event, EventType

bus = EventBus()

# Subscribe to events
async def on_tool_call(event: Event):
    print(f"Tool called: {event.data}")

bus.subscribe(EventType.TOOL_CALL, on_tool_call)

# Emit events
await bus.emit(Event(EventType.TOOL_CALL, {"tool": "edit", "file": "main.py"}))
```

### Shadow Git

Atomic commits with rollback:

```python
from moss import ShadowGit

git = ShadowGit(Path("."))

# Create a branch for work
branch = await git.create_branch("feature/add-auth")

# Make changes and commit
handle = await git.commit("Add authentication module")

# Rollback if needed
await git.rollback(handle.sha)
```

### Validators

Chain validators for verification:

```python
from moss import create_python_validator_chain, SyntaxValidator

# Use built-in chain
chain = create_python_validator_chain()

# Or build custom
chain = ValidatorChain([
    SyntaxValidator(),
    RuffValidator(),
    PytestValidator(),
])

result = await chain.validate(Path("src/main.py"))
if not result.passed:
    print(f"Validation failed: {result.issues}")
```

### Policy Engine

Enforce safety rules:

```python
from moss import create_default_policy_engine

engine = create_default_policy_engine()

# Check if action is allowed
result = await engine.check("edit", target=Path("src/main.py"))
if not result.allowed:
    print(f"Blocked by {result.blocking_result.policy_name}")
```

### Code Synthesis

Plugin-based code generation:

```python
from moss.synthesis import SynthesisFramework, Specification
from moss.synthesis.plugins import get_synthesis_registry

# Get the global registry (discovers plugins automatically)
registry = get_synthesis_registry()

# Create the framework
framework = SynthesisFramework()

# Define what to generate
spec = Specification(
    name="validate_email",
    description="Validate email address format",
    type_signature="(email: str) -> bool",
    language="python",
)

# Synthesize code
result = await framework.solve(spec)
if result.success:
    print(result.code)
```

#### Built-in Generators

| Generator | Description |
|-----------|-------------|
| `placeholder` | Returns TODO placeholders (safe fallback) |
| `template` | User-configurable templates for common patterns |
| `llm` | LLM-based generation via LiteLLM (Claude, GPT, etc.) |

#### LLM Generator

```python
from moss.synthesis.plugins.generators import create_llm_generator

# Create with real LLM provider (requires litellm)
generator = create_llm_generator(model="claude-sonnet-4-20250514")

# Or use mock for testing
from moss.synthesis.plugins.generators import create_mock_generator
generator = create_mock_generator()
```

## Development

```bash
# Enter dev shell
nix develop

# Install dependencies
uv sync --all-extras

# Run tests
uv run pytest

# Lint
ruff check && ruff format
```

## Documentation

Generate API documentation:

```bash
# Install docs dependencies
uv add pdoc --optional docs

# Generate static HTML docs
python docs/generate.py

# Or serve docs locally with live reload
python docs/generate.py --serve
```

Documentation is generated from docstrings using [pdoc](https://pdoc.dev/).

## Project Structure

```
src/moss/
├── __init__.py      # Public API exports
├── cli.py           # Command-line interface
├── events.py        # Event bus system
├── shadow_git.py    # Git operations
├── handles.py       # Lazy file references
├── views.py         # View providers
├── skeleton.py      # AST skeleton extraction
├── dependencies.py  # Dependency analysis
├── context.py       # Context compilation
├── anchors.py       # Fuzzy anchor matching
├── patches.py       # AST-aware patching
├── validators.py    # Validation chain
├── loop.py          # Silent retry loop
├── policy.py        # Safety policies
├── memory.py        # Episodic/semantic memory
├── agents.py        # Multi-agent coordination
├── config.py        # Configuration DSL
├── api.py           # API surface
└── synthesis/       # Code synthesis framework
    ├── framework.py     # Main synthesis engine
    ├── strategy.py      # Decomposition strategies
    ├── types.py         # Specification, Context types
    └── plugins/         # Pluggable components
        ├── generators/  # Code generators (placeholder, template, LLM)
        ├── validators/  # Synthesis validators (pytest, type check)
        └── libraries/   # Abstraction libraries (memory, learned)
```

## License

MIT
